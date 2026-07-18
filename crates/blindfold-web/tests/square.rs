//! Tests for the board's geometry.
//!
//! This is the app's only arithmetic, and it has two ways to be silently wrong:
//! ranks run bottom-up in chess and top-down in a browser, and the board mirrors
//! when Black is to play. Either sign flip yields a board that looks completely
//! plausible and puts every arrow on the wrong square — and a blindfold user
//! cannot see that it did. They would simply be told their correct answer was
//! wrong.
//!
//! So the corners are pinned by hand in both orientations rather than derived,
//! which is the point: a test that computes the expected square the same way the
//! code does would agree with any flip.

use blindfold_web::constants;
use blindfold_web::square;

const WHITE: square::Orientation = square::Orientation(shakmaty::Color::White);
const BLACK: square::Orientation = square::Orientation(shakmaty::Color::Black);

/// Fractions land in the middle of a square rather than on its edge, so a
/// rounding slip is a failure rather than a coin toss.
fn at(col: u32, row: u32) -> (f64, f64) {
    // The board's side, not a hand-pinned expectation — the literals this file
    // *does* pin by hand are the squares (`A8`, `H1`) and the centres.
    let cell = 1.0 / constants::BOARD_SIDE as f64;
    ((f64::from(col) + 0.5) * cell, (f64::from(row) + 0.5) * cell)
}

/// White's back rank is at the bottom, so a8 is the top-left corner.
#[test]
fn white_sees_a8_top_left_and_a1_bottom_left() {
    let (x, y) = at(0, 0);
    assert_eq!(square::of_fraction(x, y, WHITE), Some(shakmaty::Square::A8));
    let (x, y) = at(0, 7);
    assert_eq!(square::of_fraction(x, y, WHITE), Some(shakmaty::Square::A1));
    let (x, y) = at(7, 0);
    assert_eq!(square::of_fraction(x, y, WHITE), Some(shakmaty::Square::H8));
    let (x, y) = at(7, 7);
    assert_eq!(square::of_fraction(x, y, WHITE), Some(shakmaty::Square::H1));
}

/// Black's back rank is at the bottom, so the board is mirrored through *both*
/// axes — not flipped about one. h1 top-left, a8 bottom-right.
#[test]
fn black_sees_h1_top_left_and_h8_bottom_left() {
    let (x, y) = at(0, 0);
    assert_eq!(square::of_fraction(x, y, BLACK), Some(shakmaty::Square::H1));
    let (x, y) = at(0, 7);
    assert_eq!(square::of_fraction(x, y, BLACK), Some(shakmaty::Square::H8));
    let (x, y) = at(7, 0);
    assert_eq!(square::of_fraction(x, y, BLACK), Some(shakmaty::Square::A1));
    let (x, y) = at(7, 7);
    assert_eq!(square::of_fraction(x, y, BLACK), Some(shakmaty::Square::A8));
}

/// The mirror is a rotation, not a reflection: a rank flip alone would leave the
/// files running a-h left to right for Black too, which is the classic wrong
/// board. e4 and d5 are diagonal neighbours and would survive a single flip, so
/// they are what pins it.
#[test]
fn flipping_for_black_mirrors_both_axes() {
    assert_eq!(square::cell(shakmaty::Square::E4, WHITE), (4, 4));
    assert_eq!(
        square::cell(shakmaty::Square::E4, BLACK),
        (3, 3),
        "e4 is not merely reflected vertically"
    );
    // Every square's cell under Black is the point-reflection of its cell under
    // White, for all 64 — which is the property, rather than two examples of it.
    for sq in shakmaty::Square::ALL {
        let (wc, wr) = square::cell(sq, WHITE);
        let (bc, br) = square::cell(sq, BLACK);
        assert_eq!((bc, br), (7 - wc, 7 - wr), "{sq}");
    }
}

/// `of_fraction` and `cell` are the two directions of one mapping — a pointer
/// coming in, a square going out — and nothing but this holds them together. If
/// they disagree, an arrow lands where the user did not drag it.
#[test]
fn a_squares_cell_is_where_a_pointer_finds_it() {
    for orientation in [WHITE, BLACK] {
        for sq in shakmaty::Square::ALL {
            let (col, row) = square::cell(sq, orientation);
            let (x, y) = at(col, row);
            assert_eq!(
                square::of_fraction(x, y, orientation),
                Some(sq),
                "{sq} at cell ({col}, {row})"
            );
        }
    }
}

/// Off the board is `None`, never a clamp to the nearest edge. A drag legitimately
/// leaves the board and comes back, and an arrow to a square the user never
/// touched is worse than no arrow.
#[test]
fn a_point_outside_the_board_is_not_a_square() {
    for (x, y) in [
        (-0.01, 0.5),
        (0.5, -0.01),
        (1.0, 0.5),
        (0.5, 1.0),
        (1.5, 1.5),
        (f64::NAN, 0.5),
    ] {
        assert_eq!(square::of_fraction(x, y, WHITE), None, "({x}, {y})");
    }
}

