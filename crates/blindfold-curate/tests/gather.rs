//! Tests for the streaming candidate filter.
//!
//! `gather` decides what the database is drawn from, and every one of its rejections
//! is invisible in the output: a puzzle that was never gathered leaves no trace. It
//! used to live in `main.rs`, where none of this could be tested without a 302 MB
//! download.
//!
//! The rows are **verbatim real dump rows**, and their square counts and clocks are
//! measured, not eyeballed. Hand-built FENs are how this repo has wasted whole cycles
//! before: the first draft of this file claimed a row it invented was "8 squares" when
//! it was 18, which the gate rejects — the test would have failed for a reason that
//! had nothing to do with the code.

use blindfold_curate::constants;
use blindfold_curate::gather;

/// Real `mateIn2`. 17 squares raw, 16 after the setup move — over the roster gate
/// either way.
const HEAVY_MATE_IN_2: &str = "000Zo,4r3/1k6/pp3r2/1b2P2p/3R1p2/P1R2P2/1P4PP/6K1 w - - 0 35,e5f6 e8e1 g1f2 e1f1,1363,75,93,120,endgame mate mateIn2 operaMate short";

/// Real `mateIn1`, 6 squares — a rook endgame, the shape this trainer wants.
const LIGHT_MATE_IN_1: &str = "00C7m,8/5k2/1P4R1/6PK/1r6/8/8/8 w - - 1 58,h5h6 b4h4,745,119,67,62,endgame mate mateIn1 oneMove rookEndgame";

/// Real `mateIn1`, 4 squares — the sparsest shape in the dump.
const SPARSE_MATE_IN_1: &str = "00T85,8/8/8/8/8/4K3/1k3Q2/1q6 b - - 5 53,b2c1 f2d2,1039,92,93,1528,endgame master mate mateIn1 oneMove queenEndgame";

/// Real `mateIn1` whose setup move `f6h8` is a **capture**: 15 squares in the row's
/// FEN, 14 in the position the user is actually shown. Sits astride the gate, so it
/// is kept only by a gate that measures after the setup move.
const CAPTURE_SETUP_MATE_IN_1: &str = "00AfZ,2r3kR/Q7/5q2/1brpN3/5Pp1/4P1P1/6K1/1B6 b - - 2 43,f6h8 a7f7,978,76,100,536,master mate mateIn1 middlegame oneMove";

fn pool_of(rows: &[&str]) -> gather::Pool {
    let text = rows.join("\n");
    gather::of_rows(std::io::BufReader::new(text.as_bytes()), |_| {}).expect("in-memory read")
}

fn ids(pool: &gather::Pool, depth: usize) -> Vec<String> {
    pool.by_depth
        .get(&depth)
        .map(|ps| ps.iter().map(|p| p.id.clone()).collect())
        .unwrap_or_default()
}

/// A real non-mate row is not a rejection — it is simply not a candidate, and must not
/// be tallied as malformed.
#[test]
fn rows_without_a_mate_tag_are_skipped_silently() {
    let row = "00008,r6k/pp2r2p/4Rp1Q/3p4/8/1N1P2R1/PqP2bPP/7K b - - 0 24,f2g3 e6e7 b2b1 b3c1 b1c1 h6c1,1852,74,96,11995,crushing hangingPiece long middlegame";
    let pool = pool_of(&[row]);
    assert_eq!(pool.scanned, 1);
    assert_eq!(
        pool.rejected.total(),
        0,
        "a non-mate row is not a rejection"
    );
    assert!(pool.by_depth.is_empty());
}

/// The gate that makes this a blindfold trainer rather than a memory test. Without it
/// the first cut of this database shipped a mate-in-**one** with all 32 pieces on the
/// board, rated 1029.
#[test]
fn rows_whose_roster_is_too_heavy_to_hold_are_rejected() {
    let pool = pool_of(&[HEAVY_MATE_IN_2]);
    assert_eq!(pool.rejected.too_heavy, 1, "16 squares is over the gate");
    assert!(ids(&pool, 2).is_empty());
}

#[test]
fn light_rows_become_candidates() {
    let pool = pool_of(&[LIGHT_MATE_IN_1, SPARSE_MATE_IN_1]);
    assert_eq!(pool.rejected.total(), 0);
    assert_eq!(ids(&pool, 1), ["00C7m", "00T85"]);
}

/// The roster gate must measure the position the *user is shown* — after the setup
/// move — not the row's FEN, which is one ply earlier and may still hold a piece the
/// setup move captures. One piece is enough to flip a puzzle across the gate.
#[test]
fn the_roster_gate_measures_the_position_after_the_setup_move() {
    assert_eq!(
        constants::MAX_ROSTER_SQUARES,
        14,
        "this fixture straddles a gate of 14 (15 raw, 14 shown); re-pick it if the \
         gate moves"
    );
    let pool = pool_of(&[CAPTURE_SETUP_MATE_IN_1]);
    assert_eq!(
        ids(&pool, 1),
        ["00AfZ"],
        "the setup move captures, bringing the roster the user sees down to the gate"
    );
}

