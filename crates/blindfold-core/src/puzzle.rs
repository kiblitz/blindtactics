//! The puzzle model and its on-disk form.
//!
//! Stored as JSONL, one puzzle per line: human-readable, greppable, and diffable
//! one-puzzle-at-a-time in review.
//!
//! Note the `fen` here is *not* the Lichess `FEN` column. Lichess stores the
//! position before the opponent's setup move; we store the position actually
//! shown to the user, with that move already applied. The curation tool does the
//! conversion once so nothing downstream has to remember the quirk.

use crate::arrow;
use crate::mate;
use shakmaty::Position as _;

/// A curated, proven-linear mate puzzle.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Puzzle {
    /// The Lichess puzzle id, so it can be traced back upstream.
    pub id: String,
    /// The position shown to the user. Solver is the side to move.
    pub fen: String,
    /// A proven-linear mating line. Not the only one that may exist; correctness
    /// is decided by playing the user's line out, never by comparing to this.
    pub solution: Vec<arrow::Arrow>,
    /// Solver moves to mate. Equal to `solution.len()`, and proven minimal: no
    /// shorter linear mate exists from `fen`.
    pub depth: usize,
    /// Lichess crowd rating, kept for difficulty ordering.
    pub rating: u32,
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("could not parse FEN `{fen}`: {message}")]
    Fen { fen: String, message: String },
    #[error("FEN `{fen}` is not a legal position: {message}")]
    Position { fen: String, message: String },
}

impl Puzzle {
    /// The position shown to the user.
    pub fn position(&self) -> Result<shakmaty::Chess, Error> {
        position_of_fen(&self.fen)
    }

    /// Whose turn it is to find the mate.
    pub fn solver(&self) -> Result<shakmaty::Color, Error> {
        Ok(self.position()?.turn())
    }

    /// Re-prove this puzzle from scratch: the stored solution really is linear,
    /// really mates, and really is minimal.
    ///
    /// Every puzzle in `database/` is run through this in CI, so a corrupt or
    /// mislabelled entry cannot reach the app.
    pub fn verify(&self) -> Result<(), Invalid> {
        let pos = self.position().map_err(Invalid::Unparseable)?;

        if self.depth != self.solution.len() {
            return Err(Invalid::DepthMismatch {
                depth: self.depth,
                solution: self.solution.len(),
            });
        }

        match mate::judge(&pos, &self.solution) {
            mate::Verdict::Mates { plies } if plies == self.depth => {}
            mate::Verdict::Mates { plies } => {
                return Err(Invalid::ShorterThanClaimed {
                    claimed: self.depth,
                    actual: plies,
                })
            }
            mate::Verdict::Refuted { defense, reason } => {
                return Err(Invalid::NotLinear { defense, reason })
            }
        }

        // Minimality. A puzzle advertised as mate-in-4 that is really a linear
        // mate-in-2 would ask the user for arrows the position does not need.
        match mate::min_depth(&pos, self.depth) {
            Some(d) if d == self.depth => Ok(()),
            Some(d) => Err(Invalid::NotMinimal {
                claimed: self.depth,
                actual: d,
            }),
            None => unreachable!("the solution above already mates in `depth`"),
        }
    }
}

/// Why a stored puzzle failed re-verification.
#[derive(Clone, Debug, thiserror::Error)]
pub enum Invalid {
    #[error("position does not parse: {0}")]
    Unparseable(Error),
    #[error("declared depth {depth} but the solution has {solution} moves")]
    DepthMismatch { depth: usize, solution: usize },
    #[error("solution is not linear: {reason}")]
    NotLinear {
        defense: Vec<arrow::Arrow>,
        reason: mate::Reason,
    },
    #[error("claimed mate in {claimed} but the stored line mates in {actual}")]
    ShorterThanClaimed { claimed: usize, actual: usize },
    #[error("claimed mate in {claimed} but a linear mate in {actual} exists")]
    NotMinimal { claimed: usize, actual: usize },
}

/// Parse a FEN into a legal position.
pub fn position_of_fen(fen: &str) -> Result<shakmaty::Chess, Error> {
    let parsed: shakmaty::fen::Fen =
        fen.parse()
            .map_err(|e: shakmaty::fen::ParseFenError| Error::Fen {
                fen: fen.to_owned(),
                message: e.to_string(),
            })?;
    parsed
        .into_position(shakmaty::CastlingMode::Standard)
        .map_err(
            |e: shakmaty::PositionError<shakmaty::Chess>| Error::Position {
                fen: fen.to_owned(),
                message: e.to_string(),
            },
        )
}

/// Read a JSONL puzzle file.
pub fn parse_jsonl(contents: &str) -> Result<Vec<Puzzle>, serde_json::Error> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(serde_json::from_str)
        .collect()
}

/// Write puzzles as JSONL.
pub fn to_jsonl(puzzles: &[Puzzle]) -> Result<String, serde_json::Error> {
    let mut out = String::new();
    for p in puzzles {
        out.push_str(&serde_json::to_string(p)?);
        out.push('\n');
    }
    Ok(out)
}
