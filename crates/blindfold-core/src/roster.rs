//! Where the pieces are — the only thing the blindfold user is told.
//!
//! Modelled as structured data rather than a display string, deliberately. The
//! same roster has to render three ways: as SVG piece icons beside coordinates, as
//! plain text ([`Roster::text`], "white to play. white: king d5…"), and as speech
//! ([`Roster::speech`], the same but with squares upper-cased for a voice — "king D5",
//! read "king dee five"). Building a string here would force the other renderings to
//! parse it apart.
//!
//! Text and speech rendering live here rather than in the UI crate because they are
//! shared (plain text feeds display and screen readers; speech feeds the read-aloud
//! mode). SVG rendering belongs to the web crate, which is its only consumer.
//!
//! **The roster must carry everything that decides the answer, not just placement.**
//! Chess has two pieces of state that are not "where the pieces are" — castling
//! rights and the en-passant square — and both can be the whole point of a mate.
//! Since the user cannot look at the board, anything missing here is not a
//! presentational gap; it makes the puzzle unsolvable and then marks a correct
//! answer wrong. `tests/roster.rs::roster_distinguishes_positions_whose_answers_differ`
//! is what holds this, and it is the test to extend if chess ever grows a third
//! such thing.

use crate::constants;
use shakmaty::Position as _;

/// Every square holding a given role, for one side.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    pub role: shakmaty::Role,
    /// Sorted by file, then rank — `a6 b7 g5`, the order a person reads them out.
    pub squares: Vec<shakmaty::Square>,
}

/// Which castles a side may still make.
///
/// Rights, not availability: these say the king and rook are unmoved, never that
/// castling is legal right now. A side in check has its rights and cannot use them
/// this ply, which is correct to announce — the check may be parried and the right
/// used later in the line.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Castling {
    pub kingside: bool,
    pub queenside: bool,
}

/// One side's pieces, ordered king first down to pawns, and what it may still do.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Side {
    pub color: shakmaty::Color,
    pub entries: Vec<Entry>,
    pub castling: Castling,
}

/// Everything the user is told: where the pieces are, whose turn it is, and the
/// two bits of chess state that are neither.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Roster {
    pub to_move: shakmaty::Color,
    pub white: Side,
    pub black: Side,
    /// The square an en-passant capture would land on, when one is legal.
    ///
    /// Taken with [`shakmaty::EnPassantMode::Legal`], so this is `Some` only when
    /// the capture can actually be played. That is deliberate on both counts: it
    /// keeps the roster from announcing a right nobody can use, and it is still
    /// sound, because en-passant expires after one ply — a square with no legal
    /// capture *now* can never affect the game, so hiding it cannot hide an answer.
    /// (`EnPassantMode::Always` would report the FEN's square regardless, which is
    /// what the raw Lichess FEN carries.)
    pub en_passant: Option<shakmaty::Square>,
}

/// Read the roster out of a position.
pub fn of(pos: &shakmaty::Chess) -> Roster {
    Roster {
        to_move: pos.turn(),
        white: side_of(pos, shakmaty::Color::White),
        black: side_of(pos, shakmaty::Color::Black),
        en_passant: pos.ep_square(shakmaty::EnPassantMode::Legal),
    }
}

fn side_of(pos: &shakmaty::Chess, color: shakmaty::Color) -> Side {
    let board = pos.board();
    let entries = constants::ANNOUNCE_ORDER
        .iter()
        .filter_map(|&role| {
            let mut squares: Vec<shakmaty::Square> = (board.by_color(color) & board.by_role(role))
                .into_iter()
                .collect();
            if squares.is_empty() {
                return None;
            }
            squares.sort_by_key(|s| (s.file(), s.rank()));
            Some(Entry { role, squares })
        })
        .collect();
    let castles = pos.castles();
    Side {
        color,
        entries,
        castling: Castling {
            kingside: castles.has(color, shakmaty::CastlingSide::KingSide),
            queenside: castles.has(color, shakmaty::CastlingSide::QueenSide),
        },
    }
}

impl Roster {
    /// How many squares the user has to hold in their head.
    ///
    /// The blindfold cost of a puzzle, and the reason this is not simply "pieces on
    /// the board": what makes a puzzle hard to *carry* is the length of the roster
    /// being read out, and that is one item per occupied square. Curation gates on
    /// this — a mate in one is still unusable if finding it means memorizing 32
    /// squares first.
    ///
    /// Castling rights and the en-passant square are deliberately not counted. They
    /// are announced, and they matter to the answer, but they attach to pieces the
    /// user is already holding rather than adding new ones.
    pub fn squares(&self) -> usize {
        [&self.white, &self.black]
            .iter()
            .flat_map(|side| side.entries.iter())
            .map(|entry| entry.squares.len())
            .sum()
    }

