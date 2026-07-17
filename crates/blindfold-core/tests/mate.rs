//! Tests for the linearity prover.

mod common;

use blindfold_core::constants;
use blindfold_core::mate;
// Trait import, so `as _`: shakmaty's board queries are trait methods and Rust has
// no way to reach them via a module path. Nothing here refers to the name.
use shakmaty::Position as _;

#[test]
fn back_rank_mate_in_one() {
    let v = mate::judge(&common::pos(common::BACK_RANK), &common::line("a1a8"));
    assert_eq!(v, mate::Verdict::Mates { moves: 1 });
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
    assert_eq!(v, mate::Verdict::Mates { moves: 2 });
}

/// Sanity-check the fixture itself: Black really does have several defenses, so
/// the test above is actually exercising a branch rather than a forced sequence.
#[test]
fn linear_fixture_really_branches() {
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
    let mut pos = common::pos(common::BRANCHING_BLOCKED);
    for uci in ["f6g6", "h8g8"] {
        let m = common::a(uci).resolve(&pos).expect("legal");
        pos.play_unchecked(m);
    }
    // Against `Kg8` specifically, Rb8 is mate — which is why proving linearity
    // needs every defense, not the one the puzzle author's engine picked.
    assert_eq!(
        mate::judge(&pos, &common::line("b1b8")),
        mate::Verdict::Mates { moves: 1 }
    );
}

