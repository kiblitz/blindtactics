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

use std::fmt;
use std::str::FromStr;

/// Roles a pawn may promote to. Excludes king and pawn, which `Role::from_char`
/// will happily hand back.
const PROMOTABLE: [shakmaty::Role; 4] = [
    shakmaty::Role::Queen,
    shakmaty::Role::Rook,
    shakmaty::Role::Bishop,
    shakmaty::Role::Knight,
];

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
    #[error("expected 4 or 5 characters of UCI, got {0}")]
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
        shakmaty::uci::UciMove::Normal {
            from: self.from,
            to: self.to,
            promotion: self.promotion,
        }
        .to_move(pos)
        .map_err(|_| Error::Illegal)
    }

    /// Whether this arrow is legal in `pos`.
    pub fn is_legal(self, pos: &shakmaty::Chess) -> bool {
        self.resolve(pos).is_ok()
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

impl FromStr for Arrow {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 4 && s.len() != 5 {
            return Err(ParseError::Length(s.len()));
        }
        let square = |t: &str| {
            shakmaty::Square::from_ascii(t.as_bytes()).map_err(|_| ParseError::Square(t.to_owned()))
        };
        let from = square(&s[0..2])?;
        let to = square(&s[2..4])?;
        let promotion = match s.as_bytes().get(4) {
            None => None,
            Some(&c) => {
                // `Role::from_char` accepts 'k' and 'p' too, which are not legal
                // promotions, so the result is filtered rather than trusted.
                let role = shakmaty::Role::from_char(c as char)
                    .filter(|r| PROMOTABLE.contains(r))
                    .ok_or(ParseError::Promotion(c as char))?;
                Some(role)
            }
        };
        Ok(Self {
            from,
            to,
            promotion,
        })
    }
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