    /// The side to move, then the other. Announcing the mover first is what a
    /// human does, and it is what the audio mode will want.
    pub fn sides_in_announce_order(&self) -> [&Side; 2] {
        match self.to_move {
            shakmaty::Color::White => [&self.white, &self.black],
            shakmaty::Color::Black => [&self.black, &self.white],
        }
    }

    /// Render as plain text — for display and for a screen reader.
    ///
    /// `"white to play. white: king d5. bishops b4, c6. pawns a6, b7, g5. black: king g7."`
    ///
    /// Plain coordinates (`a2`), because that is what the eye reads and what a screen
    /// reader spells for itself. Our *own* text-to-speech wants [`speech`](Roster::speech)
    /// instead — see the note there.
    ///
    /// En passant is announced last, as its own sentence, because it is a property
    /// of the position rather than of either side's material — and because a
    /// blindfold user needs it to land after they have both sides' pawns in mind.
    pub fn text(&self) -> String {
        self.render(square_plain)
    }

    /// The same announcement rendered for text-to-speech: each square upper-cased so a
    /// speech engine reads the file as its letter *name*, not as a word.
    ///
    /// Separate from [`text`](Roster::text) deliberately. Handing "a2" to the
    /// browser's `speechSynthesis` gets the lone file letter read as the article "a" —
    /// "ah two" rather than "ay two"; the upper-cased "A2" is the cell reference "ay
    /// two". It is *not* right for a screen reader, which spells coordinates correctly on
    /// its own, which is why the two renderings stay distinct. See [`square_spoken`].
    pub fn speech(&self) -> String {
        self.render(square_spoken)
    }

    /// Shared by [`text`](Roster::text) and [`speech`](Roster::speech): identical
    /// wording and structure, differing only in how a square is spelled.
    fn render(&self, square: fn(shakmaty::Square) -> String) -> String {
        let mut out = format!("{} to play.", color_name(self.to_move));
        for side in self.sides_in_announce_order() {
            out.push(' ');
            out.push_str(&side.render(square));
        }
        if let Some(sq) = self.en_passant {
            out.push_str(&format!(" en passant on {}.", square(sq)));
        }
        out
    }
}

impl Side {
    /// `"white: king e1. queen f4. rooks a1, c2. may castle queenside."`
    pub fn text(&self) -> String {
        self.render(square_plain)
    }

    fn render(&self, square: fn(shakmaty::Square) -> String) -> String {
        let mut listed: Vec<String> = self.entries.iter().map(|e| e.render(square)).collect();
        if let Some(castling) = self.castling.text() {
            listed.push(castling.to_owned());
        }
        // Piece types are separated by ". " (a full stop), the squares *within* a type
        // by ", " — so a read-aloud voice pauses hardest between the roles it is
        // listing and only lightly between the squares of one role. The same grouping
        // reads just as clearly for a screen reader, which is why it lives in the
        // shared render rather than only in `speech`.
        format!(
            "{}: {}.",
            color_name(self.color),
            listed.join(constants::ROSTER_TYPE_SEP)
        )
    }
}

impl Castling {
    /// `None` when the side has no rights left, which is the common case and
    /// deserves silence rather than "may castle neither side".
    pub fn text(self) -> Option<&'static str> {
        match (self.kingside, self.queenside) {
            (true, true) => Some("may castle either side"),
            (true, false) => Some("may castle kingside"),
            (false, true) => Some("may castle queenside"),
            (false, false) => None,
        }
    }
}

impl Entry {
    /// `"pawns a6, b7, g5"`, or `"king d5"` for a lone piece.
    pub fn text(&self) -> String {
        self.render(square_plain)
    }

    fn render(&self, square: fn(shakmaty::Square) -> String) -> String {
        let squares: Vec<String> = self.squares.iter().map(|s| square(*s)).collect();
        format!(
            "{} {}",
            self.name(),
            squares.join(constants::ROSTER_SQUARE_SEP)
        )
    }

