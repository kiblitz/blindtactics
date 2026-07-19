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
/// viewBox units. The head is then drawn forward from there, its tip landing a
/// little short of the centre.
///
/// Two jobs at once. It keeps the tip off the square's centre so two arrows
/// meeting on one square do not overlap into a blob, and — since the shaft ends at
/// the head's wide base — it is also what leaves room for the whole head. Tuned
/// against [`ARROW_HEAD_LENGTH`]: the shaft stops `INSET` short and the head then
/// projects `LENGTH` of that gap back forward, so `INSET - LENGTH` of clear space is
/// left beyond the tip. The two live together because splitting them is how they drift.
pub const ARROW_HEAD_INSET: f64 = 42.0;

/// The arrowhead triangle, in viewBox units: how far it projects forward from the
/// shaft's (inset) end to its tip, and its half-width at the back edge.
///
/// The head is a plain filled `<polygon>` drawn in the arrow's own `<g>` and filled
/// like the shaft — *not* a shared `<marker>`. A marker lives in `<defs>`, outside
/// the arrow's element, so it cannot inherit the arrow's per-move colour: both
/// `currentColor` (which the marker re-resolves in its own context) and
/// `context-stroke` were tried, and both painted every head the board's base amber
/// in the browser. Drawing the head inline sidesteps marker paint entirely — the
/// same reason the number badge is an inline `<circle>`. Tuned against
/// [`ARROW_HEAD_INSET`], which reserves the shaft space.
pub const ARROW_HEAD_LENGTH: f64 = 32.0;
pub const ARROW_HEAD_HALF_WIDTH: f64 = 14.0;

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

/// The `localStorage` key the point-of-view preference persists under.
pub const POV_STORAGE_KEY: &str = "blindfold.pov";

/// The `localStorage` key the input mode (draw / speak) persists under.
pub const INPUT_STORAGE_KEY: &str = "blindfold.input";

/// The `localStorage` key the output mode (show / read-aloud) persists under.
pub const OUTPUT_STORAGE_KEY: &str = "blindfold.output";

/// The `localStorage` key the spoken-input silence timeout persists under.
pub const SILENCE_STORAGE_KEY: &str = "blindfold.silence";

/// How long a silence, in seconds, ends a spoken line once the mic is armed — the
/// default, and the bounds the settings stepper (and the loader's recovery) clamp to.
///
/// The mic runs a countdown while listening; each heard phrase resets it, and when it
/// reaches zero the record control turns itself off. Five seconds is long enough to
/// think between moves without stranding the mic on. The range is a usability floor
/// and ceiling, not a hard limit — below the floor a thinking pause would cut the mic,
/// above the ceiling it barely turns off at all.
pub const SILENCE_DEFAULT_SECS: u32 = 5;
pub const SILENCE_MIN_SECS: u32 = 2;
pub const SILENCE_MAX_SECS: u32 = 15;

/// How much one press of the silence-timeout stepper moves it, in seconds.
pub const SILENCE_STEP_SECS: u32 = 1;

/// The tick of the silence countdown: one second, so the displayed number counts down
/// in whole seconds. Named rather than an inline `Duration::from_secs(1)` at the call
/// site, per the constants rule.
pub const SILENCE_TICK_MS: u64 = 1000;

/// Speaking rate for the read-aloud voice, as a multiple of the platform default
/// (`1.0`). A touch under one so the roster is read at an unhurried, deliberate pace
/// rather than the rushed default — the "calm" half of the voice.
pub const SPEECH_RATE: f32 = 0.9;

/// Speaking pitch for the read-aloud voice, as a multiple of the platform default
/// (`1.0`, range `0.0`–`2.0`). Slightly lowered for a calmer, less bright tone.
pub const SPEECH_PITCH: f32 = 0.9;

/// How many of the closest-rated puzzles the next one is drawn from.
///
/// Selection is "random, near your rating": the candidates are the
/// `SELECTION_POOL` puzzles whose rating is nearest the user's, and one of those
/// is chosen uniformly. Small enough that difficulty tracks the user, large
/// enough not to serve the same handful over and over.
pub const SELECTION_POOL: usize = 24;
