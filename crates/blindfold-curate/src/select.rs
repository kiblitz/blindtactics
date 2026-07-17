//! Choosing which verified puzzles to keep.
//!
//! Its own module, and free of I/O, so it can be tested natively like everything in
//! core. The alternative — "take the first N" — is what you get by accident, and it
//! is wrong in a way that is invisible: the dump is ordered by puzzle ID, which
//! correlates with nothing a solver cares about but is fixed, so the first 100 are
//! an arbitrary-but-not-random sample and every regeneration returns the same ones.

use blindfold_core::puzzle;

/// Pick `want` puzzles spread evenly across the rating range.
///
/// Sorts by rating and takes evenly spaced entries, so a depth's file runs from its
/// easiest survivor to its hardest rather than clustering wherever the scan happened
/// to stop. That matters more than it looks: within a mate depth, Lichess ratings
/// span roughly 400-2800, and a set drawn from one end would make an entire tier
/// feel uniformly trivial or uniformly brutal.
///
/// Returns everything, sorted, if there are fewer than `want`.
pub fn by_rating_spread(mut puzzles: Vec<puzzle::Puzzle>, want: usize) -> Vec<puzzle::Puzzle> {
    // Ties broken by id so the output is deterministic: same dump in, same database
    // out, and a regeneration diffs cleanly instead of churning.
    puzzles.sort_by(|a, b| a.rating.cmp(&b.rating).then_with(|| a.id.cmp(&b.id)));

    // Explicit rather than emergent. The loop below would return empty for `want == 0`
    // on its own — `0..0` never runs — so this guard is belt and braces, not the fix.
    //
    // It is here because of what the *original* spelling did: `len() <= want ||
    // want == 0` shared one branch, and `want == 0` reached `return puzzles`, so
    // asking for no puzzles returned every puzzle. Written out separately, that
    // reading is impossible.
    if want == 0 {
        return Vec::new();
    }
    if puzzles.len() <= want {
        return puzzles;
    }

    // Evenly spaced indices across the whole range, endpoints included. Using
    // `len - 1` over `want - 1` puts the last pick exactly on the hardest puzzle
    // rather than one short of it.
    let last = puzzles.len() - 1;
    let mut picked: Vec<puzzle::Puzzle> = Vec::with_capacity(want);
    let mut taken: Vec<bool> = vec![false; puzzles.len()];
    for i in 0..want {
        let idx = if want == 1 {
            0
        } else {
            (i * last).div_ceil(want - 1)
        };
        taken[idx] = true;
    }
    for (p, keep) in puzzles.into_iter().zip(taken) {
        if keep {
            picked.push(p);
        }
    }
    picked
}
