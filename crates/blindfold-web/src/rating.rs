//! The user's puzzle Elo: the update math, and where it is kept.
//!
//! The math is a plain function with no browser in it, so this module's tests
//! drive it under native `cargo test`. Only [`load`] and [`save`] touch
//! `localStorage`, and they are as thin as the binding allows.

use crate::constants;

/// The outcome of the first submission on a puzzle — the only one that scores.
///
/// A puzzle counts once: chess.com-style, your first attempt decides it, and
/// retrying afterward does not re-rate. Which submission is "first" is tracked by
/// [`crate::session::Attempt`], so this is only ever the win/loss itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The line mated — a win against the puzzle's rating.
    Solved,
    /// Some defense survived — a loss.
    Failed,
}

impl Outcome {
    /// The Elo actual-score: 1 for a win, 0 for a loss.
    fn score(self) -> f64 {
        match self {
            Outcome::Solved => 1.0,
            Outcome::Failed => 0.0,
        }
    }
}

/// The user's rating after facing a puzzle rated `puzzle` with `outcome`.
///
/// Standard Elo: the expected score is a logistic on the rating gap, and the
/// rating moves by `K * (actual - expected)`. So beating a higher-rated puzzle
/// gains more than beating a lower one, a miss against an easy one costs more, and
/// the result is clamped to a sane band.
pub fn update(user: u32, puzzle: u32, outcome: Outcome) -> u32 {
    let gap = (f64::from(puzzle) - f64::from(user)) / constants::ELO_SCALE;
    let expected = 1.0 / (1.0 + constants::ELO_LOG_BASE.powf(gap));
    let next = f64::from(user) + constants::ELO_K * (outcome.score() - expected);
    (next.round() as i64).clamp(
        i64::from(constants::ELO_FLOOR),
        i64::from(constants::ELO_CEILING),
    ) as u32
}

/// The stored rating, or [`constants::ELO_START`] for a browser that has none.
///
/// Anything unparseable is treated as absent rather than an error to surface: a
/// corrupt key is not something the user can act on, and starting fresh is the
/// right recovery.
pub fn load() -> u32 {
    storage()
        .and_then(|s| s.get_item(constants::ELO_STORAGE_KEY).ok().flatten())
        .and_then(|raw| raw.parse::<u32>().ok())
        .map_or(constants::ELO_START, |r| {
            r.clamp(constants::ELO_FLOOR, constants::ELO_CEILING)
        })
}

/// Persist the rating. Silent if `localStorage` is unavailable (private mode,
/// storage disabled): the rating then simply does not survive a reload, which is a
/// graceful degradation rather than a failure worth interrupting the user over.
pub fn save(rating: u32) {
    if let Some(s) = storage() {
        let _ = s.set_item(constants::ELO_STORAGE_KEY, &rating.to_string());
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}
