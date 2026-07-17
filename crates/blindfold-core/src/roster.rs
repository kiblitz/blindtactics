//! Where the pieces are — the only thing the blindfold user is told.
//!
//! Modelled as structured data rather than a display string, deliberately. The
//! same roster has to render three ways: as SVG piece icons beside coordinates,
//! as plain text, and (later) as speech — "white to play. white: king d5, bishops
//! b4 c6, pawns a6 b7 g5. black: king g7." Building a string here would force the
//! audio mode to parse it back apart.
//!
//! Text rendering lives here rather than in the UI crate because two consumers
//! share it (plain text and speech). SVG rendering belongs to the web crate,
//! which is its only consumer.
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

    /// Render for reading aloud or displaying as plain text.
    ///
    /// `"white to play. white: king d5, bishops b4 c6, pawns a6 b7 g5. black: king g7."`
    ///
    /// En passant is announced last, as its own sentence, because it is a property
    /// of the position rather than of either side's material — and because a
    /// blindfold user needs it to land after they have both sides' pawns in mind.
    pub fn text(&self) -> String {
        let mut out = format!("{} to play.", color_name(self.to_move));
        for side in self.sides_in_announce_order() {
            out.push(' ');
            out.push_str(&side.text());
        }
        if let Some(square) = self.en_passant {
            out.push_str(&format!(" en passant on {square}."));
        }
        out
    }
}

impl Side {
    /// `"white: king e1, queen f4, rooks a1 c2, may castle queenside."`
    pub fn text(&self) -> String {
        let mut listed: Vec<String> = self.entries.iter().map(Entry::text).collect();
        if let Some(castling) = self.castling.text() {
            listed.push(castling.to_owned());
        }
        format!("{}: {}.", color_name(self.color), listed.join(", "))
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
    /// `"pawns a6 b7 g5"`, or `"king d5"` for a lone piece.
    pub fn text(&self) -> String {
        let squares: Vec<String> = self.squares.iter().map(|s| s.to_string()).collect();
        format!("{} {}", self.name(), squares.join(" "))
    }

    /// The role's name, pluralized to match the number of squares.
    ///
    /// A king is always one square (`tests/roster.rs::both_kings_are_always_present`),
    /// so it never reaches the plural arm.
    pub fn name(&self) -> &'static str {
        name(self.role, self.squares.len() > 1)
    }
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
