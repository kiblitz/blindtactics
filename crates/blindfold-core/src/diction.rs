//! Spoken input: a heard phrase, turned into a move.
//!
//! This is the input half of voice mode, and the most bug-prone part of the project
//! — so it lives in core, is a pure function of its arguments, and is tested hard.
//! Two stages, deliberately split:
//!
//! - [`parse`] turns a raw speech-recognition transcript into an [`Intent`]: a move
//!   spoken in algebraic notation, a castle, an app [`Command`], or [`Intent::Unclear`].
//!   It knows nothing about any position — it is string work only.
//! - [`resolve`] checks a move-shaped intent against a concrete position and returns a
//!   single [`arrow::Arrow`], or a reason it could not: the move was ambiguous, a
//!   promotion piece was not named, a castle side was not named, or nothing legal fit.
//!
//! # The two grammar rules, both load-bearing
//!
//! The user speaks standard algebraic notation, and two rules govern how forgiving we
//! are — set by the user and not to be softened:
//!
//! - **Never penalise extra information.** A full from-square when the move needs no
//!   disambiguation, a spoken "takes" / "check" / "mate" — all accepted, none required.
//!   Extra words are dropped, never turned into a rejection.
//! - **Never penalise missing information, but never auto-resolve it either.** An
//!   ambiguous "knight f6" with two knights available must *ask* which — returning
//!   [`Resolution::Ambiguous`] with the candidate squares — because guessing would hand
//!   the user the answer and rejecting would punish a legal intent. Same for a promotion
//!   with no piece named ([`Resolution::NeedsPromotion`]) and a bare "castle" with both
//!   sides legal ([`Resolution::NeedsCastleSide`]).
//!
//! # Why parsing is fuzzy
//!
//! A speech recogniser is tuned for prose, not chess. "knight" comes back as "night",
//! "rook" as "rock", and a coordinate like "f6" arrives glued ("f6"), spaced ("f 6"),
//! or spelled ("eff six"). So [`parse`] maps generously against homophone tables and
//! recovers the move's *structure* rather than trusting exact spelling: the destination
//! is the last file-and-rank spoken, any earlier file or rank is disambiguation, a role
//! before the destination is the mover and a role after it is a promotion. A phrase it
//! cannot make chess-shaped becomes [`Intent::Unclear`] — never a wrong move.

use crate::arrow;
use crate::constants;
use shakmaty::Position as _;

/// A spoken instruction that drives the app rather than making a move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    /// Play the drawn line out and score it.
    Submit,
    /// Remove the last move.
    Undo,
    /// Remove every move.
    Clear,
    /// Move on to the next puzzle.
    Next,
    /// Read the roster out again.
    Repeat,
    /// Reveal the solution, scored as a loss.
    GiveUp,
}

/// A move as spoken: a destination, plus whatever else the speaker chose to say.
///
/// Everything but the destination is optional — the "never penalise missing
/// information" half of the grammar. `role` is `None` for a bare-square pawn move
/// (`"e4"`), exactly as algebraic notation omits the piece letter for a pawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Move {
    /// The moving piece; `None` means a pawn.
    pub role: Option<shakmaty::Role>,
    /// A disambiguating from-file, if one was spoken.
    pub from_file: Option<shakmaty::File>,
    /// A disambiguating from-rank, if one was spoken.
    pub from_rank: Option<shakmaty::Rank>,
    /// The square the piece is moving to.
    pub to: shakmaty::Square,
    /// The piece a promotion was declared for, if any.
    pub promotion: Option<shakmaty::Role>,
}

/// What an utterance was understood to mean, before any position is consulted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intent {
    /// A move, spoken in algebraic notation.
    Move(Move),
    /// A castle; the side is `None` when the speaker did not name one.
    Castle(Option<shakmaty::CastlingSide>),
    /// An app command.
    Command(Command),
    /// Nothing chess-shaped was recognised.
    Unclear,
}

