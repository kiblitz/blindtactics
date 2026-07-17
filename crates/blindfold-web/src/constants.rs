//! Named constants for the app.
//!
//! Separate from `blindfold_core::constants` (facts about chess) and
//! `blindfold_curate::constants` (curation policy). These are facts about the
//! *interface*. Nothing here can change whether an answer is right.

/// Files and ranks per side. Not a magic 8: `board.rs` builds the grid, maps a
/// pointer to a square, and lays arrows out on a viewBox, and all three have to
/// agree.
pub const BOARD_SIDE: usize = 8;

/// Side of the arrow overlay's viewBox, in its own user units.
///
/// The overlay scales with the board, so this is only a coordinate system — but
/// it has to be one where a square is a whole number, hence a multiple of
/// [`BOARD_SIDE`]. 800 gives a 100-unit square, so the stroke widths and radii
/// below read as percentages of a square.
pub const VIEWBOX_SIDE: u32 = 800;

/// Side of one square in [`VIEWBOX_SIDE`] units.
pub const SQUARE_SIDE: u32 = VIEWBOX_SIDE / BOARD_SIDE as u32;

/// Arrow shaft thickness, in viewBox units.
pub const ARROW_WIDTH: u32 = 9;

/// Radius of the numbered disc at an arrow's tail, in viewBox units.
pub const ARROW_BADGE_RADIUS: u32 = 16;

/// How far short of the target square's centre an arrow stops, in viewBox units.
///
/// Without it the head lands dead centre and two arrows meeting at one square
/// overlap into a blob. Pulling back leaves the square's centre legible.
pub const ARROW_HEAD_INSET: f64 = 22.0;

/// Milliseconds between plies when a solved line is replayed.
///
/// The reveal is the entire payoff of solving blind, so it is paced to be read,
/// not to be efficient.
pub const PLAYBACK_MS: u64 = 600;

/// Milliseconds before the first ply of the replay.
///
/// The board has just gone from void to pieces; moving something immediately
/// would step on the moment the user solved the puzzle for.
///
/// **Coupled to `styles.css` and nothing enforces it.** The reveal is two
/// animations there — the void-to-board fade (`.board--revealed .square`, 0.5s)
/// and the pieces arriving (`piece-appears`, 0.4s) — and this is their sum, so
/// the first move waits for the reveal to finish rather than cutting across it.
/// Retune either and the pacing desyncs with nothing failing. Emitting the
/// durations as custom properties from here would close it; until then, this
/// paragraph is the link.
pub const REVEAL_MS: u64 = 900;

/// The arrowhead marker's `id`, and the `url(#...)` that references it.
///
/// Two literals that must match or every arrow loses its head, so they are one
/// literal instead.
pub const ARROW_HEAD_ID: &str = "arrowhead";

/// Side of the arrowhead marker's own viewBox.
///
/// The head is a `<marker>`, so it has a coordinate system of its own rather than
/// the board's — which is why it is not in [`SQUARE_SIDE`] units. It lives here
/// beside [`ARROW_HEAD_INSET`] because the two were tuned against each other: the
/// inset is how far the shaft stops short to make room for exactly this head, and
/// splitting them across two files is how they drift.
pub const ARROW_HEAD_VIEWBOX: u32 = 10;

/// Inset of the arrowhead triangle from its viewBox edge, in marker units.
///
/// The head's tip is at `ARROW_HEAD_VIEWBOX - ARROW_HEAD_MARGIN` and its back edge
/// spans `ARROW_HEAD_MARGIN..=ARROW_HEAD_VIEWBOX - ARROW_HEAD_MARGIN`, so the shape
/// never touches the viewBox edge. `board.rs` builds the `<path>` from these
/// rather than spelling the coordinates, so retuning the viewBox moves the whole
/// triangle with it instead of leaving a stray literal behind.
pub const ARROW_HEAD_MARGIN: u32 = 1;

/// Where the line's endpoint is pinned along the head (`refX`/`refY`).
///
/// `refY` is the midline, so the head is centred on the shaft. `refX` sits *behind*
/// the tip — the tip is at `ARROW_HEAD_VIEWBOX - ARROW_HEAD_MARGIN` (9), the anchor
/// at 7 — so the point projects a little past where the shaft stops and caps it
/// cleanly. Tuned against [`ARROW_HEAD_INSET`], which pulls the shaft back to leave
/// room for exactly this head; the two live together because splitting them is how
/// they drift.
pub const ARROW_HEAD_ANCHOR_X: u32 = 7;
pub const ARROW_HEAD_ANCHOR_Y: u32 = 5;

/// The head's size as a multiple of [`ARROW_WIDTH`] — `marker*` units scale with
/// the stroke.
pub const ARROW_HEAD_SCALE: u32 = 4;

/// Font size of the number inside an arrow's badge, in viewBox units.
///
/// Sized against [`ARROW_BADGE_RADIUS`]: it has to sit inside the disc.
pub const ARROW_NUMBER_SIZE: u32 = 17;

/// Vertical nudge that centres the badge number on its disc, in `em`.
///
/// SVG anchors text at its baseline, not its middle, so a glyph centred by
/// coordinate alone sits high by about half its x-height. There is no
/// `dominant-baseline` value that is reliable across browsers for this, hence a
/// measured constant.
pub const ARROW_NUMBER_BASELINE_EM: &str = "0.36em";
