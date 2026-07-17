//! The puzzle model and its on-disk form.
//!
//! Stored as JSONL, one puzzle per line: human-readable, greppable, and diffable
//! one-puzzle-at-a-time in review.
//!
//! Note the `fen` here is *not* the Lichess `FEN` column. Lichess stores the
//! position before the opponent's setup move; we store the position actually
//! shown to the user, with that move already applied. The curation tool will do
//! that conversion once, so nothing downstream has to remember the quirk.

use crate::arrow;
use crate::constants;
use crate::mate;
use crate::position;
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
    /// Solver moves to mate. Proven minimal: no shorter linear mate exists from
    /// `fen`.
    ///
    /// Redundant with `solution.len()`, and kept anyway: the database is
    /// committed JSONL that we want to slice with grep (`"depth":3`) without
    /// running a parser. `verify` checks the two agree.
    pub depth: usize,
    /// Lichess crowd rating, kept for difficulty ordering within a depth.
    pub rating: u32,
}

impl Puzzle {
    /// The position shown to the user.
    pub fn position(&self) -> Result<shakmaty::Chess, position::Error> {
        position::of_fen(&self.fen)
    }

    /// Whose turn it is to find the mate.
    pub fn solver(&self) -> Result<shakmaty::Color, position::Error> {
        Ok(self.position()?.turn())
    }

    /// Re-prove this puzzle from scratch: the stored solution really is linear,
    /// really mates, and really is minimal.
    ///
    /// Every puzzle in `database/` is meant to be run through this in CI, so a
    /// corrupt or mislabelled entry cannot reach the app.
    pub fn verify(&self) -> Result<(), Invalid> {
        // Checked before anything expensive: `depth` arrives from untrusted JSON
        // and is handed to a search whose cost grows ~30x per level. Without this
        // a line claiming `"depth": 12` is indistinguishable from a hang.
        if self.depth == 0 || self.depth > constants::MAX_DEPTH {
            return Err(Invalid::DepthOutOfRange {
                depth: self.depth,
                max: constants::MAX_DEPTH,
            });
        }
        if self.depth != self.solution.len() {
            return Err(Invalid::DepthMismatch {
                depth: self.depth,
                solution: self.solution.len(),
            });
        }

        let pos = self.position().map_err(Invalid::Unparseable)?;

        match mate::judge(&pos, &self.solution) {
            mate::Verdict::Mates { moves } if moves == self.depth => {}
            mate::Verdict::Mates { moves } => {
                return Err(Invalid::ShorterThanClaimed {
                    claimed: self.depth,
                    actual: moves,
                })
            }
            mate::Verdict::Refuted { defense, reason } => {
                return Err(Invalid::NotLinear { defense, reason })
            }
            mate::Verdict::TooComplex { reason } => return Err(Invalid::TooComplex { reason }),
        }

        // Minimality. A puzzle advertised as mate-in-2 that is really a mate-in-1
        // would ask the user for an arrow the position does not need.
        //
        // Only *shorter* lines are in question: `judge` above already proved a
        // linear mate at `depth` exists, so searching `depth` itself would be
        // guaranteed to succeed — and it is by far the most expensive search of
        // the set, ~97% of the total. Searching `depth - 1` instead is exactly
        // equivalent and measured ~42x faster on mate-in-4.
        match mate::min_depth(&pos, self.depth - 1) {
            None => Ok(()),
            Some(actual) => Err(Invalid::NotMinimal {
                claimed: self.depth,
                actual,
            }),
        }
    }
}

/// Why a stored puzzle failed re-verification.
#[derive(Clone, Debug, thiserror::Error)]
pub enum Invalid {
    #[error("position does not parse: {0}")]
    Unparseable(position::Error),
    #[error("declared depth {depth} is outside 1..={max}")]
    DepthOutOfRange { depth: usize, max: usize },
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
    #[error("too complex to verify: {reason}")]
    TooComplex { reason: mate::Limit },
}

/// Read a JSONL puzzle file.
pub fn of_jsonl(contents: &str) -> Result<Vec<Puzzle>, serde_json::Error> {
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