/// The outcome of checking a move-shaped [`Intent`] against a concrete position.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Resolution {
    /// Exactly one legal move fits: play this arrow.
    Move(arrow::Arrow),
    /// More than one piece can make the move. The speaker must disambiguate by
    /// from-square; the candidate from-squares are carried here, in board order, so
    /// the app can ask "which one — d5 or f5?".
    Ambiguous(Vec<shakmaty::Square>),
    /// A pawn reaches the last rank but no promotion piece was named. Carries the
    /// destination so the app can ask "promote to what?".
    NeedsPromotion(shakmaty::Square),
    /// A bare "castle" with both sides legal: the speaker must say which.
    NeedsCastleSide,
    /// Nothing legal fits what was said.
    Illegal,
}

/// Turn a raw transcript into an [`Intent`]. Pure string work — no position.
pub fn parse(transcript: &str) -> Intent {
    let words: Vec<Word> = transcript
        .split(|c: char| !c.is_ascii_alphanumeric())
        .flat_map(split_letters_from_digits)
        .map(|w| classify(&w))
        .collect();

    // A castle is its own grammar: "castle", "castles kingside", "long castle". The
    // side words are checked *before* the move path so "king side" cannot be misread as
    // a king move.
    if words.iter().any(|w| matches!(w, Word::Castle)) {
        let side = words.iter().find_map(|w| match w {
            Word::Side(side) => Some(*side),
            _ => None,
        });
        return Intent::Castle(side);
    }

    // The destination is the last file and the last rank spoken; earlier ones
    // disambiguate. Roles are split by whether they come before the destination (the
    // mover) or after it (a promotion), which is what tells "queen e8" (a queen moves)
    // from "e8 queen" (a pawn promotes).
    let files: Vec<shakmaty::File> = words.iter().filter_map(Word::file).collect();
    let ranks: Vec<shakmaty::Rank> = words.iter().filter_map(Word::rank).collect();

    let (Some(&to_file), Some(&to_rank)) = (files.last(), ranks.last()) else {
        // No destination, so this is not a move. A command word alone stands here.
        return match words.iter().find_map(Word::command) {
            Some(command) => Intent::Command(command),
            None => Intent::Unclear,
        };
    };

    let last_coord = words
        .iter()
        .rposition(|w| w.file().is_some() || w.rank().is_some())
        .expect("a destination file and rank were just found");
    let first_coord = words
        .iter()
        .position(|w| w.file().is_some() || w.rank().is_some())
        .expect("a destination file and rank were just found");

    let role = words[..first_coord].iter().find_map(Word::role);
    let promotion = words[last_coord + 1..]
        .iter()
        .find_map(Word::role)
        .filter(|r| constants::PROMOTABLE.contains(r));

    Intent::Move(Move {
        role,
        // The disambiguators are the file and rank *before* the destination's.
        from_file: nth_from_last(&files, 1),
        from_rank: nth_from_last(&ranks, 1),
        to: shakmaty::Square::from_coords(to_file, to_rank),
        promotion,
    })
}

/// Check a move-shaped intent against `pos`. `None` for a [`Command`] or
/// [`Intent::Unclear`], which do not resolve to a move.
pub fn resolve(intent: &Intent, pos: &shakmaty::Chess) -> Option<Resolution> {
    match intent {
        Intent::Move(spoken) => Some(resolve_move(spoken, pos)),
        Intent::Castle(side) => Some(resolve_castle(*side, pos)),
        Intent::Command(_) | Intent::Unclear => None,
    }
}

