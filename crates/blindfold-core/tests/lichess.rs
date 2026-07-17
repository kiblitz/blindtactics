//! Tests for the Lichess row -> puzzle conversion.
//!
//! Every fixture here is a **verbatim row from the real dump**, not one written by
//! hand. That is deliberate and it is the point: the two facts this module exists to
//! encode — that the FEN precedes the setup move, and that the solution line
//! alternates — are exactly the kind a hand-built fixture would get wrong in the
//! same direction as the code, and pass.
//!
//! The first draft of `of_row` did take `Moves[1..]` as the user's arrows. Rows
//! invented from the module doc agreed with it. These rows did not.

mod common;

use blindfold_core::lichess;
use blindfold_core::mate;
use blindfold_core::position;
use blindfold_core::puzzle;

/// Real rows, copied out of `lichess_db_puzzle.csv` (2026-07-01 dump).
///
/// Trailing columns are truncated to what `Row::of_csv` reads; `GameUrl` and
/// `OpeningTags` are deliberately absent to prove an 8-column row still works.
const MATE_IN_1: &str = "000rZ,2kr1b1r/p1p2pp1/2pqb3/7p/3N2n1/2NPB3/PPP2PPP/R2Q1RK1 w - - 2 13,d4e6 d6h2,602,75,93,120,kingsideAttack mate mateIn1 oneMove opening";
const MATE_IN_2: &str = "000Zo,4r3/1k6/pp3r2/1b2P2p/3R1p2/P1R2P2/1P4PP/6K1 w - - 0 35,e5f6 e8e1 g1f2 e1f1,1363,75,93,120,endgame mate mateIn2 operaMate short";
const MATE_IN_3: &str = "001wR,6nr/pp3p1p/k1p5/8/1QN5/2P1P3/4KPqP/8 b - - 5 26,b7b5 b4a5 a6b7 c4d6 b7b8 a5d8,1152,75,93,120,endgame long mate mateIn3";
/// Kept with its real trailing columns, unlike the others, so `Row::of_csv` is
/// exercised against both a 10-column row and the truncated 8-column ones above.
const MATE_IN_4: &str = "01oPU,1r6/RP4k1/5bp1/2p1p1p1/Q1Pp2q1/3P1pP1/1P3P1K/7R w - - 3 36,a4c6 b8h8 h2g1 h8h1 g1h1 g4h3 h1g1 h3g2,1434,77,92,413,attraction endgame master mate mateIn4 veryLong,https://lichess.org/UN9LTxg5#71,";

/// A real row Lichess tags `mateIn4` whose stored line is **not linear**.
///
/// This is the project's whole thesis, on real data: Lichess's line records one
/// engine-chosen defense, and against that one defense the line does mate. Black has
/// another. Ship this and the user would draw four entirely reasonable arrows and be
/// told they were wrong.
///
/// It is not an oddity. Measured over the first 300 `mateIn4` rows of the 2026-07-01
/// dump, **196 of 300 are rejected exactly like this** — 34.7% survive, against the
/// 34.8% CLAUDE.md estimated from sampling. This one is simply the first.
const MATE_IN_4_NOT_LINEAR: &str = "00LRq,1k6/1p1q4/P2p3p/1NpPpn1Q/5b2/2P3r1/1P2B1P1/R6K b - - 3 28,g3g7 a6a7 b8a8 b5c7 d7c7 h5e8 c7b8 a7b8q,2253,75,93,120,advancedPawn discoveredCheck doubleCheck mate mateIn4 middlegame promotion queensideAttack sacrifice veryLong";

fn of(line: &str) -> puzzle::Puzzle {
    let row = lichess::Row::of_csv(line).expect("real row must split");
    lichess::of_row(&row).expect("real row must convert")
}

// ---------------------------------------------------------------------------
// The two facts.
// ---------------------------------------------------------------------------

/// The solver is the side to move *after* the setup move — the opposite of the raw
/// FEN's active colour.
#[test]
fn the_solver_is_the_opposite_colour_to_the_raw_fen() {
    for (line, raw_turn, solver) in [
        (MATE_IN_1, shakmaty::Color::White, shakmaty::Color::Black),
        (MATE_IN_2, shakmaty::Color::White, shakmaty::Color::Black),
        (MATE_IN_3, shakmaty::Color::Black, shakmaty::Color::White),
        (MATE_IN_4, shakmaty::Color::White, shakmaty::Color::Black),
        (
            MATE_IN_4_NOT_LINEAR,
            shakmaty::Color::Black,
            shakmaty::Color::White,
        ),
    ] {
        let row = lichess::Row::of_csv(line).expect("splits");
        let before = position::of_fen(row.fen).expect("raw fen is legal");
        assert_eq!(
            shakmaty::Position::turn(&before),
            raw_turn,
            "fixture premise: the raw FEN has {raw_turn:?} to move"
        );
        assert_eq!(
            of(line).solver().expect("parses"),
            solver,
            "the setup move flips it: the user plays {solver:?}"
        );
    }
}