/// The edges belong to the square they open, so the whole board is covered with
/// no square eating its neighbour's first pixel.
#[test]
fn a_squares_leading_edge_belongs_to_it() {
    assert_eq!(
        square::of_fraction(0.0, 0.0, WHITE),
        Some(shakmaty::Square::A8)
    );
    // Just inside the far edge is still on the board...
    assert_eq!(
        square::of_fraction(1.0 - f64::EPSILON, 1.0 - f64::EPSILON, WHITE),
        Some(shakmaty::Square::H1)
    );
    // ...and the boundary between two squares goes to the second.
    assert_eq!(
        square::of_fraction(0.125, 0.0, WHITE),
        Some(shakmaty::Square::B8)
    );
}

/// Arrows are drawn in viewBox units while squares are laid out by CSS grid, so
/// the two coordinate systems have to name the same place.
#[test]
fn centres_sit_in_the_middle_of_their_cell() {
    // a8 is the top-left cell, so its centre is half a square in from the corner.
    assert_eq!(square::centre(shakmaty::Square::A8, WHITE), (50.0, 50.0));
    assert_eq!(square::centre(shakmaty::Square::H1, WHITE), (750.0, 750.0));
    // Mirrored for Black.
    assert_eq!(square::centre(shakmaty::Square::A8, BLACK), (750.0, 750.0));

    for sq in shakmaty::Square::ALL {
        let (x, y) = square::centre(sq, WHITE);
        let side = f64::from(constants::VIEWBOX_SIDE);
        assert_eq!(
            square::of_fraction(x / side, y / side, WHITE),
            Some(sq),
            "the centre of {sq} must be inside {sq}"
        );
    }
}

/// The layout order is what the grid is built from, so a duplicate or a gap would
/// be a board with a square drawn twice and another missing.
#[test]
fn layout_order_is_every_square_once_reading_left_to_right() {
    for orientation in [WHITE, BLACK] {
        let order = square::in_layout_order(orientation);
        assert_eq!(order.len(), 64);

        let unique: std::collections::HashSet<_> = order.iter().collect();
        assert_eq!(unique.len(), 64, "every square exactly once");

        // Index in the list must be the cell the geometry claims, or the grid and
        // the arrows are laid out against different boards.
        for (i, sq) in order.iter().enumerate() {
            let (col, row) = square::cell(*sq, orientation);
            assert_eq!(
                (row * constants::BOARD_SIDE as u32 + col) as usize,
                i,
                "{sq} is at index {i} but cell ({col}, {row})"
            );
        }
    }
}

/// The first square laid out is the top-left one, which is where the rank label
/// goes — and it differs by orientation.
#[test]
fn layout_starts_at_the_top_left_corner() {
    assert_eq!(square::in_layout_order(WHITE)[0], shakmaty::Square::A8);
    assert_eq!(square::in_layout_order(BLACK)[0], shakmaty::Square::H1);
}

/// `fan` shifts a segment perpendicular to itself by the whole offset, keeping its
/// length and direction — the property duplicate arrows rely on. Pinned here rather
/// than left inside the arrow component because a sign slip in the perpendicular is
/// exactly the silent-geometry bug this file guards against.
#[test]
fn fan_shifts_perpendicular_and_preserves_the_segment() {
    // A horizontal rightward segment: the perpendicular is vertical, so both ends
    // move by the full offset in y and not at all in x.
    let (from, to) = ((100.0, 100.0), (300.0, 100.0));
    let (a, b) = square::fan(from, to, 25.0);
    assert_eq!(a, (100.0, 125.0));
    assert_eq!(b, (300.0, 125.0));

    // Length is preserved (a shift, not a scale), for an arbitrary diagonal.
    let (from, to) = ((40.0, 60.0), (200.0, 340.0));
    let (a, b) = square::fan(from, to, 25.0);
    let len = |(px, py): (f64, f64), (qx, qy): (f64, f64)| (qx - px).hypot(qy - py);
    assert!(
        (len(a, b) - len(from, to)).abs() < 1e-9,
        "fanning must not stretch the shaft"
    );
    // Each endpoint moved exactly the offset from where it started.
    assert!((len(from, a) - 25.0).abs() < 1e-9);
    assert!((len(to, b) - 25.0).abs() < 1e-9);
}

/// A zero offset (a move drawn once) and a zero-length segment (`from == to`) both
/// return the endpoints untouched — the latter guarding the perpendicular's divide
/// against a zero magnitude.
#[test]
fn fan_leaves_a_zero_offset_or_zero_length_segment_alone() {
    let (from, to) = ((100.0, 100.0), (300.0, 220.0));
    assert_eq!(square::fan(from, to, 0.0), (from, to));
    assert_eq!(square::fan(from, from, 25.0), (from, from));
}
