//! Turning a raw Lichess puzzle row into one of ours.
//!
//! This lives in core rather than in the curation tool on purpose. It is pure
//! logic with no I/O, so the rule "anything that can live in core, must" applies —
//! and more to the point, it encodes the single most error-prone fact in the
//! project, which deserves the fast native test suite rather than a hand-rolled
//! conversion inside a CLI.
//!
//! # Two facts that trip everyone up, not one
//!
//! **First**, the `FEN` column is *not* the position the player sees. It is the
//! position *before* the opponent's setup move. `Moves[0]` is that setup move; you
//! apply it, and the result is what the player is shown. So the side the user plays
//! is the side to move **after** `Moves[0]` — the *opposite* of the FEN's active
//! colour. Getting this backwards is wrong in a way that looks right: a legal
//! position, a legal line, and the wrong player to move.
//!
//! **Second**, `Moves[1..]` is the solution *line*, and a line alternates. It is
//! **not** the list of moves the user plays. Only the odd indices are theirs:
//!
//! ```text
//! Moves:  e5f6   e8e1   g1f2   e1f1        (a real mateIn2 row, 000Zo)
//!         setup  SOLVER defence SOLVER
//!         [0]    [1]    [2]     [3]
//! ```
//!
//! So the user's arrows are `Moves[1]`, `Moves[3]`, `Moves[5]`, ... and the depth is
//! `Moves.len() / 2` (the list is always even: one setup, then solver and defender
//! alternating, ending on the solver's mate). Taking `Moves[1..]` wholesale would
//! feed the *defender's replies* in as user arrows — the puzzle would be nonsense,
//! and `verify` would reject it, which is the good outcome. The bad outcome is the
//! rare row where the mangled line happens to still be a legal mate.
//!
//! The defender's moves in the row are simply thrown away. They are one engine's
//! choice at generation time, and [`crate::mate::judge`] plays *every* defense — so
//! the only thing the row's replies could do is mislead us.
//!
//! # Nothing here is trusted
//!
//! A row's `mateInN` theme is a *candidate filter*, no more. Lichess accepts any
//! mating move as correct and deliberately waives uniqueness for mate puzzles, and
//! its stored line records exactly one engine-chosen defense — so the tag says
//! nothing about whether the line is linear. [`of_row`] only parses; it is
//! [`crate::puzzle::Puzzle::verify`] that re-proves the puzzle from scratch.

use crate::arrow;
use crate::constants;
use crate::position;
use crate::puzzle;

/// Why a Lichess row could not become a puzzle.
///
/// Most of these are *expected* in bulk: the dump is 6M rows and we keep a few
/// hundred. Rejection is the normal path, so the reasons carry enough detail to
/// tell a corrupt dump from an honest filter.
#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "expected at least {} columns, got {got}",
        constants::LICHESS_MIN_COLUMNS
    )]
    Columns { got: usize },
    #[error("puzzle {id}: raw FEN did not parse: {source}")]
    Fen {
        id: String,
        #[source]
        source: position::Error,
    },
    #[error("puzzle {id}: move list is empty, so there is no setup move")]
    NoSetupMove { id: String },
    #[error("puzzle {id}: move `{uci}` is not a UCI move: {source}")]
    Uci {
        id: String,
        uci: String,
        #[source]
        source: arrow::ParseError,
    },
    #[error("puzzle {id}: setup move `{uci}` is illegal in the raw FEN")]
    IllegalSetup { id: String, uci: String },
    #[error("puzzle {id}: rating `{rating}` is not a number")]
    Rating { id: String, rating: String },
    #[error("puzzle {id}: no solution moves after the setup move")]
    NoSolution { id: String },
}

/// One row of `lichess_db_puzzle.csv`, already split into fields.
///
/// Borrowed rather than owned: the caller is streaming ~6M of these and almost all
/// will be rejected, so there is no reason to allocate for them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Row<'a> {
    pub id: &'a str,
    pub fen: &'a str,
    pub moves: &'a str,
    pub rating: &'a str,
    pub themes: &'a str,
}

