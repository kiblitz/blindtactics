//! Tests for which verified puzzles get kept.
//!
//! `select` decides the app's entire content from the pool that survived
//! verification, and every choice it makes is invisible in the output: a database
//! drawn entirely from one end of the rating range is a file of perfectly valid
//! puzzles that happens to make a whole tier feel uniformly trivial. Nothing else
//! catches that, so it is tested here directly.

use blindfold_core::arrow;
use blindfold_core::puzzle;
use blindfold_curate::select;

/// Ratings are the only field `select` reads; the rest just has to be well-formed.
fn puzzles(ratings: &[u32]) -> Vec<puzzle::Puzzle> {
    ratings
        .iter()
        .enumerate()
        .map(|(i, &rating)| puzzle::Puzzle {
            id: format!("p{i:03}"),
            fen: "4k3/8/8/8/8/8/8/4K2R w K - 0 1".to_owned(),
            solution: vec!["h1h8".parse::<arrow::Arrow>().expect("valid uci")],
            depth: 1,
            rating,
        })
        .collect()
}

fn ratings_of(puzzles: &[puzzle::Puzzle]) -> Vec<u32> {
    puzzles.iter().map(|p| p.rating).collect()
}

#[test]
fn keeps_everything_when_there_is_less_than_asked_for() {
    let kept = select::by_rating_spread(puzzles(&[1500, 900, 1200]), 10);
    assert_eq!(ratings_of(&kept), [900, 1200, 1500]);
}

/// The endpoints are the point. Taking the first N of a rating-sorted pool would
/// pass a "spans a range" check while quietly capping the tier's difficulty.
#[test]
fn the_easiest_and_hardest_survivors_are_both_kept() {
    let pool: Vec<u32> = (0..50).map(|i| 400 + i * 40).collect();
    let kept = ratings_of(&select::by_rating_spread(puzzles(&pool), 5));
    assert_eq!(kept.first(), Some(&400), "easiest must be kept");
    assert_eq!(kept.last(), Some(&2360), "hardest must be kept");
}

#[test]
fn picks_are_spread_across_the_range_not_clustered() {
    let pool: Vec<u32> = (0..100).map(|i| 1000 + i).collect();
    let kept = ratings_of(&select::by_rating_spread(puzzles(&pool), 5));
    assert_eq!(kept, [1000, 1025, 1050, 1075, 1099]);
}

/// The count is load-bearing: `every_depth_has_a_full_set` in the database test
/// asserts exactly `PER_DEPTH`, so an off-by-one in the spacing arithmetic — two
/// indices colliding and yielding `want - 1` puzzles — would surface as a confusing
/// database failure rather than here.
#[test]
fn returns_exactly_what_was_asked_for() {
    for len in 1..60usize {
        for want in 1..=len {
            let pool: Vec<u32> = (0..len as u32).map(|i| 500 + i * 7).collect();
            let kept = select::by_rating_spread(puzzles(&pool), want);
            assert_eq!(kept.len(), want, "len {len}, want {want}");
        }
    }
}

#[test]
fn never_returns_the_same_puzzle_twice() {
    let pool: Vec<u32> = (0..40).map(|i| 800 + i * 3).collect();
    let kept = select::by_rating_spread(puzzles(&pool), 12);
    let unique: std::collections::HashSet<&String> = kept.iter().map(|p| &p.id).collect();
    assert_eq!(unique.len(), kept.len());
}

/// Ties on rating are common — the dump has thousands of puzzles at any given
/// rating — so without a tiebreak the output would depend on scan order and a
/// regeneration would churn the diff for no reason.
#[test]
fn output_is_deterministic_regardless_of_input_order() {
    let mut forward = puzzles(&[1200, 1200, 1200, 800, 1600]);
    let mut backward = forward.clone();
    backward.reverse();

    forward = select::by_rating_spread(forward, 3);
    backward = select::by_rating_spread(backward, 3);

    let ids = |ps: &[puzzle::Puzzle]| ps.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&forward), ids(&backward));
}

#[test]
fn asking_for_none_returns_none() {
    assert!(select::by_rating_spread(puzzles(&[900, 1200]), 0).is_empty());
}
