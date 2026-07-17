//! Shared fixtures for the core test suite.
//!
//! Each integration test binary compiles this module in full, so anything only
//! some of them use looks dead to the rest. That is inherent to `mod common` and
//! not a signal worth acting on.
#![allow(dead_code)]

use blindfold_core::arrow;
use blindfold_core::position;

/// Parse a FEN, panicking with a useful message. Tests only.
pub fn pos(fen: &str) -> shakmaty::Chess {
    position::of_fen(fen).unwrap_or_else(|e| panic!("bad test FEN `{fen}`: {e}"))
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
/// move* `Kg8` the line mates perfectly — see the test
/// `a_single_defense_can_hide_non_linearity`. A filter that trusted the Lichess
/// line, which records exactly one engine-chosen defense, would happily ship this
/// puzzle, and the user would draw two entirely reasonable arrows and be told
/// they were wrong.
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

/// White Ba1, Kg1; Black Ka8, bishops b7 e6 c4 g4. White to move.
///
/// A position engineered so that **no defense ever refutes** and the frontier
/// grows without bound. White's dark-squared bishop shuffles a1<->b2; all four
/// black bishops are light-squared, so by construction they can never occupy
/// a1/b2, never capture the shuffling bishop, and never check a king on dark g1.
///
/// Nothing here mates — the point is the cost. Measured frontier growth is ~32x
/// per ply: ~933k branches at ply 4, ~30M (~5 GiB) at ply 5, reached in about six
/// seconds. On wasm32 that is past the 4 GB address space. Used to prove the
/// frontier bound actually fires.
pub const UNBOUNDED_FRONTIER: &str = "k7/1b6/4b3/8/2b3b1/8/8/B5K1 w - - 0 1";

/// [`BACK_RANK`] with only the side to move flipped, so Black is to move and has
/// nothing to do. Named rather than inlined: the placement must stay identical to
/// [`BACK_RANK`] for the roster comparison to mean anything.
pub const BACK_RANK_IDLE: &str = "6k1/5ppp/8/8/8/8/8/R5K1 b - - 0 1";

/// Black Ra8, Kg8; White Kg1, pawns f2 g2 h2. **Black** to move. `Ra1#`.
///
/// [`BACK_RANK`] mirrored. Every other fixture here has White to move, so
/// nothing else would notice a solver that assumed the solver is White.
pub const BACK_RANK_BLACK: &str = "r5k1/8/8/8/8/8/5PPP/6K1 b - - 0 1";

/// White Ka4, Qc7, pawn b7; Black Ka6. White to move.
///
/// **Underpromotion-only mate in 1**: `b8=N#` mates (the knight checks a6 and is
/// covered by Qc7, which also takes a7/b6/b7), while `b8=Q` is *stalemate* — the
/// queen on b8 gives no check and takes the last flight square.
///
/// The one shape where a solver that quietly assumes "promotion means queen"
/// gives the wrong answer in both directions at once: it would miss the only mate
/// *and* offer a draw as the solution. Found by brute-force enumeration rather
/// than by hand.
pub const UNDERPROMOTION: &str = "8/1PQ5/k7/8/K7/8/8/8 w - - 0 1";

/// White Kc1, Qa6, pawn a5; Black Ka1, pawn b5 having just double-pushed, so b6
/// is the en-passant square. White to move.
///
/// **Mate in 1 by en-passant capture**: `axb6#`, and the check is *discovered* —
/// the capturing pawn vacates a5 and opens Qa6's line down the a-file onto the
/// king. Only an e.p. capture both leaves the a-file and removes the b5 pawn, so
/// this is a shape no other fixture reaches: the arrow `a5b6` lands on an *empty*
/// square and captures a pawn that is not on it.
pub const EN_PASSANT_MATE: &str = "8/8/Q7/Pp6/8/8/8/k1K5 w - b6 0 1";

/// White Ra1, Ke1 (queenside rights), Rc2, Qf4; Black Kd3. White to move.
///
/// **Castling is the only mate in 1**: `O-O-O#` — the rook lands on d1 and checks
/// down the d-file while the king tucks to c1, covering Rc2. Verified by
/// enumeration that no non-castling move mates, so a solver that could not
/// express a castle would report this position as having no mate at all.
pub const CASTLING_MATE: &str = "8/8/8/8/5Q2/3k4/2R5/R3K3 w Q - 0 1";

/// White Ng5, Kg1; Black Kh8, Rg8, pawns g7 h7. White to move. `Nf7#`.
///
/// Smothered mate: the mating piece is not defended and the king is boxed in
/// entirely by its own pieces.
pub const SMOTHERED: &str = "6rk/6pp/8/6N1/8/8/8/6K1 w - - 0 1";
