//! Named constants for the core crate.
//!
//! This is a *category* module, not a concept module, and it is that way on
//! purpose. Nothing in here is shared between modules — `MAX_LINE` and
//! `MAX_FRONTIER` belong to `mate`, `MAX_DEPTH` to `puzzle`, `PROMOTABLE` to
//! `arrow` — so grouping them by "is a constant" rather than by what they are
//! about cuts against how the rest of the crate is organised. It is done anyway
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
/// A branch is a `(Chess, Vec<Arrow>)`, measured at 160 bytes plus the `Vec`'s
/// heap. An unrefuted line reaches ~30M of them — over 4.8 GiB, past wasm32's
/// whole 4 GB address space — within about six seconds. This cap puts the ceiling
/// near 170 MB, about the most a browser heap should be asked to absorb.
pub const MAX_FRONTIER: usize = 1 << 20;