fn resolve_move(spoken: &Move, pos: &shakmaty::Chess) -> Resolution {
    let mover = spoken.role.unwrap_or(shakmaty::Role::Pawn);
    let matched: Vec<shakmaty::Move> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| {
            m.role() == mover
                && m.to() == spoken.to
                && spoken
                    .from_file
                    .is_none_or(|f| m.from().is_some_and(|s| s.file() == f))
                && spoken
                    .from_rank
                    .is_none_or(|r| m.from().is_some_and(|s| s.rank() == r))
        })
        .collect();

    if matched.is_empty() {
        return Resolution::Illegal;
    }

    // Distinct from-squares decide ambiguity: one piece, or several that could all do it.
    let mut froms: Vec<shakmaty::Square> = matched.iter().filter_map(|m| m.from()).collect();
    froms.sort_unstable();
    froms.dedup();
    let [from] = froms[..] else {
        return Resolution::Ambiguous(froms);
    };

    // A promotion shows up as several legal moves sharing one from/to. If the speaker
    // named the piece, use it; if not, ask. A stray promotion word on a non-promoting
    // move is extra information, so it is dropped rather than made illegal.
    if matched.iter().any(|m| m.promotion().is_some()) {
        return match spoken.promotion {
            Some(role) => Resolution::Move(arrow::Arrow::promoting(from, spoken.to, role)),
            None => Resolution::NeedsPromotion(spoken.to),
        };
    }
    Resolution::Move(arrow::Arrow::new(from, spoken.to))
}

fn resolve_castle(side: Option<shakmaty::CastlingSide>, pos: &shakmaty::Chess) -> Resolution {
    let castles: Vec<(shakmaty::CastlingSide, arrow::Arrow)> = pos
        .legal_moves()
        .into_iter()
        .filter_map(|m| match &m {
            shakmaty::Move::Castle { king, rook } => {
                let side = if rook.file() > king.file() {
                    shakmaty::CastlingSide::KingSide
                } else {
                    shakmaty::CastlingSide::QueenSide
                };
                // `of_move` canonicalises a castle to the king's travel (`e1g1`), which
                // is the arrow a drag would have produced — so voice and drawing agree.
                arrow::Arrow::of_move(&m).map(|a| (side, a))
            }
            _ => None,
        })
        .collect();

    match side {
        Some(side) => castles
            .iter()
            .find(|(s, _)| *s == side)
            .map_or(Resolution::Illegal, |(_, a)| Resolution::Move(*a)),
        None => match castles[..] {
            [] => Resolution::Illegal,
            [(_, a)] => Resolution::Move(a),
            _ => Resolution::NeedsCastleSide,
        },
    }
}

/// The element `n` places from the end (`0` is the last), if the slice is long enough.
/// Used to pick the disambiguating file/rank — the one before the destination's.
fn nth_from_last<T: Copy>(items: &[T], n: usize) -> Option<T> {
    items.len().checked_sub(n + 1).map(|i| items[i])
}

/// A single classified word from the transcript.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Word {
    Role(shakmaty::Role),
    File(shakmaty::File),
    Rank(shakmaty::Rank),
    Castle,
    Side(shakmaty::CastlingSide),
    Command(Command),
    /// A filler word, or one that carries no move information — dropped.
    Noise,
}

impl Word {
    fn file(&self) -> Option<shakmaty::File> {
        match self {
            Word::File(f) => Some(*f),
            _ => None,
        }
    }

    fn rank(&self) -> Option<shakmaty::Rank> {
        match self {
            Word::Rank(r) => Some(*r),
            _ => None,
        }
    }

    fn role(&self) -> Option<shakmaty::Role> {
        match self {
            Word::Role(r) => Some(*r),
            _ => None,
        }
    }

    fn command(&self) -> Option<Command> {
        match self {
            Word::Command(c) => Some(*c),
            _ => None,
        }
    }
}

/// Break one token at letter/digit boundaries, so a glued coordinate like `"f6"`
/// becomes `["f", "6"]` while a word like `"queen"` is left whole. The recogniser
/// returns coordinates both ways, and downstream only ever wants the pieces apart.
fn split_letters_from_digits(token: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in token.chars() {
        let start_new = match out.last().and_then(|s| s.chars().next()) {
            Some(prev) => prev.is_ascii_digit() != c.is_ascii_digit(),
            None => true,
        };
        if start_new {
            out.push(String::new());
        }
        out.last_mut().expect("just pushed if empty").push(c);
    }
    out
}

