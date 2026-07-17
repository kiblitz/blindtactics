//! Tests for the linearity prover.

mod common;

use blindfold_core::mate;

#[test]
fn back_rank_mate_in_one() {
    let v = mate::judge(&common::pos(common::BACK_RANK), &common::line("a1a8"));
    assert_eq!(v, mate::Verdict::Mates { plies: 1 });
}

#[test]
fn wrong_move_is_refuted() {
    let v = mate::judge(&common::pos(common::BACK_RANK), &common::line("a1a7"));
    assert!(matches!(
        v,
        mate::Verdict::Refuted {
            reason: mate::Reason::NoMate,
            ..
        }
    ));
}

#[test]
fn arrow_not_on_the_board_is_illegal() {
    // No white piece on e4 at all.
    let v = mate::judge(&common::pos(common::BACK_RANK), &common::line("e4e5"));
    match v {
        mate::Verdict::Refuted {
            reason: mate::Reason::Illegal(a),
            defense,
        } => {
            assert_eq!(a, common::a("e4e5"));
            assert!(
                defense.is_empty(),
                "refuted at the root, before any defense"
            );
        }
        other => panic!("expected Illegal, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Linearity: the property the whole project turns on.
// ---------------------------------------------------------------------------

/// The same arrows must mate no matter which of Black's five defenses is chosen.
#[test]
fn linear_line_mates_through_a_branch() {
    let v = mate::judge(
        &common::pos(common::BRANCHING_LINEAR),
        &common::line("f6g6 b1b8"),
    );
    assert_eq!(v, mate::Verdict::Mates { plies: 2 });
}

/// Sanity-check the fixture itself: Black really does have several defenses, so
/// the test above is actually exercising a branch rather than a forced sequence.
#[test]
fn linear_fixture_really_branches() {
    use shakmaty::Position as _;

    let mut pos = common::pos(common::BRANCHING_LINEAR);
    let key = common::a("f6g6").resolve(&pos).expect("Kg6 is legal");
    pos.play_unchecked(key);
    assert_eq!(pos.legal_moves().len(), 5, "expected Kg8, a6, a5, c6, c5");
}

/// Give Black a piece that can reach the rook's file and the same line
/// collapses — some defense survives it.
///
/// The reason is deliberately not asserted: several of Black's defenses refute
/// this line in different ways, and which one the move generator reaches first is
/// an implementation detail. `arrow_is_illegal_against_a_blocking_defense` pins
/// the interesting one down deterministically.
#[test]
fn a_line_that_fails_one_defense_is_refuted() {
    let v = mate::judge(
        &common::pos(common::BRANCHING_BLOCKED),
        &common::line("f6g6 b1b8"),
    );
    match v {
        mate::Verdict::Refuted { defense, .. } => {
            assert!(
                !defense.is_empty(),
                "must be refuted by a defense, not at the root"
            );
        }
        other => panic!("expected refutation, got {other:?}"),
    }
}

/// The specific failure mode: after Black interposes `Rb7`, the second arrow is
/// not merely bad, it is not a legal move at all. The user drew a rook slide
/// through an occupied square.
#[test]
fn arrow_is_illegal_against_a_blocking_defense() {
    use shakmaty::Position as _;

    let mut pos = common::pos(common::BRANCHING_BLOCKED);
    for uci in ["f6g6", "a7b7"] {
        let m = common::a(uci).resolve(&pos).expect("legal");
        pos.play_unchecked(m);
    }
    let v = mate::judge(&pos, &common::line("b1b8"));
    assert!(
        matches!(v, mate::Verdict::Refuted { reason: mate::Reason::Illegal(a), .. }
            if a == common::a("b1b8")),
        "Rb7 blocks the file, so Rb8 is illegal; got {v:?}"
    );
}

/// The Lichess line for a puzzle like [`common::BRANCHING_BLOCKED`] would record
/// a single defense. If that one defense happens not to block, the line looks
/// perfect. This asserts the trap is real: the line *does* mate against a
/// cherry-picked defense, and is still unusable.
#[test]
fn a_single_defense_can_hide_non_linearity() {
    use shakmaty::Position as _;

    let mut pos = common::pos(common::BRANCHING_BLOCKED);
    for uci in ["f6g6", "h8g8"] {
        let m = common::a(uci).resolve(&pos).expect("legal");
        pos.play_unchecked(m);
    }
    // Against `Kg8` specifically, Rb8 is mate — which is why proving linearity
    // needs every defense, not the one the puzzle author's engine picked.
    assert_eq!(
        mate::judge(&pos, &common::line("b1b8")),
        mate::Verdict::Mates { plies: 1 }
    );
}

#[test]
fn ladder_mate_is_linear() {
    let v = mate::judge(&common::pos(common::LADDER), &common::line("a1a7 b1b8"));
    assert_eq!(v, mate::Verdict::Mates { plies: 2 });
}

/// Same two arrows, wrong order: `Rb8+` lets the king out to h7 or g7 and `Ra7+`
/// mates neither.
#[test]
fn move_order_matters() {
    let v = mate::judge(&common::pos(common::LADDER), &common::line("b1b8 a1a7"));
    assert!(matches!(
        v,
        mate::Verdict::Refuted {
            reason: mate::Reason::NoMate,
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// Stalemate: a draw refutes a mate. This is the classic solver bug.
// ---------------------------------------------------------------------------

#[test]
fn stalemate_refutes_and_is_reported_as_such() {
    let v = mate::judge(&common::pos(common::STALEMATE_TRAP), &common::line("g7c7"));
    assert!(
        matches!(
            v,
            mate::Verdict::Refuted {
                reason: mate::Reason::Stalemate,
                ..
            }
        ),
        "Qc7 stalemates; it must not be mistaken for mate, got {v:?}"
    );
}

#[test]
fn the_stalemate_trap_position_does_have_a_mate() {
    let v = mate::judge(&common::pos(common::STALEMATE_TRAP), &common::line("g7b7"));
    assert_eq!(v, mate::Verdict::Mates { plies: 1 });
}

/// A stalemating move must never be offered as a solution.
///
/// Note this position has *two* mates in 1 — `Qb7#` and `Qa7#`, both protected by
/// the king — so the exact line found is not asserted. Duals like this are
/// precisely what Lichess tolerates in mate puzzles, and they are harmless here:
/// correctness is decided by playing a line out, not by matching a stored one.
#[test]
fn search_never_returns_a_stalemating_line() {
    let pos = common::pos(common::STALEMATE_TRAP);
    let found = mate::find_linear(&pos, 1).expect("mate in 1 exists");
    assert_ne!(
        found,
        common::line("g7c7"),
        "Qc7 stalemates and must never be returned"
    );
    assert!(mate::judge(&pos, &found).mates());
}

/// The invariant tying the two halves of this module together: anything
/// [`mate::find_linear`] returns, [`mate::judge`] must accept.
///
/// This matters beyond tidiness. `find_linear` decides what goes into the
/// database and `judge` decides whether the user solved it. If they could ever
/// disagree, the app would present a puzzle whose own stored answer it rejects.
#[test]
fn search_and_judge_agree() {
    for fen in [
        common::BACK_RANK,
        common::BRANCHING_LINEAR,
        common::BRANCHING_BLOCKED,
        common::LADDER,
        common::STALEMATE_TRAP,
    ] {
        let pos = common::pos(fen);
        for depth in 1..=3 {
            if let Some(line) = mate::find_linear(&pos, depth) {
                assert!(
                    !line.is_empty() && line.len() <= depth,
                    "a line from a depth-{depth} search must be 1..={depth} long, got {line:?}: {fen}"
                );
                assert_eq!(
                    mate::judge(&pos, &line),
                    mate::Verdict::Mates { plies: line.len() },
                    "search found {line:?} at depth {depth} but judge rejects it: {fen}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Trailing arrows
// ---------------------------------------------------------------------------

/// If every defense is mated early, the extra arrows are simply never played.
/// The user still solved it.
#[test]
fn trailing_arrows_after_mate_are_ignored() {
    let v = mate::judge(
        &common::pos(common::BACK_RANK),
        &common::line("a1a8 a8a7 a7a6"),
    );
    assert_eq!(v, mate::Verdict::Mates { plies: 1 });
}

#[test]
fn an_empty_line_never_mates() {
    let v = mate::judge(&common::pos(common::BACK_RANK), &[]);
    assert!(matches!(
        v,
        mate::Verdict::Refuted {
            reason: mate::Reason::NoMate,
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn finds_the_back_rank_mate() {
    let found = mate::find_linear(&common::pos(common::BACK_RANK), 1);
    assert_eq!(found, Some(common::line("a1a8")));
}

/// `Kg6` is quiet, so this also pins down that the search is full-width. Any
/// "checks only" pruning at the first ply would return `None` here.
#[test]
fn finds_a_mate_whose_key_move_is_quiet() {
    let found = mate::find_linear(&common::pos(common::BRANCHING_LINEAR), 2);
    assert_eq!(found, Some(common::line("f6g6 b1b8")));
}

#[test]
fn no_mate_in_one_when_the_king_escapes() {
    assert_eq!(
        mate::find_linear(&common::pos(common::BRANCHING_LINEAR), 1),
        None
    );
}

#[test]
fn min_depth_reports_the_shortest_linear_mate() {
    assert_eq!(mate::min_depth(&common::pos(common::BACK_RANK), 4), Some(1));
    assert_eq!(
        mate::min_depth(&common::pos(common::BRANCHING_LINEAR), 4),
        Some(2)
    );
    assert_eq!(mate::min_depth(&common::pos(common::LADDER), 4), Some(2));
}

/// The whole point of `min_depth`: a position with a mate in 2 must not be
/// advertised as a mate in 3 or 4.
#[test]
fn min_depth_is_not_fooled_by_a_longer_search_budget() {
    assert_eq!(mate::min_depth(&common::pos(common::LADDER), 2), Some(2));
    assert_eq!(mate::min_depth(&common::pos(common::LADDER), 4), Some(2));
}
