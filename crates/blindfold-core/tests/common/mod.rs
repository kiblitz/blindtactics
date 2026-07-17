//! Shared fixtures for the core test suite.
//!
//! Each integration test binary compiles this module in full, so anything only
//! some of them use looks dead to the rest. That is inherent to `mod common` and
//! not a signal worth acting on.
#![allow(dead_code)]

use blindfold_core::arrow;
use blindfold_core::puzzle;

/// Parse a FEN, panicking with a useful message. Tests only.
pub fn pos(fen: &str) -> shakmaty::Chess {
    puzzle::position_of_fen(fen).unwrap_or_else(|e| panic!("bad test FEN `{fen}`: {e}"))
}

/// Parse a space-separated UCI line into arrows. Tests only.
pub fn line(uci: &str) -> Vec<arrow::Arrow> {
    uci.split_whitespace()
        .map(|t| {
            t.parse()
                .unwrap_or_else(|e| panic!("bad test arrow `{t}`: {e}"))
        })
        .collect()
}

/// Parse a single arrow. Tests only.
pub fn a(uci: &str) -> arrow::Arrow {
    uci.parse()
        .unwrap_or_else(|e| panic!("bad test arrow `{uci}`: {e}"))
}

// ---------------------------------------------------------------------------
// Positions
//
// Each is hand-built to isolate one property. The two BRANCHING_* positions are
// the same idea with one pawn moved, and they are the heart of the suite: they
// are what "linear" does and does not mean.
// ---------------------------------------------------------------------------

/// White Kf6, Rb1; Black Kh8, pawns a7 c7. White to move.
///
/// Linear mate in 2: `Kg6` then `Rb8#`.
///
/// The point: after the quiet `Kg6`, Black has *five* legal defenses (`Kg8`,
/// `a6`, `a5`, `c6`, `c5`) and `Rb8#` mates against every one. Branching exists
/// but is invisible to the user, which is exactly the property the arrow UI
/// needs.
///
/// It also doubles as a soundness check on the search: `Kg6` is a *quiet* move,
/// so any "only consider checking moves" pruning would fail to find this.
pub const BRANCHING_LINEAR: &str = "7k/p1p5/5K2/8/8/8/8/1R6 w - - 0 1";

/// White Kf6, Rb1; Black Kh8, rook a7. White to move.
///
/// The same shape as [`BRANCHING_LINEAR`], but Black has a rook that can reach
/// the b-file. Now `Kg6` `Rb8#` is NOT linear: Black answers `Rb7`, which blocks
/// the file and makes the second arrow flatly illegal.
///
/// This is the most important negative test in the project. Against the *king
/// move* `Kg8` the line mates perfectly — see `a_single_defense_can_hide_non_-
/// linearity`. A filter that trusted the Lichess line, which records exactly one
/// engine-chosen defense, would happily ship this puzzle, and the user would draw
/// two entirely reasonable arrows and be told they were wrong.
///
/// (An earlier draft of this fixture tried to block the file with a pawn push.
/// That is impossible: a pawn can only reach the b-file if it starts there, in
/// which case the rook was already blocked at move one. Blocking mid-line
/// requires a piece that can change file.)
pub const BRANCHING_BLOCKED: &str = "7k/r7/5K2/8/8/8/8/1R6 w - - 0 1";

/// White Ra1, Rb1, Kg1; Black Kh8. White to move.
///
/// Linear mate in 2 by rook ladder: `Ra7` (quiet, and Black's reply `Kg8` is
/// forced — the only legal move) then `Rb8#`.
pub const LADDER: &str = "7k/8/8/8/8/8/8/RR4K1 w - - 0 1";

/// White Kb6, Qg7; Black Ka8. White to move.
///
/// `Qb7#` mates. `Qc7` **stalemates** — Black is not in check and has no legal
/// move. The classic mate-solver bug is treating "no legal moves" as a win; this
/// position catches it.
///
/// The queen sits on g7 rather than somewhere closer because it needs a straight
/// line to *both* b7 and c7, and the rank is the only one that offers both.
pub const STALEMATE_TRAP: &str = "k7/6Q1/1K6/8/8/8/8/8 w - - 0 1";

/// White Ra1, Kg1; Black Kg8, pawns f7 g7 h7. White to move. `Ra8#`.
pub const BACK_RANK: &str = "6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1";
