//! Tests for the Elo update math.
//!
//! `load`/`save` touch `localStorage` and are not covered here; the arithmetic
//! that actually decides the rating is pure, and this is where it is pinned.

use blindfold_web::constants;
use blindfold_web::rating;

#[test]
fn a_win_raises_and_a_loss_lowers() {
    let base = 1500;
    assert!(rating::update(base, 1500, rating::Outcome::Solved) > base);
    assert!(rating::update(base, 1500, rating::Outcome::Failed) < base);
}

/// Against an equal-rated puzzle the expected score is 0.5, so the swing is half
/// of K each way.
#[test]
fn an_even_match_moves_by_half_k() {
    let base = 1500;
    let half_k = (constants::ELO_K / 2.0).round() as u32;
    assert_eq!(
        rating::update(base, 1500, rating::Outcome::Solved) - base,
        half_k
    );
    assert_eq!(
        base - rating::update(base, 1500, rating::Outcome::Failed),
        half_k
    );
}

/// Beating a stronger puzzle gains more than beating a weaker one — the gap is
/// what makes the rating mean anything.
#[test]
fn beating_a_harder_puzzle_gains_more() {
    let base = 1500;
    let vs_strong = rating::update(base, 1900, rating::Outcome::Solved) - base;
    let vs_weak = rating::update(base, 1100, rating::Outcome::Solved) - base;
    assert!(vs_strong > vs_weak, "strong {vs_strong} vs weak {vs_weak}");
}

/// And losing to a weaker puzzle costs more than losing to a stronger one.
#[test]
fn losing_to_an_easier_puzzle_costs_more() {
    let base = 1500;
    let to_weak = base - rating::update(base, 1100, rating::Outcome::Failed);
    let to_strong = base - rating::update(base, 1900, rating::Outcome::Failed);
    assert!(to_weak > to_strong, "weak {to_weak} vs strong {to_strong}");
}

/// The rating never leaves its band, even where an unclamped update would push it
/// past — losing at the floor, or winning at the ceiling.
#[test]
fn the_rating_stays_within_its_bounds() {
    assert_eq!(
        rating::update(
            constants::ELO_FLOOR,
            constants::ELO_FLOOR,
            rating::Outcome::Failed
        ),
        constants::ELO_FLOOR,
        "cannot fall below the floor"
    );
    assert_eq!(
        rating::update(
            constants::ELO_CEILING,
            constants::ELO_CEILING,
            rating::Outcome::Solved
        ),
        constants::ELO_CEILING,
        "cannot rise above the ceiling"
    );
}