/// `Moves[1..]` is the solution *line*, which alternates. Only the odd indices are
/// the user's arrows.
///
/// This is the test the first implementation failed. It took `Moves[1..]` whole, so
/// mate-in-2 came out with 3 arrows and mate-in-4 with 7 — the defender's replies
/// smuggled in as things the user is expected to draw.
#[test]
fn only_every_other_move_is_the_users() {
    for (line, moves_in_row, depth, first_arrow) in [
        (MATE_IN_1, 2, 1, "d6h2"),
        (MATE_IN_2, 4, 2, "e8e1"),
        (MATE_IN_3, 6, 3, "b4a5"),
        (MATE_IN_4, 8, 4, "b8h8"),
        (MATE_IN_4_NOT_LINEAR, 8, 4, "a6a7"),
    ] {
        let row = lichess::Row::of_csv(line).expect("splits");
        assert_eq!(
            row.moves.split_whitespace().count(),
            moves_in_row,
            "fixture premise: the row's line is {moves_in_row} moves"
        );

        let p = of(line);
        assert_eq!(
            p.solution.len(),
            depth,
            "{moves_in_row} moves in the row = 1 setup + {depth} solver + {} defence",
            depth - 1
        );
        assert_eq!(p.depth, depth);
        assert_eq!(
            p.solution[0].to_string(),
            first_arrow,
            "the user's first arrow is Moves[1], not Moves[0]"
        );
    }
}

/// The end-to-end claim: a real Lichess mate row, converted, is a real mate that
/// our own solver proves at the advertised depth.
///
/// Not every row survives — most mate-in-3s and -4s are not linear, which is the
/// whole reason this project re-proves them. These four happen to be, which makes
/// them a fair check that the *conversion* is right rather than merely plausible.
#[test]
fn real_rows_convert_into_puzzles_our_solver_proves() {
    for (line, depth) in [
        (MATE_IN_1, 1),
        (MATE_IN_2, 2),
        (MATE_IN_3, 3),
        (MATE_IN_4, 4),
    ] {
        let p = of(line);
        let pos = p.position().expect("converted fen is legal");
        assert_eq!(
            mate::judge(&pos, &p.solution),
            mate::Verdict::Mates { moves: depth },
            "row {} must mate in {depth} against every defence",
            p.id
        );
    }
}

/// And the full guard rail: `verify` re-proves legality, linearity, depth and
/// minimality from scratch. This is what stands between the dump and the app.
#[test]
fn real_rows_pass_verify() {
    for line in [MATE_IN_1, MATE_IN_2, MATE_IN_3, MATE_IN_4] {
        of(line).verify().expect("a real linear mate must verify");
    }
}

/// The reason this project exists, on a real row rather than a fixture.
///
/// Lichess tags `00LRq` mateIn4 and its stored line does mate — against the single
/// defense its generator's engine happened to play. Our solver plays every defense
/// and finds one that survives, so the puzzle is unusable here: the arrow UI cannot
/// branch, and a user who drew this line would be told they were wrong.
///
/// Nothing about the row is malformed, which is the point. It converts perfectly.
/// Only `verify` can tell it apart from a good one, and 196 of the first 300
/// `mateIn4` rows fail this same way.
#[test]
fn a_real_lichess_mate_in_4_that_is_not_linear_is_rejected() {
    let p = of(MATE_IN_4_NOT_LINEAR);
    assert_eq!(p.depth, 4, "the row converts cleanly; nothing is malformed");

    let pos = p.position().expect("legal");
    let v = mate::judge(&pos, &p.solution);
    assert!(
        v.refuted(),
        "some defense must survive Lichess's own line, got {v:?}"
    );
    assert!(
        matches!(p.verify(), Err(puzzle::Invalid::NotLinear { .. })),
        "and verify must be the thing that says so, got {:?}",
        p.verify()
    );
}

// ---------------------------------------------------------------------------
// Row parsing.
// ---------------------------------------------------------------------------

#[test]
fn reads_the_fields_it_needs() {
    let row = lichess::Row::of_csv(MATE_IN_2).expect("splits");
    assert_eq!(row.id, "000Zo");
    assert_eq!(row.rating, "1363");
    assert!(row.fen.starts_with("4r3/1k6/"));
    assert!(row.themes.contains("mateIn2"));
    assert_eq!(of(MATE_IN_2).rating, 1363);
}