    /// The role's name, pluralized to match the number of squares.
    ///
    /// A king is always one square (`tests/roster.rs::both_kings_are_always_present`),
    /// so it never reaches the plural arm.
    pub fn name(&self) -> &'static str {
        name(self.role, self.squares.len() > 1)
    }
}

/// A square as plain coordinates — `a2`. What the eye and a screen reader want.
fn square_plain(square: shakmaty::Square) -> String {
    square.to_string()
}

/// A square spelled for text-to-speech: the coordinate uppercased — `b7` → `"B7"`,
/// `a2` → `"A2"`, `e3` → `"E3"`. The spreadsheet-cell form, which a speech engine reads
/// as a tight "letter name + number" ("bee seven", "ay two", "ee three").
///
/// Two things it must get right, and one empirical reversal behind them:
///
/// - **Letter name, not the article.** A lone lower-case `a` is read as the word "a"
///   ("ah"), so `"a2"` comes out "ah two". Upper-casing drops the engine into
///   letter-name reading — `"A2"` is the cell reference "ay two". The rank digit reads
///   correctly on its own either way.
/// - **No separation.** The file and rank stay glued into one token so they are spoken
///   as a unit. This is the fix for a real complaint: an earlier form spelled each file
///   as an *initial with a full stop* (`"A. 2"`) to force letter-name reading, but the
///   full stop is a sentence break — it split "a2" into "ay … two" with a long pause
///   between, and on the neural TTS→STT loop the `a`-file's period form was mis-voiced
///   badly enough to be heard as "eight three" rather than "A three". The glued
///   upper-case coordinate reads cleanly on that loop across voices, with no gap, and
///   the pause is gone.
///
/// Still *not* right for a screen reader, which spells coordinates correctly on its own
/// and wants the plain [`text`](Roster::text); the two renderings stay distinct. Read
/// off the square's own `Display` (`"a1"`), so it cannot disagree with how a square prints.
pub fn square_spoken(square: shakmaty::Square) -> String {
    square.to_string().to_ascii_uppercase()
}

/// What a role is called when it is announced.
///
/// The vocabulary the roster reads out, and the one the app labels its promotion
/// picker with — a picker whose knight said anything but "knight" would be
/// teaching a second name for a piece the roster has already named. It is also
/// what the audio mode will speak. One list, three consumers, which is the whole
/// reason text rendering lives in core rather than in the UI.
pub fn name(role: shakmaty::Role, plural: bool) -> &'static str {
    match (role, plural) {
        (shakmaty::Role::King, false) => "king",
        (shakmaty::Role::King, true) => "kings",
        (shakmaty::Role::Queen, false) => "queen",
        (shakmaty::Role::Queen, true) => "queens",
        (shakmaty::Role::Rook, false) => "rook",
        (shakmaty::Role::Rook, true) => "rooks",
        (shakmaty::Role::Bishop, false) => "bishop",
        (shakmaty::Role::Bishop, true) => "bishops",
        (shakmaty::Role::Knight, false) => "knight",
        (shakmaty::Role::Knight, true) => "knights",
        (shakmaty::Role::Pawn, false) => "pawn",
        (shakmaty::Role::Pawn, true) => "pawns",
    }
}

/// What a side is called when it is announced.
///
/// Public for the same reason [`name`] is: the roster reads "white to play", the
/// panel heads its list "White", and the audio mode will speak one of them. Three
/// consumers, and while this was private the web crate simply re-typed the pair —
/// twice — so the argument the roster makes for roles was quietly not made for
/// colours.
///
/// Lower case, because the roster's sentences are lower case and a caller
/// wanting a heading can capitalize — see [`heading`]. The reverse is not true.
pub fn color_name(color: shakmaty::Color) -> &'static str {
    match color {
        shakmaty::Color::White => "white",
        shakmaty::Color::Black => "black",
    }
}

/// The capitalized side name, to start a sentence or head a list: "White",
/// "Black".
///
/// Derived from [`color_name`] rather than a second table, so the two cannot
/// disagree about the word — the panel renders "white to play" and a "White"
/// heading into the same DOM, and one source is what keeps them the same word.
/// Lives here rather than in the web crate because it is pure text the audio mode
/// will also speak, which is the same line [`name`] and [`color_name`] sit on.
pub fn heading(color: shakmaty::Color) -> String {
    let name = color_name(color);
    let mut chars = name.chars();
    match chars.next() {
        // `color_name` is ASCII and non-empty, so this capitalizes rather than
        // pretending to be a general title-caser.
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
