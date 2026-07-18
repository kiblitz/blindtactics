//! The embedded database, checked against the files it was compiled from.
//!
//! `blindfold-curate`'s `tests/database.rs` re-proves the committed JSONL from
//! disk. This is a different question: whether the *app* got all of it. The paths
//! in `database.rs` are `include_str!` literals — they cannot be built from
//! `curate::constants::file_name`, which is what writes them — so a rename that
//! misses one is a seam nothing else covers. It fails here rather than shipping a
//! tier of puzzles nobody can reach.

use blindfold_core::arrow;
use blindfold_core::mate;
use blindfold_core::roster;
use blindfold_web::database;
use blindfold_web::session;

#[test]
fn every_depth_the_curator_writes_is_embedded() {
    let puzzles = database::load();
    let mut by_depth: std::collections::BTreeMap<usize, usize> = Default::default();
    for p in &puzzles {
        *by_depth.entry(p.depth).or_default() += 1;
    }
    assert_eq!(
        by_depth.keys().copied().collect::<Vec<_>>(),
        (1..=blindfold_core::constants::MAX_DEPTH).collect::<Vec<_>>(),
        "a depth is missing from the include_str! list"
    );
    // Every tier the same size: a file quietly included twice, or one included in
    // place of another, shows up here as a lopsided count.
    let counts: Vec<usize> = by_depth.values().copied().collect();
    assert!(
        counts.iter().all(|n| *n == counts[0]),
        "tiers are uneven: {by_depth:?} — is a file included twice?"
    );
    assert_eq!(
        puzzles.len(),
        counts[0] * blindfold_core::constants::MAX_DEPTH
    );
}

/// `database::load` concatenates the four `mate_in_N.jsonl` files in ascending
/// depth order. Selection no longer depends on that order — `choose_near` picks by
/// rating — but pinning it catches an out-of-order `include_str!` list, which would
/// mean the embedded pool was assembled differently than the module claims.
#[test]
fn puzzles_are_embedded_in_depth_order() {
    let depths: Vec<usize> = database::load().iter().map(|p| p.depth).collect();
    assert!(
        depths.windows(2).all(|w| w[0] <= w[1]),
        "depths are not ascending — the include_str! list is out of order"
    );
}

#[test]
fn ids_are_unique_so_a_puzzle_can_be_named() {
    let puzzles = database::load();
    let unique: std::collections::HashSet<&str> = puzzles.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(unique.len(), puzzles.len());
}

/// Every embedded puzzle must be one the app can actually put on screen: a legal
/// FEN, a roster to announce, and a solver to orient the board for. `expect()` on
/// these paths is all over the UI, and it is only honest if this holds.
#[test]
fn every_embedded_puzzle_can_be_rendered() {
    for p in database::load() {
        let position = p
            .position()
            .unwrap_or_else(|e| panic!("puzzle {}: {e:?}", p.id));
        p.solver()
            .unwrap_or_else(|e| panic!("puzzle {}: {e:?}", p.id));
        let r = roster::of(&position);
        assert!(!r.text().is_empty(), "puzzle {}: empty roster", p.id);
        assert!(r.squares() >= 2, "puzzle {}: fewer than two kings?", p.id);
    }
}

/// The app's whole promise: the stored line is judged to mate, by the same call
/// the browser makes on submit. If this fails, a user typing the recorded answer
/// is told they are wrong — with no board to check it against.
#[test]
fn the_stored_line_solves_every_embedded_puzzle() {
    for p in database::load() {
        match session::solve(&p, &p.solution) {
            session::Solve::Solved(steps) => {
                assert!(!steps.is_empty(), "puzzle {}: solved with no replay", p.id);
            }
            other => panic!("puzzle {}: {other:?}", p.id),
        }
    }
}

/// Playback is what the reveal steps through, and the reveal is the payoff. It must
/// end on the mate — a replay that stops early leaves the board showing a
/// position the user did not solve, captioned as though they had.
#[test]
fn the_replay_ends_in_checkmate() {
    use shakmaty::Position as _;
    for p in database::load() {
        let session::Solve::Solved(steps) = session::solve(&p, &p.solution) else {
            panic!("puzzle {} does not solve", p.id);
        };
        let last = steps.last().expect("a mating line has steps");
        assert!(
            last.after.is_checkmate(),
            "puzzle {}: the replay's last position is not mate",
            p.id
        );
        // The defender replies between the solver's moves, so a mate in N replays
        // 2N-1 plies. The reveal's pacing is derived from this.
        assert_eq!(
            steps.len(),
            2 * p.depth - 1,
            "puzzle {}: unexpected replay length",
            p.id
        );
    }
}

/// A wrong line must be *refuted*, not merely "not solved". The distinction is
/// the difference between telling the user they were wrong and telling them we
/// declined to check.
#[test]
fn an_illegal_line_is_refuted_rather_than_accepted() {
    let p = database::load().into_iter().next().expect("a puzzle");
    let nonsense = vec![arrow::Arrow::new(
        shakmaty::Square::A1,
        shakmaty::Square::A2,
    )];
    match session::solve(&p, &nonsense) {
        session::Solve::Refuted { reason, .. } => {
            assert!(matches!(reason, mate::Reason::Illegal(_)));
        }
        other => panic!("expected a refutation, got {other:?}"),
    }
}

/// The app accepts any mate, not the recorded one — because Lichess deliberately
/// waives uniqueness for mate puzzles, so duals are in the data by design. A
/// blindfold user who finds the other mate cannot see that they were right, so
/// being told "wrong" would be unanswerable.
///
/// Rather than assume a dual exists, this asks the solver for a line and checks
/// that a line it did not copy from `solution` is still accepted.
#[test]
fn any_mate_is_accepted_not_just_the_recorded_one() {
    let mut duals = 0;
    for p in database::load() {
        let position = p.position().expect("verified");
        let found = mate::find_linear(&position, p.depth).expect("a mate exists");
        if found != p.solution {
            duals += 1;
            assert!(
                matches!(session::solve(&p, &found), session::Solve::Solved(_)),
                "puzzle {}: found a different mate and the app rejected it",
                p.id
            );
        }
    }
    assert!(
        duals > 0,
        "no puzzle in the database has a line other than the stored one — this test \
         proved nothing; find a dual or delete it"
    );
}
