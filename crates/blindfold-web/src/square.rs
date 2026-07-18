//! Where a square is on screen, and which square a pointer is over.
//!
//! Its own module, with no Leptos in it, because this is the one piece of the UI
//! that is arithmetic rather than markup — and it is arithmetic with two ways to
//! be silently wrong. Ranks run bottom-up in chess and top-down in a browser, and
//! the board is mirrored when Black is to play; both are sign flips that produce
//! a board which looks entirely plausible and puts every piece on the wrong
//! square. `tests/square.rs` pins the corners in both orientations.
//!
//! Everything here is in *fractions* (0.0-1.0 across the board) or viewBox units,
//! never pixels. The board is a CSS `aspect-ratio: 1` box of whatever size the
//! layout gives it, so pixels are not knowable here and not needed.

use crate::constants;

/// Which side of the board the user is sitting on.
///
/// Explicit rather than a `flipped: bool`, so call sites read as the chess fact
/// they encode rather than as a rendering detail. The wrapped colour is whichever
/// side is drawn along the bottom edge — the solver's by default, but the
/// point-of-view setting and the per-puzzle flip can seat either side there (see
/// [`crate::settings::facing`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Orientation(pub shakmaty::Color);

impl Orientation {
    /// The rank drawn along the bottom edge, and the file along the left.
    fn flips(self) -> bool {
        self.0 == shakmaty::Color::Black
    }
}

/// The square under a point given as fractions of the board's width and height,
/// measured from its **top-left corner** — which is what the DOM gives us and
/// the opposite of how a rank is numbered.
///
/// `None` when the point is outside the board, which a pointer legitimately is
/// mid-drag: a drag that leaves the board and comes back must not be silently
/// clamped to an edge square the user never touched.
pub fn of_fraction(x: f64, y: f64, orientation: Orientation) -> Option<shakmaty::Square> {
    if !(0.0..1.0).contains(&x) || !(0.0..1.0).contains(&y) {
        return None;
    }

    let side = constants::BOARD_SIDE as f64;
    // `floor` then cast: both are in 0..8 by the bounds check above.
    let across = (x * side).floor() as u32;
    let down = (y * side).floor() as u32;

    let (file, rank) = if orientation.flips() {
        (constants::BOARD_SIDE as u32 - 1 - across, down)
    } else {
        (across, constants::BOARD_SIDE as u32 - 1 - down)
    };

    Some(shakmaty::Square::new(
        rank * constants::BOARD_SIDE as u32 + file,
    ))
}

/// A square's position as a (column, row) index from the board's top-left, which
/// is what a CSS grid and a percentage offset both want.
pub fn cell(sq: shakmaty::Square, orientation: Orientation) -> (u32, u32) {
    let (file, rank) = (u32::from(sq.file()), u32::from(sq.rank()));
    let last = constants::BOARD_SIDE as u32 - 1;
    if orientation.flips() {
        (last - file, rank)
    } else {
        (file, last - rank)
    }
}

/// The centre of a square in viewBox units, for drawing arrows over the board.
pub fn centre(sq: shakmaty::Square, orientation: Orientation) -> (f64, f64) {
    let (col, row) = cell(sq, orientation);
    let side = f64::from(constants::SQUARE_SIDE);
    let half = side / 2.0;
    (f64::from(col) * side + half, f64::from(row) * side + half)
}

/// Shift a segment perpendicular to its own direction by `offset` viewBox units, so
/// a move drawn more than once fans off its twin instead of hiding exactly under it.
///
/// Returns the endpoints unchanged when `offset` is zero (the common case — a move
/// drawn once sits on the true line) or when the segment has no length (`from ==
/// to`, guarded so a stray call cannot divide by zero). Here, and tested in
/// `tests/square.rs`, because a sign slip in the perpendicular would fan arrows the
/// wrong way — the same silent-geometry failure class this module exists to pin, and
/// unreachable from a native test while it lived inside a Leptos component.
pub fn fan(from: (f64, f64), to: (f64, f64), offset: f64) -> ((f64, f64), (f64, f64)) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy);
    if offset == 0.0 || length == 0.0 {
        return (from, to);
    }
    let (perp_x, perp_y) = (-dy / length, dx / length);
    let shift = |(x, y): (f64, f64)| (x + perp_x * offset, y + perp_y * offset);
    (shift(from), shift(to))
}

/// The squares of the board in the order they must be laid out, top-left first.
///
/// A rendering order, not a chess one — which is why it lives here next to
/// [`cell`] rather than being open-coded in the board's `view!`.
pub fn in_layout_order(orientation: Orientation) -> Vec<shakmaty::Square> {
    let mut squares: Vec<shakmaty::Square> = shakmaty::Square::ALL.into();
    squares.sort_by_key(|sq| {
        let (col, row) = cell(*sq, orientation);
        (row, col)
    });
    squares
}
