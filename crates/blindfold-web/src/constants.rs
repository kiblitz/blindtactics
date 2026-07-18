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

/// One square as a percentage of the board's side.
///
/// A cell index scales straight to a CSS percentage by this, so absolutely-placed
/// layers — the pieces and the promotion popup — sit on the grid without each
/// re-deriving `100 / BOARD_SIDE`.
pub const PERCENT_PER_SQUARE: f64 = 100.0 / BOARD_SIDE as f64;

/// Arrow shaft thickness, in viewBox units.
pub const ARROW_WIDTH: u32 = 9;

/// Radius of the numbered disc at an arrow's tail, in viewBox units.
pub const ARROW_BADGE_RADIUS: u32 = 16;

/// How far short of the target square's centre the arrow's *shaft* stops, in
/// viewBox units. The head is then drawn forward from there (see
/// [`ARROW_HEAD_ANCHOR_X`]), its tip landing a little short of the centre.
///
/// Two jobs at once. It keeps the tip off the square's centre so two arrows
/// meeting on one square do not overlap into a blob, and — since the shaft now
/// ends at the head's wide base — it is also what leaves room for the whole head.
/// Big enough to clear the head (`ANCHOR_X` at the base means the head projects its
/// full length forward) with a little gap beyond the tip.
pub const ARROW_HEAD_INSET: f64 = 42.0;

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
/// `refY` is the midline, so the head is centred on the shaft. `refX` is `0` — the
/// head's *base* — so the shaft ends where the triangle is at its widest and the
/// whole head projects forward from there. That is the fix for the stray "stub":
/// with the anchor near the tip (it was `7`), the shaft ended under the *narrow*
/// end of the head, and the stroke's cap poked out either side of it. Ending at the
/// base tucks the cap under the widest part, where nothing shows. Tuned against
/// [`ARROW_HEAD_INSET`], which pulls the shaft back to leave room for exactly this
/// head; the two live together because splitting them is how they drift.
pub const ARROW_HEAD_ANCHOR_X: u32 = 0;
pub const ARROW_HEAD_ANCHOR_Y: u32 = 5;

/// The head's size as a multiple of [`ARROW_WIDTH`] — `marker*` units scale with
/// the stroke.
pub const ARROW_HEAD_SCALE: u32 = 4;

/// Distinct colours for the numbered arrows, cycled by the move's position in the
/// line so each arrow reads as its own colour rather than every one sharing the
/// board's amber. Mid-saturation on purpose: the badge number is white with a dark
/// outline (see `styles.css`), which stays legible on all of these on both themes.
pub const ARROW_COLORS: [&str; 8] = [
    "#d99a3f", "#4f8fd6", "#4bab5e", "#b45bd1", "#d95b6b", "#2fa7a0", "#b07a34", "#7b6cd6",
];

/// The colour of the in-flight ghost arrow — the board's base amber, distinct from
/// the committed arrows' cycled [`ARROW_COLORS`]. Named so the one arrow whose
/// colour is *not* drawn from the palette is still sourced from `constants` rather
/// than inlined at the call site.
pub const GHOST_ARROW_COLOR: &str = "var(--amber)";

/// How far apart, in viewBox units, to fan two arrows that share the same from/to
/// so a move drawn twice does not hide under its twin. Derived as a quarter of
/// [`SQUARE_SIDE`] rather than hardcoded, so it stays a quarter-square if the
/// viewBox is ever retuned.
pub const ARROW_DUP_OFFSET: f64 = SQUARE_SIDE as f64 / 4.0;

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

/// The rating a new user starts at, before any puzzle has moved it.
///
/// Puzzle ratings in the database span roughly 600–2600, so 1200 is mid-pack and
/// settles quickly toward the user's real level.
pub const ELO_START: u32 = 1200;

/// Elo K-factor: the most one puzzle can move the rating.
pub const ELO_K: f64 = 32.0;

/// Logistic scale of the Elo expected-score curve. 400 is the classic value — a
/// 400-point edge is about 10:1 expected odds.
pub const ELO_SCALE: f64 = 400.0;

/// Base of the Elo expected-score logistic. The `10` half of "a 400-point edge is
/// about 10:1 odds" — it pairs with [`ELO_SCALE`] and is meaningless without it, so
/// the two live together rather than one being named and the other an inline
/// literal in the formula.
pub const ELO_LOG_BASE: f64 = 10.0;

/// Bounds on the stored rating, so a long streak either way cannot drive it
/// somewhere absurd.
pub const ELO_FLOOR: u32 = 100;
pub const ELO_CEILING: u32 = 3000;

/// The `localStorage` key the rating persists under.
pub const ELO_STORAGE_KEY: &str = "blindfold.elo";

/// How many of the closest-rated puzzles the next one is drawn from.
///
/// Selection is "random, near your rating": the candidates are the
/// `SELECTION_POOL` puzzles whose rating is nearest the user's, and one of those
/// is chosen uniformly. Small enough that difficulty tracks the user, large
/// enough not to serve the same handful over and over.
pub const SELECTION_POOL: usize = 24;
