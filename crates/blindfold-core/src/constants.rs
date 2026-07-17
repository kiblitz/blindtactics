//! Named constants for the core crate.
//!
//! This is a *category* module, not a concept module, and it is that way on
//! purpose. Nothing in here is shared between modules — `MAX_LINE` and
//! `MAX_FRONTIER` belong to `mate`, `MAX_DEPTH` to `puzzle`, `PROMOTABLE` to
//! `arrow`, `ANNOUNCE_ORDER` to `roster`; all five, and there is no sixth — so
//! grouping them by "is a constant" rather than by what they are about cuts
//! against how the rest of the crate is organised. It is done anyway
//! because the project's standing rule is that constants live in a dedicated
//! constants module, and a rule followed unevenly is worse than either choice
//! made consistently: the moment some constants live here and others live next to
//! their use site, "where does this go?" becomes a judgement call on every commit.
//! Reviewers reach for this file periodically — the answer is that it is settled,
//! not overlooked.

/// Roles a pawn may promote to.
///
/// `shakmaty::Role::from_char` will happily hand back `King` or `Pawn`, so it is
/// not a promotion validator on its own and every parse filters against this.
pub const PROMOTABLE: [shakmaty::Role; 4] = [
    shakmaty::Role::Queen,
    shakmaty::Role::Rook,
    shakmaty::Role::Bishop,
    shakmaty::Role::Knight,
];

/// Columns a Lichess puzzle row must have before we will read it.
///
/// The real header is
/// `PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes,GameUrl,OpeningTags`
/// — ten columns. Eight is what we *need*: `Themes` is at index 7 and is the last
/// field [`crate::lichess::Row`] reads, so a row missing only the trailing
/// `GameUrl` / `OpeningTags` is still perfectly usable and should not be dropped.
pub const LICHESS_MIN_COLUMNS: usize = 8;

/// Prefix of the Lichess theme tag for mates: `mateIn1`, `mateIn2`, ...
///
/// A candidate filter only. The tag is not evidence that a line is linear, or even
/// that it is the shortest mate — see [`crate::lichess`].
pub const LICHESS_MATE_THEME_PREFIX: &str = "mateIn";

/// The order pieces are announced in: the king anchors the position, then
/// descending value.
///
/// `shakmaty::Role`'s own ordering runs the other way (pawn first), so it cannot
/// be used here.
pub const ANNOUNCE_ORDER: [shakmaty::Role; 6] = [
    shakmaty::Role::King,
    shakmaty::Role::Queen,
    shakmaty::Role::Rook,
    shakmaty::Role::Bishop,
    shakmaty::Role::Knight,
    shakmaty::Role::Pawn,
];

/// The deepest mate this trainer deals in.
///
/// Puzzles are mate-in-1 through mate-in-4. This is a real ceiling, not a
/// preference: the linearity search costs roughly 30x per extra ply, so an
/// unbounded depth is indistinguishable from a hang.
pub const MAX_DEPTH: usize = 4;

/// The longest submitted line worth judging.
///
/// A correct line is at most [`MAX_DEPTH`] arrows, but nothing stops a user from
/// drawing more. Judging is breadth-first over every legal defense and the
/// frontier grows ~30x per ply, so this is what stands between a long submission
/// and an out-of-memory abort. Set above [`MAX_DEPTH`] so an over-long-but-
/// plausible attempt is still played out and told honestly that it does not mate.
pub const MAX_LINE: usize = 8;

/// Ceiling on live defenses tracked while judging a line.
///
/// Without a cap this is unbounded: an unrefuted line reaches ~30M branches, over
/// 4.8 GiB, within about six seconds — past wasm32's entire 4 GB address space.
///
/// Sizing it is not `MAX_FRONTIER * size_of::<Branch>()`, which is the error this
/// comment made twice before anyone measured it. A branch is 160 bytes flat plus a
/// small `defense` allocation, but `judge` also holds the *old* frontier while
/// pushing the new one into a `Vec` that doubles as it grows, so real peak runs
/// well over twice the flat size. `1 << 20` computed to "about 150 MB" on paper and
/// cost **527 MB** in practice. On wasm that is worse than it sounds: linear memory
/// never shrinks back, so one pathological submission would hold half a gigabyte
/// for the rest of the session.
///
/// `1 << 18` measures at **97 MB** peak working set (`examples/frontier_memory.rs`,
/// on the `UNBOUNDED_FRONTIER` fixture) against a 40 MB flat frontier.
///
/// Why it does not reject legitimate work. Two arguments, and it matters which is
/// which — an earlier version of this doc ran them together and overclaimed.
///
/// **Proven, for a solution.** Only three plies of a *solution* ever generate a
/// frontier: the last arrow is `is_last`, so `judge` returns without pushing, and
/// [`MAX_DEPTH`] = 4 leaves three advancing plies. Measured growth on
/// `UNBOUNDED_FRONTIER` (no defense ever refutes it, so nothing prunes) is
/// `[30, 926, 29203, 933297, ...]`, ~32x per ply. Reaching the bound takes **five
/// or more arrows**, and no solution has them.
///
/// Note the scope. `judge` itself bounds submissions by [`MAX_LINE`] = 8, not by
/// [`MAX_DEPTH`], so a *user* may draw 5-8 arrows and build four columns or more —
/// `examples/frontier_memory.rs` does exactly that on purpose. That is the case the
/// bound exists for, and it is covered by the fail-safe paragraph below, not by
/// this one.
///
/// **Empirical.** What remains is the third column, and that is a measurement, not
/// a theorem. Sweeping the one shape immune to the usual self-correction — black
/// light-squared bishops against an all-dark mating line, so they can never capture
/// the mating piece or interpose — the widest third-ply frontier found is **63,308**
/// (8 bishops, absurd material), about **4x** clear of the bound. That is the worst
/// *constructed*, not the worst *possible*. Sustaining ~30x per ply normally needs
/// long-range defenders, and long-range is exactly what lets them refute the line
/// and collapse the frontier, which is why this is hard to push higher.
///
/// **And if the empirical part is ever wrong, nothing breaks.** Exceeding the bound
/// yields [`crate::mate::Verdict::TooComplex`], which is not a refutation — the user
/// is never told they were wrong. Curation calls the same `judge` under the same
/// constant, and `verify` demands `Mates`, so a puzzle that trips this bound fails
/// verification and never reaches the database. The app is therefore never asked to
/// judge one. That shared constant is the whole safety property, and it is why both
/// crates must read it from here rather than pick their own. (Neither exists yet;
/// this is the intent they are to be built to.)
pub const MAX_FRONTIER: usize = 1 << 18;
