//! Tests for the puzzle model and its self-verification.
//!
//! `verify` is what is meant to stand between a bad puzzle and the user: once
//! `database/` and CI exist, every entry will be run through it. Neither exists
//! yet, so for now these are tests of a guard rail that is built but not yet
//! deployed.

mod common;

use blindfold_core::puzzle;

fn good() -> puzzle::Puzzle {
    puzzle::Puzzle {
        id: "test01".to_owned(),
        fen: common::BRANCHING_LINEAR.to_owned(),
        solution: common::line("f6g6 b1b8"),
        depth: 2,
        rating: 1500,
    }
}

#[test]
fn a_sound_puzzle_verifies() {
    good()
        .verify()
        .expect("this puzzle is a genuine linear mate in 2");
}

#[test]
fn solver_is_the_side_to_move() {
    assert_eq!(good().solver().expect("parses"), shakmaty::Color::White);
}

#[test]
fn round_trips_through_jsonl() {
    let puzzles = vec![good()];
    let text = puzzle::to_jsonl(&puzzles).expect("serializes");
    assert_eq!(text.lines().count(), 1, "one puzzle per line");
    assert!(
        text.contains(r#""solution":["f6g6","b1b8"]"#),
        "arrows stay readable: {text}"
    );
    assert_eq!(puzzle::of_jsonl(&text).expect("parses"), puzzles);
}

#[test]
fn jsonl_tolerates_blank_lines() {
    let text = format!(
        "\n{}\n\n",
        puzzle::to_jsonl(&[good()]).expect("serializes").trim()
    );
    assert_eq!(puzzle::of_jsonl(&text).expect("parses").len(), 1);
}

// ---------------------------------------------------------------------------
// Every way a stored puzzle can be wrong.
// ---------------------------------------------------------------------------

#[test]
fn rejects_a_depth_that_disagrees_with_the_solution() {
    let p = puzzle::Puzzle { depth: 3, ..good() };
    assert!(matches!(
        p.verify(),
        Err(puzzle::Invalid::DepthMismatch {
            depth: 3,
            solution: 2
        })
    ));
}

/// `depth` arrives from untrusted JSON and is handed straight to a search whose
/// cost grows ~30x per level. Unclamped, a line claiming `"depth": 12` is
/// indistinguishable from a hang — and it would hang the CI job that is supposed
/// to be guarding the database.
#[test]
fn rejects_a_depth_outside_the_supported_range() {
    for depth in [0, blindfold_core::constants::MAX_DEPTH + 1, 12, usize::MAX] {
        let p = puzzle::Puzzle { depth, ..good() };
        assert!(
            matches!(p.verify(), Err(puzzle::Invalid::DepthOutOfRange { .. })),
            "depth {depth} must be rejected outright, got {:?}",
            p.verify()
        );
    }
}

#[test]
fn rejects_an_unparseable_position() {
    let p = puzzle::Puzzle {
        fen: "not a fen".to_owned(),
        ..good()
    };
    assert!(matches!(p.verify(), Err(puzzle::Invalid::Unparseable(_))));
}

/// The case the whole project exists to catch: a line that mates against the one
/// defense an engine happened to pick, but not against all of them.
#[test]
fn rejects_a_non_linear_solution() {
    let p = puzzle::Puzzle {
        fen: common::BRANCHING_BLOCKED.to_owned(),
        ..good()
    };
    assert!(
        matches!(p.verify(), Err(puzzle::Invalid::NotLinear { .. })),
        "Black interposes on the b-file and the line dies"
    );
}

#[test]
fn rejects_a_solution_that_does_not_mate() {
    let p = puzzle::Puzzle {
        solution: common::line("f6g6 b1b7"),
        ..good()
    };
    assert!(matches!(p.verify(), Err(puzzle::Invalid::NotLinear { .. })));
}

/// A puzzle advertised as mate-in-2 that is really a mate-in-1 would ask the user
/// for an arrow the position does not need.
#[test]
fn rejects_a_puzzle_that_is_really_shorter() {
    // Qh7 then Qb7# is a genuine linear mate in 2 — but Qb7# mates immediately,
    // so this position is a mate in 1 and must not be sold as anything else.
    let p = puzzle::Puzzle {
        fen: common::STALEMATE_TRAP.to_owned(),
        solution: common::line("g7h7 h7b7"),
        depth: 2,
        ..good()
    };
    assert!(
        matches!(
            p.verify(),
            Err(puzzle::Invalid::NotMinimal {
                claimed: 2,
                actual: 1
            })
        ),
        "got {:?}",
        p.verify()
    );
}