impl<'a> Row<'a> {
    /// Split a raw CSV line into the fields we care about.
    ///
    /// Plain `split(',')`, deliberately, rather than a CSV crate. The columns we
    /// read — id, FEN, moves, rating, themes — cannot contain a comma or a quote:
    /// they are all IDs, FEN, UCI, an integer, and space-separated theme tags.
    /// `OpeningTags` and `GameUrl` come later in the row and are never read, so
    /// even if they did quote something it could not shift the fields we use.
    pub fn of_csv(line: &'a str) -> Result<Self, Error> {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < constants::LICHESS_MIN_COLUMNS {
            return Err(Error::Columns { got: f.len() });
        }
        Ok(Self {
            id: f[constants::LICHESS_COL_ID],
            fen: f[constants::LICHESS_COL_FEN],
            moves: f[constants::LICHESS_COL_MOVES],
            rating: f[constants::LICHESS_COL_RATING],
            themes: f[constants::LICHESS_COL_THEMES],
        })
    }

    /// Whether the row carries the given theme tag.
    ///
    /// Themes are space-separated, so this matches whole tags: `mateIn1` must not
    /// be found inside `mateIn12` (which does not exist today, but `substring`
    /// matching here would be a bug waiting for one).
    pub fn has_theme(&self, theme: &str) -> bool {
        self.themes.split_whitespace().any(|t| t == theme)
    }
}

/// Convert a row into a puzzle, applying the setup move.
///
/// The returned puzzle is a *candidate*: parsed, legal, and correctly oriented, but
/// not proved. Its `depth` is simply the length of the stored solution — what
/// Lichess claims, not what we have checked. Call
/// [`crate::puzzle::Puzzle::verify`] before believing any of it.
pub fn of_row(row: &Row<'_>) -> Result<puzzle::Puzzle, Error> {
    let before = position::of_fen(row.fen).map_err(|source| Error::Fen {
        id: row.id.to_owned(),
        source,
    })?;

    let mut moves = row.moves.split_whitespace();
    let setup_uci = moves.next().ok_or_else(|| Error::NoSetupMove {
        id: row.id.to_owned(),
    })?;
    let setup: arrow::Arrow = setup_uci.parse().map_err(|source| Error::Uci {
        id: row.id.to_owned(),
        uci: setup_uci.to_owned(),
        source,
    })?;

    // Apply the setup move. *This* is the position the player is shown.
    let mut shown = before;
    let mv = setup.resolve(&shown).map_err(|_| Error::IllegalSetup {
        id: row.id.to_owned(),
        uci: setup_uci.to_owned(),
    })?;
    shakmaty::Position::play_unchecked(&mut shown, mv);

    // `moves` is now Moves[1..] — the solution *line*, alternating solver and
    // defender. `step_by(2)` keeps the solver's, starting with theirs. The
    // defender's are dropped: they are one engine's pick, and `judge` plays every
    // defense, so keeping them could only mislead.
    let solution = moves
        .step_by(2)
        .map(|uci| {
            uci.parse::<arrow::Arrow>().map_err(|source| Error::Uci {
                id: row.id.to_owned(),
                uci: uci.to_owned(),
                source,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if solution.is_empty() {
        return Err(Error::NoSolution {
            id: row.id.to_owned(),
        });
    }

    let rating = row.rating.parse().map_err(|_| Error::Rating {
        id: row.id.to_owned(),
        rating: row.rating.to_owned(),
    })?;

    Ok(puzzle::Puzzle {
        id: row.id.to_owned(),
        fen: position::to_fen(&shown),
        depth: solution.len(),
        solution,
        rating,
    })
}

/// The theme tag for a mate in `depth`, e.g. `mateIn2`.
///
/// Used only to shrink the candidate pool before the real work; see the module doc
/// for why the tag proves nothing.
pub fn mate_theme(depth: usize) -> String {
    format!("{}{depth}", constants::LICHESS_MATE_THEME_PREFIX)
}
