//! The user's unit of input: a drag across the blank board, from one square to
//! another.
//!
//! # Why arrows are not `shakmaty::Move`s
//!
//! This is the load-bearing idea of the whole project, so it is worth stating
//! plainly. An [`Arrow`] is identified by `(from, to, promotion)` and *nothing
//! else*. A `shakmaty::Move` additionally carries the moving role and whether the
//! move captured.
//!
//! Those extra fields are position-dependent. Consider a rook lift to d8 that
//! mates: against one defense d8 is empty (a quiet move), against another the
//! defender has interposed and d8 is occupied (a capture). It is the *same drag*
//! by the user, but two different `Move` values.
//!
//! Since a blindfold user commits to their whole line before seeing any reply,
//! the thing they commit to is the drag — the arrow. If linearity were defined
//! over `Move` equality, puzzles like the above would be wrongly rejected, and
//! the app would wrongly refuse a correct answer. So: arrows are the identity,
//! and they are *resolved* against a concrete position only when played.
//!
//! The invariant that makes this work is `of_move(resolve(a)) == a`, pinned by
//! `tests/arrow.rs::resolve_and_of_move_round_trip`.
//!
//! It holds for every arrow a user can draw, with one deliberate exception: a
//! castle has two UCI spellings. `resolve` accepts both `e1g1` (the king's
//! travel, which is what a drag produces) and `e1h1` (the king-takes-rook form
//! shakmaty stores internally), but `of_move` can only return one, and returns
//! `e1g1`. So `e1h1` survives the round trip as `e1g1`. This is canonicalization,
//! not drift — both spell the same `Move::Castle` — and `search` depends on it to
//! avoid offering the same castle twice.

use crate::constants;
use std::fmt;

/// A drag from one square to another, optionally promoting.
///
/// Serialized as UCI (`"e2e4"`, `"e7e8q"`) so the on-disk puzzle database stays
/// human-readable and diffable.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Arrow {
    pub from: shakmaty::Square,
    pub to: shakmaty::Square,
    pub promotion: Option<shakmaty::Role>,
}

/// Why an arrow could not be turned into a legal move in a given position.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum Error {
    #[error("no legal move matches this arrow in the given position")]
    Illegal,
}

/// Why a string could not be parsed as an arrow.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum ParseError {
    /// Counted in bytes, because that is what the parser actually gates on.
    /// Every arrow that could ever be valid is pure ASCII, so for anything the
    /// user can usefully be told about, bytes and characters agree. They diverge
    /// only on input that was never going to parse — and reporting characters
    /// there produces a self-refuting message: `"é2é4"` is four characters, so
    /// "expected 4 or 5, got 4" would be the complaint about a rejected string.
    #[error("expected 4 or 5 bytes of UCI, got {0}")]
    Length(usize),
    #[error("`{0}` is not a square")]
    Square(String),
    #[error("`{0}` is not a promotable piece")]
    Promotion(char),
}

impl Arrow {
    pub fn new(from: shakmaty::Square, to: shakmaty::Square) -> Self {
        Self {
            from,
            to,
            promotion: None,
        }
    }

    pub fn promoting(from: shakmaty::Square, to: shakmaty::Square, role: shakmaty::Role) -> Self {
        Self {
            from,
            to,
            promotion: Some(role),
        }
    }

    /// Find the legal move in `pos` that this arrow denotes, if any.
    ///
    /// Delegates to shakmaty's UCI resolution, which is doing real work for us on
    /// castling. Internally a castle is `Castle { king: E1, rook: H1 }`, so its
    /// raw `Move::to()` is the *rook's* square, not the king's; `to_move` accepts
    /// both the king-travel spelling (`e1g1`) and the king-takes-rook spelling
    /// (`e1h1`) and resolves them to the same move.
    ///
    /// Both spellings matter here. A user dragging their king two squares means
    /// "castle", a user dragging their king onto their own rook also means
    /// "castle", and the Lichess database emits the latter form in places (lila
    /// carries an `altCastles` table to undo it).
    pub fn resolve(self, pos: &shakmaty::Chess) -> Result<shakmaty::Move, Error> {
        // shakmaty builds `Move::EnPassant` without consulting the promotion
        // suffix, so `e5d6q` would otherwise resolve to the very same move as
        // `e5d6` while comparing unequal as an `Arrow` — breaking this module's
        // central claim that the triple is the identity. A capture that promotes
        // must land on a back rank, so anything else carrying a suffix is
        // rejected before delegating.
        if self.promotion.is_some() && !self.lands_on_back_rank() {
            return Err(Error::Illegal);
        }

        shakmaty::uci::UciMove::Normal {
            from: self.from,
            to: self.to,
            promotion: self.promotion,
        }
        .to_move(pos)
        .map_err(|_| Error::Illegal)
    }

    fn lands_on_back_rank(self) -> bool {
        matches!(
            self.to.rank(),
            shakmaty::Rank::First | shakmaty::Rank::Eighth
        )
    }

    /// The arrow a user would have drawn to play `m`.
    ///
    /// Goes through `to_uci(CastlingMode::Standard)` rather than reading
    /// `Move::from()`/`to()` directly, so a castle comes back as the king's
    /// travel (`e1g1`) — what was actually dragged — instead of the rook square.
    ///
    /// `None` for drops, which standard chess has no notion of.
    pub fn of_move(m: &shakmaty::Move) -> Option<Self> {
        match m.to_uci(shakmaty::CastlingMode::Standard) {
            shakmaty::uci::UciMove::Normal {
                from,
                to,
                promotion,
            } => Some(Self {
                from,
                to,
                promotion,
            }),
            shakmaty::uci::UciMove::Put { .. } | shakmaty::uci::UciMove::Null => None,
        }
    }
}

impl fmt::Display for Arrow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.from, self.to)?;
        if let Some(role) = self.promotion {
            write!(f, "{}", role.char())?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Arrow {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Parsed over bytes throughout. Slicing the `&str` at fixed offsets
        // panics on multi-byte input, because the arity check counts bytes while
        // the slice indices are char boundaries — a 4-byte emoji passes the gate
        // and then splits a codepoint. Matching on the byte slice makes the arity
        // structural and removes the indices entirely.
        match *s.as_bytes() {
            [a, b, c, d] => Ok(Self {
                from: square([a, b])?,
                to: square([c, d])?,
                promotion: None,
            }),
            [a, b, c, d, p] => Ok(Self {
                from: square([a, b])?,
                to: square([c, d])?,
                promotion: Some(promotion(p)?),
            }),
            _ => Err(ParseError::Length(s.len())),
        }
    }
}

fn square(bytes: [u8; 2]) -> Result<shakmaty::Square, ParseError> {
    shakmaty::Square::from_ascii(&bytes)
        .map_err(|_| ParseError::Square(String::from_utf8_lossy(&bytes).into_owned()))
}

fn promotion(byte: u8) -> Result<shakmaty::Role, ParseError> {
    let c = byte as char;
    // Two things `Role::from_char` will not do for us: it accepts 'k' and 'p',
    // which are not promotable, and it accepts uppercase, which is not UCI and
    // would make parsing non-round-trip-stable since `Display` emits lowercase.
    if !byte.is_ascii_lowercase() {
        return Err(ParseError::Promotion(c));
    }
    shakmaty::Role::from_char(c)
        .filter(|r| constants::PROMOTABLE.contains(r))
        .ok_or(ParseError::Promotion(c))
}

impl TryFrom<String> for Arrow {
    type Error = ParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Arrow> for String {
    fn from(a: Arrow) -> String {
        a.to_string()
    }
}
