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
/// It cannot reject a legitimate puzzle or solve, and the reason is structural, not
/// a matter of headroom. Measured frontier by ply on that fixture — which is built
/// so no defense *ever* refutes, the worst case there is — runs
/// `[30, 926, 29203, 933297, 30105423]`. The bound is first reachable at ply 4, so
/// it takes a line of **five or more arrows** to trip. [`MAX_DEPTH`] caps a
/// solution at four. A legitimate line therefore never gets past the ply-3 column,
/// where the worst case is ~29k — 9x clear of the bound.
///
/// Trying to beat that on purpose is self-defeating: sustaining ~30x per ply needs
/// black's spare pieces to be long-range, and long-range is exactly what lets them
/// capture the mating piece or interpose. The one shape immune to it (black
/// light-squared bishops against an all-dark mating line) peaks at ~52k with absurd
/// material, still 5x under. So the bound only ever fires on input that was never
/// going to mate, and there, giving up sooner is strictly better.
///
/// Curation and the app are to share this constant rather than pick their own —
/// neither exists yet, so this is intent — because that is what keeps them
/// agreeing: a puzzle the curation tool could only verify by exceeding this bound
/// is one the browser could not verify either, so it must not reach the database.
pub const MAX_FRONTIER: usize = 1 << 18;