#[test]
fn ladder_mate_is_linear() {
    let v = mate::judge(&common::pos(common::LADDER), &common::line("a1a7 b1b8"));
    assert_eq!(v, mate::Verdict::Mates { moves: 2 });
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
    assert_eq!(v, mate::Verdict::Mates { moves: 1 });
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
                    mate::Verdict::Mates { moves: line.len() },
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
    assert_eq!(v, mate::Verdict::Mates { moves: 1 });
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
// Bounds
//
// Nothing caps how many arrows a user may draw, and judging is breadth-first
// over every legal defense, so these are what stand between a pathological
// submission and an out-of-memory abort in the browser.
// ---------------------------------------------------------------------------

#[test]
fn an_over_long_line_is_rejected_before_any_work() {
    let line: Vec<_> = std::iter::repeat_n(common::a("a1a8"), constants::MAX_LINE + 1).collect();
    let v = mate::judge(&common::pos(common::BACK_RANK), &line);
    assert_eq!(
        v,
        mate::Verdict::TooComplex {
            reason: mate::Limit::Length {
                moves: constants::MAX_LINE + 1
            }
        }
    );
}

#[test]
fn a_line_at_the_length_limit_is_still_judged() {
    let mut line = common::line("a1a8"); // Mates immediately.
    while line.len() < constants::MAX_LINE {
        line.push(common::a("a8a7"));
    }
    assert_eq!(
        mate::judge(&common::pos(common::BACK_RANK), &line),
        mate::Verdict::Mates { moves: 1 },
        "the limit is inclusive, and trailing arrows after mate are ignored"
    );
}

/// The runaway case: a line no defense can refute, whose frontier would reach
/// ~30M branches and ~5 GiB. Judging must give up honestly rather than die.
#[test]
fn a_runaway_frontier_is_reported_not_exhausted() {
    // Shuffle the dark bishop back and forth. Black cannot touch it.
    let line = common::line("a1b2 b2a1 a1b2 b2a1 a1b2 b2a1");
    let v = mate::judge(&common::pos(common::UNBOUNDED_FRONTIER), &line);
    match v {
        mate::Verdict::TooComplex {
            reason: mate::Limit::Frontier { branches },
        } => {
            assert!(branches > constants::MAX_FRONTIER);
        }
        other => panic!("expected the frontier bound to fire, got {other:?}"),
    }
}

/// `TooComplex` must never be mistaken for a solve — nor for a failure.
///
/// `mates()` and `refuted()` are deliberately not complements, and this is the
/// verdict that proves it: giving up is a third answer. A UI that reached for
/// `!mates()` to mean "wrong" would accuse the user of missing a mate we simply
/// declined to look for, and they cannot see the board to know better.
#[test]
fn too_complex_is_neither_a_mate_nor_a_refutation() {
    let line: Vec<_> = std::iter::repeat_n(common::a("a1a8"), constants::MAX_LINE + 1).collect();
    let v = mate::judge(&common::pos(common::BACK_RANK), &line);
    assert!(!v.mates(), "we never proved a mate");
    assert!(!v.refuted(), "and we never found a defense either");
}

/// The other half: for every verdict that is not `TooComplex`, the two really are
/// complements. Without this, `refuted()` could be stubbed `false` and the test
/// above would still pass.
#[test]
fn a_decided_verdict_is_exactly_one_of_mates_or_refuted() {
    let pos = common::pos(common::BACK_RANK);
    for uci in ["a1a8", "a1a7", "e4e5"] {
        let v = mate::judge(&pos, &common::line(uci));
        assert_ne!(
            v.mates(),
            v.refuted(),
            "{uci}: judge reached a verdict, so exactly one must hold; got {v:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

/// `judge` says *whether* a line mates; the UI also needs *what to show*.
#[test]
fn playback_walks_a_solve_to_checkmate() {
    let pos = common::pos(common::BRANCHING_LINEAR);
    let steps = mate::playback(&pos, &common::line("f6g6 b1b8")).expect("this line mates");

    // Kg6, a defense, Rb8#.
    assert_eq!(steps.len(), 3);
    assert!(steps.last().expect("non-empty").after.is_checkmate());
}

#[test]
fn playback_stops_at_an_early_mate() {
    let pos = common::pos(common::BACK_RANK);
    let steps = mate::playback(&pos, &common::line("a1a8 a8a7 a7a6")).expect("mates on arrow 1");
    assert_eq!(steps.len(), 1, "trailing arrows are never played");
}

#[test]
fn playback_refuses_a_line_that_does_not_mate() {
    let pos = common::pos(common::BACK_RANK);
    assert_eq!(mate::playback(&pos, &common::line("a1a7")), None);
}

/// The reveal must show the puzzle the user was asked to solve.
///
/// By linearity every defense ends in mate, so *any* of them animates a correct
/// line and this looks like a free choice. It is not. In
/// [`common::DIVERGENT_DEFENSE`] one defense (`Kc2`) is mated an arrow early, so
/// choosing it animates a two-move finish for a mate-in-3 — a correct line, and
/// the wrong puzzle. The user, who solved it blind, is shown a board that never
/// needed their third arrow.
///
/// This fails against `legal_moves().first()`, which picks `Kc2`.
#[test]
fn playback_shows_the_full_depth_the_puzzle_advertises() {
    let pos = common::pos(common::DIVERGENT_DEFENSE);
    let line = common::line("b6d4 d4d1 g5f4");

    // The fixture's premise: a real, minimal mate in 3.
    assert_eq!(mate::judge(&pos, &line), mate::Verdict::Mates { moves: 3 });
    assert_eq!(mate::min_depth(&pos, 4), Some(3), "no shorter mate exists");

    let steps = mate::playback(&pos, &line).expect("mates");
    let solver_moves = steps.len().div_ceil(2);
    assert_eq!(
        solver_moves,
        line.len(),
        "playback must animate all {} arrows, not stop at a defense that dies early",
        line.len()
    );
    assert!(steps.last().expect("non-empty").after.is_checkmate());
}

/// The defense `playback` picks has to be one the defender could really have
/// played — choosing the longest survivor must not smuggle in an illegal move.
#[test]
fn playback_picks_a_legal_defense() {
    let pos = common::pos(common::DIVERGENT_DEFENSE);
    let steps = mate::playback(&pos, &common::line("b6d4 d4d1 g5f4")).expect("mates");

    let mut replay = pos.clone();
    for step in &steps {
        assert!(
            replay.legal_moves().contains(&step.played),
            "{} is not legal in the position it was played from",
            step.played
        );
        replay.play_unchecked(step.played);
        assert_eq!(replay, step.after, "Step::after must follow Step::played");
    }
}

/// Whatever `playback` shows must be a real game: every move legal, ending mated.
#[test]
fn playback_produces_a_legal_game() {
    let pos = common::pos(common::LADDER);
    let line = common::line("a1a7 b1b8");
    let steps = mate::playback(&pos, &line).expect("mates");

    let mut replay = pos.clone();
    for step in &steps {
        assert!(
            replay.legal_moves().contains(&step.played),
            "illegal move in playback"
        );
        replay.play_unchecked(step.played);
        assert_eq!(
            replay, step.after,
            "recorded position disagrees with the replay"
        );
    }
    assert!(replay.is_checkmate());
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
