//! Named constants for the core crate.

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
/// Measured: an unrefuted line reaches ~30M branches (~5 GiB) within about six
/// seconds, which is past wasm32's 4 GB address space. ~1M branches is roughly
/// 150 MB, the most a browser heap should be asked to absorb.
pub const MAX_FRONTIER: usize = 1 << 20;
