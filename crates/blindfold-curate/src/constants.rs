//! Named constants for the curation tool.
//!
//! Separate from `blindfold_core::constants`, which holds facts about *chess*. These
//! are policy: what we want a puzzle set to look like and where it goes. Nothing here
//! can change whether a puzzle is correct, only which correct ones we keep.

/// Alias for core's theme prefix, so the cheap pre-filter and the real theme test
/// cannot drift apart. Aliased rather than re-typed: `"mateIn"` written twice is
/// two things to keep in step.
pub const MATE_THEME_HINT: &str = blindfold_core::constants::LICHESS_MATE_THEME_PREFIX;

/// Mate depths we curate, and the order the database files are written in.
///
/// The length is tied to core's ceiling: `Puzzle::verify` rejects anything deeper
/// than `MAX_DEPTH`, so a sixth entry here would produce a tier where every puzzle
/// fails verification and the file comes out empty. Better a compile error. (And it
/// *would* be empty: the dump has zero roster-≤10 mate-in-6 candidates — see
/// `MAX_DEPTH`'s doc.)
pub const DEPTHS: [usize; blindfold_core::constants::MAX_DEPTH] = [1, 2, 3, 4, 5];

/// How many verified puzzles to keep per depth — a *target ceiling*, not a promise.
///
/// The abundant tiers (mate-in-1, -2, -3) hit it exactly; the scarce tiers ship all they
/// have, which is fewer. Measured over the whole dump at [`MAX_ROSTER_SQUARES`], the usable
/// pool is ~6000+ / 5772 / 3137 / **271** / **56** for depths 1-5, so mate-in-4 keeps 271
/// and mate-in-5 keeps 56 no matter how high this is set — [`select::by_rating_spread`](
/// crate::select::by_rating_spread) returns everything when the pool is smaller than the
/// target. So this constant governs only depths 1-3; the hard tiers are pool-limited.
///
/// 400 (up from an earlier 100) because "more puzzles" was the ask and the cost is trivial:
/// verification is ~13 ms per mate-in-4 and `run` parallelizes it, so the whole regeneration
/// is seconds, and 400 × ~115 bytes per tier is a few tens of KiB in the wasm bundle. The
/// even rating spread means a larger target simply samples the hard end more densely too —
/// no separate "hard bias" is needed, and none is wanted, because the app draws puzzles near
/// the user's Elo and so must keep the easy end for weak users as much as the hard end for
/// strong ones.
pub const PER_DEPTH: usize = 400;

/// The fewest puzzles a depth's file may hold before a regeneration is considered broken.
///
/// A floor, not a target: the abundant tiers land on [`PER_DEPTH`] and the scarce ones on
/// their whole verified pool (271, 56), so this only fires if a tier *collapses* — a gate
/// bug, a truncated dump, a bad merge. Set below the thinnest real tier (mate-in-5's 56) with
/// margin. `tests/database.rs` asserts every file lands in `[MIN_PER_DEPTH, PER_DEPTH]`.
pub const MIN_PER_DEPTH: usize = 50;

/// How many candidates to gather per depth before verifying.
///
/// A **ceiling on work, not a target**. Gates are applied before the pool, so these
/// are already roster- and clock-filtered rows; the number only bounds how many the
/// abundant tiers bother to collect. Mate-in-1 and mate-in-2 hit it early; mate-in-3
/// and mate-in-4 never do — at [`MAX_ROSTER_SQUARES`] there are only ~766 mate-in-4
/// rows in the whole dump, so those tiers read the file to EOF and take what exists.
///
/// It replaced a value of 400, which was sized for *survival* — "400 × ~35% ≈ 140,
/// comfortably past `PER_DEPTH`" — and that is the wrong target: a pool of 140 to pick
/// 100 from is a 71% keep rate, at which `select` stops selecting and starts rounding
/// down. What matters is choice, and the scarce tiers get it only by reading
/// everything.
///
/// The economy the old value protected was imaginary: verification is ~13 ms for a
/// mate-in-4 and `run` already parallelizes it, so the whole run is seconds.
pub const CANDIDATES_PER_DEPTH: usize = 6_000;