/// `LIGHT_MATE_IN_1` with its FEN's halfmove clock set to `row_clock`.
///
/// Note the gate reads the clock of the position the user is **shown**, which is the
/// row's clock **plus one**: this row's setup move `h5h6` is a quiet king move, and a
/// quiet ply advances the clock. That off-by-one is the whole reason this helper
/// exists — the first draft of these tests set the row's clock and asserted against
/// the gate's threshold directly, and was off by exactly one ply.
fn row_with_clock(row_clock: u32) -> String {
    LIGHT_MATE_IN_1.replace("w - - 1 58", &format!("w - - {row_clock} 58"))
}

/// shakmaty has no 50-move rule, so a high clock lets the defender claim a draw our
/// solver cannot see. Rejecting it here keeps `judge` a pure function of the four
/// things the roster carries.
#[test]
fn rows_whose_halfmove_clock_allows_a_claimable_draw_are_rejected() {
    // Shown clock lands exactly on the threshold.
    let pool = pool_of(&[&row_with_clock(constants::MAX_HALFMOVE_CLOCK - 1)]);
    assert_eq!(pool.rejected.drawish, 1);
    assert!(ids(&pool, 1).is_empty());
}

/// The boundary is exclusive: a shown clock of 93 is safe, 94 is not. CLAUDE.md
/// derives why it is 94 and not `100 - 7` — the mating ply is the solver's, and mate
/// ends the game — and this is what stops that derivation being quietly rounded off.
#[test]
fn the_halfmove_boundary_is_exactly_where_it_is_documented() {
    let pool = pool_of(&[&row_with_clock(constants::MAX_HALFMOVE_CLOCK - 2)]);
    assert_eq!(
        pool.rejected.drawish, 0,
        "a shown clock of 93 is still safe"
    );
    assert_eq!(ids(&pool, 1), ["00C7m"]);
}

/// The gate measures the shown position, not the row — the same rule as the roster
/// gate, and for the same reason: the row's FEN is a ply early. Pinned separately
/// because it is invisible in the two tests above, which would both still pass if the
/// gate read `row.fen` and the threshold were shifted by one to compensate.
#[test]
fn the_halfmove_gate_measures_the_position_after_the_setup_move() {
    // Row clock 93: safe by the row, over the line once the quiet setup move ticks it.
    let row = row_with_clock(constants::MAX_HALFMOVE_CLOCK - 1);
    assert!(
        row.contains(&format!("- {} 58", constants::MAX_HALFMOVE_CLOCK - 1)),
        "the row itself is below the threshold"
    );
    let pool = pool_of(&[&row]);
    assert_eq!(
        pool.rejected.drawish, 1,
        "a gate reading the row's clock would have kept this"
    );
}

/// A row whose line length disagrees with its own theme tag. Tallied apart from
/// malformed rows: one means the dump is corrupt, the other that it is merely
/// inconsistent, and lumping them together hides the first behind the second.
#[test]
fn a_row_whose_line_length_contradicts_its_tag_is_mislabelled_not_malformed() {
    // A real mateIn2 line, retagged mateIn1: `of_row` yields depth 2 either way.
    let row = HEAVY_MATE_IN_2.replace("mateIn2", "mateIn1");
    let pool = pool_of(&[&row]);
    assert_eq!(pool.rejected.mislabelled, 1);
    assert_eq!(pool.rejected.malformed, 0);
}

#[test]
fn a_malformed_row_is_tallied_as_malformed() {
    let pool = pool_of(&["nonsense,mateIn1", "also,bad,,,,,,mateIn1 mate"]);
    assert!(pool.rejected.malformed >= 1);
    assert_eq!(pool.rejected.too_heavy, 0);
    assert_eq!(pool.rejected.drawish, 0);
}

#[test]
fn every_row_is_counted_as_scanned() {
    let pool = pool_of(&[LIGHT_MATE_IN_1, HEAVY_MATE_IN_2, "not,a,mate,row"]);
    assert_eq!(pool.scanned, 3);
}

/// `is_full` drives the early break, so a wrong answer either stops the scan before the
/// buckets are full or reads all 6M rows for nothing.
#[test]
fn a_pool_is_not_full_until_every_depth_is() {
    let pool = pool_of(&[LIGHT_MATE_IN_1]);
    assert!(
        !gather::is_full(&pool),
        "one candidate at one depth is not a full pool"
    );
}