fn classify(word: &str) -> Word {
    let w = word.to_ascii_lowercase();
    // Order matters: a bare "a"–"h" is a file, but the number and role words are
    // checked first so nothing shadows them.
    if let Some(rank) = rank_word(&w) {
        return Word::Rank(rank);
    }
    if let Some(role) = role_word(&w) {
        return Word::Role(role);
    }
    if let Some(command) = command_word(&w) {
        return Word::Command(command);
    }
    if let Some(file) = file_word(&w) {
        return Word::File(file);
    }
    match w.as_str() {
        "castle" | "castles" | "castling" | "castled" => Word::Castle,
        "kingside" | "short" => Word::Side(shakmaty::CastlingSide::KingSide),
        "queenside" | "long" => Word::Side(shakmaty::CastlingSide::QueenSide),
        _ => Word::Noise,
    }
}

/// A rank from a spoken digit (`"6"`) or number word (`"six"`). Prepositions that
/// sound like numbers — "to"/"too", "for" — are deliberately *not* mapped, since
/// "rook to f8" says "to" as a connective, not the rank 2.
fn rank_word(word: &str) -> Option<shakmaty::Rank> {
    let index = match word {
        "1" | "one" | "won" => 0,
        "2" | "two" => 1,
        "3" | "three" => 2,
        "4" | "four" => 3,
        "5" | "five" => 4,
        "6" | "six" => 5,
        "7" | "seven" => 6,
        "8" | "eight" => 7,
        _ => return None,
    };
    Some(shakmaty::Rank::new(index))
}

/// A file from a spoken single letter (`"f"`, split out of `"f6"`) or its spoken
/// name (`"eff"`). The homophones are the ones a recogniser actually returns; the
/// list stays conservative so a stray word does not become a phantom file.
fn file_word(word: &str) -> Option<shakmaty::File> {
    let index = match word {
        "a" | "ay" => 0,
        "b" | "bee" | "be" => 1,
        "c" | "cee" | "see" | "sea" => 2,
        "d" | "dee" => 3,
        "e" | "ee" => 4,
        "f" | "ef" | "eff" => 5,
        "g" | "gee" | "jee" => 6,
        "h" | "aitch" | "haitch" => 7,
        _ => return None,
    };
    Some(shakmaty::File::new(index))
}

/// A moving piece from its name, including the homophones a recogniser hears —
/// "night" for knight, "rock" for rook.
fn role_word(word: &str) -> Option<shakmaty::Role> {
    Some(match word {
        "king" | "kings" => shakmaty::Role::King,
        "queen" | "queens" | "quinn" => shakmaty::Role::Queen,
        "rook" | "rooks" | "rock" | "ruck" => shakmaty::Role::Rook,
        "bishop" | "bishops" | "bish" => shakmaty::Role::Bishop,
        "knight" | "knights" | "night" | "nights" | "nite" => shakmaty::Role::Knight,
        "pawn" | "pawns" | "paun" => shakmaty::Role::Pawn,
        _ => return None,
    })
}

fn command_word(word: &str) -> Option<Command> {
    Some(match word {
        "submit" | "done" | "enter" | "confirm" | "go" | "play" => Command::Submit,
        "undo" | "back" | "undone" | "delete" | "remove" | "oops" | "scratch" => Command::Undo,
        "clear" | "reset" | "erase" => Command::Clear,
        "next" | "skip" | "another" | "forward" => Command::Next,
        "repeat" | "again" | "pardon" | "read" => Command::Repeat,
        "resign" | "solution" | "reveal" | "giveup" | "give" => Command::GiveUp,
        _ => return None,
    })
}