/// The most squares a puzzle's roster may name.
///
/// The gate that makes this a *blindfold* trainer rather than a memory test. The user
/// never sees the board, so every occupied square is something they must hold in their
/// head before they can begin to think about mate. Chess validity does not bound this
/// at all: the first cut of this database shipped a mate-in-**one** with all 32 pieces
/// on the board, rated 1029, whose roster ran to twelve lines.
///
/// **10 is measured, not guessed**, and the measurement is the whole dump — every
/// `mateInN` row converted, clock-gated, and re-proved. Verified survivors by gate:
///
/// ```text
/// gate  mate-in-1  mate-in-2  mate-in-3  mate-in-4  mate-in-5
///  <=8     21,855     14,461      1,384        131         ?
///  <=10    45,510     34,275      3,450        271        56
///  <=14   157,258    161,399     17,812      1,242         ?
/// ```
///
/// Mate-in-5 is now the binding tier at 56 (mate-in-6 has *zero* candidates at any of
/// these gates, which is why the depth ceiling is 5). We keep all 56, and all 271
/// mate-in-4 — the scarce tiers are pool-limited, not gate-limited, so relaxing this
/// would only pull in heavier rosters, not more hard puzzles. At ≤10 the median roster
/// is 9. Do not raise it without re-running the numbers; a looser gate costs the user
/// a position they cannot hold in their head, for no gain at the depths that matter.
///
/// An earlier draft of this constant was 14 and said a gate near 10 was "simply not
/// reachable at `PER_DEPTH` for mate-in-4". That was asserted, never measured, and it
/// is false by 2.7x — the table above is what it should have been. Do not re-raise
/// this without re-running the numbers; a looser gate costs the user directly.
///
/// `each_puzzle_fits_in_a_head` in `tests/database.rs` is what holds it.
pub const MAX_ROSTER_SQUARES: usize = 10;

/// Reject a candidate at depth `depth` whose halfmove clock is this high or higher.
///
/// **Depth-aware, and it has to be.** shakmaty implements no 50-move rule, so our solver
/// cannot see a draw the defender could *claim*. The clock climbs one per ply, so a deeper
/// mate lets the defender reach a higher clock on their last turn before the mate — a
/// deeper mate needs a *stricter* gate. Measured on the position the user is **shown** (the
/// one after the row's setup move), whose clock is the `C` in CLAUDE.md's derivation.
///
/// The binding ply is the defender's *last* move, at ply `2(depth-1)`, whose clock is
/// `C + 2·depth − 3`. It is claimable under FIDE 9.3(a) once that reaches 99 (declare a
/// move making it 100), so the mate stays real only while `C + 2·depth − 3 ≤ 98`, i.e.
/// `C ≤ 101 − 2·depth`. Reject at `102 − 2·depth`:
///
/// ```text
/// depth   1    2    3    4    5
/// gate  100   98   96   94   92
/// ```
///
/// It is **not** `100 − plies`: the mating ply is the solver's, and mate ends the game
/// (FIDE 5.1.1), so the binding ply is the defender's last turn, not the mate. Depth 4's
/// 94 is the value this was before mate-in-5 was added; 5 needs 92, and applying 94 to it
/// would be too loose. (In practice moot — the deepest committed clock is 58, at mate-in-1
/// — but the gate must be correct for the depth we ship, not the depth we used to.) Read
/// the derivation in CLAUDE.md before touching this.
///
/// It lives in curation rather than in `judge`, which must stay a pure function of
/// exactly the four things the roster carries. A `const fn` so it stays a compile-time
/// policy value, one per depth.
pub const fn max_halfmove_clock(depth: usize) -> u32 {
    (102 - 2 * depth) as u32
}

/// The narrowest rating range a depth's file may span before we call the spread
/// broken.
///
/// A tripwire on [`select::by_rating_spread`](crate::select::by_rating_spread), not a
/// target: real spreads run ~1400-1900, so this fires only on a collapse, not on
/// drift.
pub const MIN_RATING_SPAN: u32 = 500;

/// Where the curated database is written, relative to the workspace root.
pub const DATABASE_DIR: &str = "database";

/// The file a depth's puzzles live in: `mate_in_2.jsonl`.
///
/// A function rather than a `FILE_STEM` constant because the stem was never the whole
/// name — the `_` and the `.jsonl` were re-typed at every call site, so a reader and
/// a writer could still drift on the two parts the constant did not cover.
pub fn file_name(depth: usize) -> String {
    format!("mate_in_{depth}.jsonl")
}

/// Progress is printed every this many rows. The dump is ~6M lines and a silent
/// multi-minute scan is indistinguishable from a hang.
pub const PROGRESS_EVERY: usize = 500_000;
