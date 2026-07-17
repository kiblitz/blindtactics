//! Streaming the dump down to a pool of candidates.
//!
//! Takes a reader rather than a path, and lives in the lib rather than in `main`, so
//! it can be tested against a dozen synthetic rows instead of a 302 MB download. It
//! is the most intricate logic in the crate — a prefilter, a theme match, two reject
//! gates and an early break — which is exactly why it should not have been the one
//! piece welded to file I/O.
//!
//! Nothing is verified here. Verification costs ~13 ms for a mate-in-4 against ~1 µs
//! for this loop, so the two are kept apart: only survivors of the cheap filters pay
//! for the expensive one.

use crate::constants;
use blindfold_core::lichess;
use blindfold_core::position;
use blindfold_core::puzzle;
use blindfold_core::roster;

/// Candidates per depth, and why the rest were dropped.
#[derive(Debug, Default)]
pub struct Pool {
    pub by_depth: std::collections::BTreeMap<usize, Vec<puzzle::Puzzle>>,
    pub scanned: usize,
    pub rejected: Rejected,
}

/// Tallied by reason rather than lumped into one counter.
///
/// `lichess::Error` is built to carry enough detail to tell a corrupt dump from an
/// honest filter, and a single `rejected += 1` would throw that away — leaving no way
/// to notice that, say, every row had suddenly become unparseable.
#[derive(Debug, Default)]
pub struct Rejected {
    /// Malformed rows: bad CSV arity, unparseable FEN or UCI, illegal setup move.
    pub malformed: usize,
    /// The row's line length disagrees with its own `mateInN` tag.
    pub mislabelled: usize,
    /// More squares than [`constants::MAX_ROSTER_SQUARES`] — unusable blindfold.
    pub too_heavy: usize,
    /// Halfmove clock at or past [`constants::MAX_HALFMOVE_CLOCK`].
    pub drawish: usize,
}

impl Rejected {
    pub fn total(&self) -> usize {
        self.malformed + self.mislabelled + self.too_heavy + self.drawish
    }

    /// Count one rejection. Lives next to the fields so that adding a reason means
    /// touching the enum and this block, rather than a `match` in the middle of the
    /// scan loop that carries no information beyond "variant N is field N".
    fn tally(&mut self, reason: Reject) {
        match reason {
            Reject::Malformed => self.malformed += 1,
            Reject::Mislabelled => self.mislabelled += 1,
            Reject::TooHeavy => self.too_heavy += 1,
            Reject::Drawish => self.drawish += 1,
        }
    }
}

impl Pool {
    /// How many candidates a depth has so far.
    pub fn candidates(&self, depth: usize) -> usize {
        self.by_depth.get(&depth).map_or(0, Vec::len)
    }

    /// Every depth has all the candidates it will take, so the scan can stop.
    ///
    /// `all`, emphatically not `any`: the abundant tiers fill first and the scarce
    /// ones (mate-in-3, mate-in-4) need the rest of the file. Stopping when any depth
    /// filled would under-gather exactly the depths that can least afford it, and it
    /// would do so silently — `run`'s "ran out of dump" note is gated on this.
    pub fn is_full(&self) -> bool {
        constants::DEPTHS
            .iter()
            .all(|d| self.candidates(*d) >= constants::CANDIDATES_PER_DEPTH)
    }
}

/// Read rows until every depth has [`constants::CANDIDATES_PER_DEPTH`], or EOF.
pub fn of_rows(
    reader: impl std::io::BufRead,
    mut progress: impl FnMut(&Pool),
) -> std::io::Result<Pool> {
    let themes: Vec<(usize, String)> = constants::DEPTHS
        .iter()
        .map(|&d| (d, lichess::mate_theme(d)))
        .collect();

    let mut pool = Pool::default();

    for line in reader.lines() {
        let line = line?;
        pool.scanned += 1;
        if pool.scanned.is_multiple_of(constants::PROGRESS_EVERY) {
            progress(&pool);
        }

        // Cheap reject first: ~68% of rows carry no `mateIn` tag at all, and this is a
        // substring scan against a line already in hand.
        if !line.contains(constants::MATE_THEME_HINT) {
            continue;
        }
        let Ok(row) = lichess::Row::of_csv(&line) else {
            pool.rejected.malformed += 1;
            continue;
        };
        let Some((depth, _)) = themes.iter().find(|(_, theme)| row.has_theme(theme)) else {
            continue;
        };
        let depth = *depth;

        if pool.candidates(depth) >= constants::CANDIDATES_PER_DEPTH {
            if pool.is_full() {
                break;
            }
            continue;
        }

        match candidate(&row, depth) {
            Ok(p) => pool.by_depth.entry(depth).or_default().push(p),
            Err(reason) => pool.rejected.tally(reason),
        }
    }

    Ok(pool)
}

enum Reject {
    Malformed,
    Mislabelled,
    TooHeavy,
    Drawish,
}

/// Convert one row and apply the gates that do not need a search.
fn candidate(row: &lichess::Row<'_>, depth: usize) -> Result<puzzle::Puzzle, Reject> {
    let puzzle = lichess::of_row(row).map_err(|_| Reject::Malformed)?;
    if puzzle.depth != depth {
        return Err(Reject::Mislabelled);
    }

    let position = puzzle.position().map_err(|_| Reject::Malformed)?;

    // The blindfold gate. Measured on the position the *user* is shown — the one after
    // the setup move — not on the raw row, whose FEN is a ply earlier and may hold a
    // piece the setup move captures.
    if roster::of(&position).squares() > constants::MAX_ROSTER_SQUARES {
        return Err(Reject::TooHeavy);
    }

    if position::halfmove_clock(&position) >= constants::MAX_HALFMOVE_CLOCK {
        return Err(Reject::Drawish);
    }

    Ok(puzzle)
}