#[test]
fn matches_whole_theme_tags_only() {
    let row = lichess::Row::of_csv(MATE_IN_2).expect("splits");
    assert!(row.has_theme("mateIn2"));
    assert!(row.has_theme("endgame"));
    assert!(
        !row.has_theme("mateIn1"),
        "must not match a different depth"
    );
    assert!(
        !row.has_theme("mate mateIn2"),
        "must not match across the separator"
    );
    // `mate` is a real tag on every mate puzzle and a prefix of `mateIn2`; whole-tag
    // matching has to keep those distinct in both directions.
    assert!(row.has_theme("mate"));
    assert!(!row.has_theme("mat"), "must not match a prefix of a tag");
}

#[test]
fn mate_theme_names_the_tag() {
    assert_eq!(lichess::mate_theme(2), "mateIn2");
    assert_eq!(lichess::mate_theme(4), "mateIn4");
}

// ---------------------------------------------------------------------------
// Rejection. Most of the 6M rows are rejected; it is the normal path.
// ---------------------------------------------------------------------------

#[test]
fn rejects_a_short_row() {
    assert!(matches!(
        lichess::Row::of_csv("a,b,c"),
        Err(lichess::Error::Columns { got: 3 })
    ));
}

#[test]
fn rejects_an_unparseable_fen() {
    let line = "id,not a fen,e2e4 e7e5,1500,75,93,120,mate mateIn1";
    let row = lichess::Row::of_csv(line).expect("splits");
    assert!(matches!(
        lichess::of_row(&row),
        Err(lichess::Error::Fen { .. })
    ));
}

/// A row whose setup move is not legal in its own FEN is a corrupt row, not a hard
/// puzzle. It must be named as such rather than silently skipped.
#[test]
fn rejects_a_setup_move_that_is_illegal() {
    let line = "id,6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1,h8h1 a1a8,1500,75,93,120,mate mateIn1";
    let row = lichess::Row::of_csv(line).expect("splits");
    assert!(matches!(
        lichess::of_row(&row),
        Err(lichess::Error::IllegalSetup { .. })
    ));
}

#[test]
fn rejects_a_row_with_only_a_setup_move() {
    let line = "id,6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1,g1h1,1500,75,93,120,mate mateIn1";
    let row = lichess::Row::of_csv(line).expect("splits");
    assert!(matches!(
        lichess::of_row(&row),
        Err(lichess::Error::NoSolution { .. })
    ));
}

#[test]
fn rejects_a_non_numeric_rating() {
    let line = "id,6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1,g1h1 a1a8,high,75,93,120,mate mateIn1";
    let row = lichess::Row::of_csv(line).expect("splits");
    assert!(matches!(
        lichess::of_row(&row),
        Err(lichess::Error::Rating { .. })
    ));
}

/// The conversion parses; it does not judge. A row tagged `mateIn2` whose line is
/// not actually a linear mate must convert cleanly and then fail `verify` — that
/// split is the whole architecture, and it is why the tag is only a prefilter.
#[test]
fn converts_a_bad_puzzle_and_lets_verify_reject_it() {
    // BRANCHING_BLOCKED with a setup move prepended: Black interposes and the line
    // dies, but nothing about the row itself is malformed.
    let line =
        "bad,7k/r7/8/5K2/8/8/8/1R6 w - - 0 1,f5f6 f6g6 a7b7 b1b8,1500,75,93,120,mate mateIn2";
    let row = lichess::Row::of_csv(line).expect("splits");
    let p = lichess::of_row(&row).expect("conversion is only parsing, and this parses");
    assert!(
        matches!(p.verify(), Err(puzzle::Invalid::NotLinear { .. })),
        "the tag claims mateIn2; our solver is what decides, got {:?}",
        p.verify()
    );
}

/// Guard the fixtures themselves: if a future dump changes these rows, the tests
/// above would quietly start proving something else.
#[test]
fn fixtures_are_the_rows_they_claim_to_be() {
    for (line, id, theme) in [
        (MATE_IN_1, "000rZ", "mateIn1"),
        (MATE_IN_2, "000Zo", "mateIn2"),
        (MATE_IN_3, "001wR", "mateIn3"),
        (MATE_IN_4, "01oPU", "mateIn4"),
        (MATE_IN_4_NOT_LINEAR, "00LRq", "mateIn4"),
    ] {
        let row = lichess::Row::of_csv(line).expect("splits");
        assert_eq!(row.id, id);
        assert!(row.has_theme(theme), "{id} must carry {theme}");
        assert!(row.has_theme("mate"));
    }
}

/// `common` is used by the other test binaries; touch it so this one compiles
/// without warnings about the shared module.
#[test]
fn shared_fixtures_still_parse() {
    let _ = common::pos(common::BACK_RANK);
}
