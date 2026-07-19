//! The user's preferences, and where they are kept.
//!
//! Like [`crate::rating`], the logic is browser-free and native-tested; only the
//! `load_*`/`save_*` pairs touch `localStorage` (via [`crate::storage`]). The
//! preferences: which side of the board faces the user ([`Pov`]), how moves are
//! entered ([`Input`]) and how the puzzle is delivered ([`Output`]) — the two halves
//! of voice mode — and how long a silence ends spoken input ([`load_silence`]). Each
//! persists under its own key so they move independently, and the module is the seam
//! more settings grow against — another is another pair here, not a rewrite.

use crate::constants;
use crate::storage;

/// Whose side of the board is drawn along the bottom edge.
///
/// The persisted half of orientation. The board is blank until solved, so this does
/// not change what the user must find — but it decides which square a drag lands on
/// and how the revealed mate reads, so a user who thinks in White's coordinates and
/// a user mating as Black both get a board that matches their head.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pov {
    /// The side to move — the solver's. The board follows the puzzle so the user is
    /// always sitting behind the pieces they are mating with. The default.
    #[default]
    ToMove,
    /// White along the bottom, whichever side the solver is.
    White,
    /// Black along the bottom.
    Black,
}

impl Pov {
    /// Every value, in the order the settings menu lists them.
    pub const ALL: [Pov; 3] = [Pov::ToMove, Pov::White, Pov::Black];

    /// The side drawn along the bottom for this preference, given who is to move.
    pub fn side(self, solver: shakmaty::Color) -> shakmaty::Color {
        match self {
            Pov::ToMove => solver,
            Pov::White => shakmaty::Color::White,
            Pov::Black => shakmaty::Color::Black,
        }
    }

    /// The label the settings menu shows for this choice.
    pub fn label(self) -> &'static str {
        match self {
            Pov::ToMove => "To move",
            Pov::White => "White",
            Pov::Black => "Black",
        }
    }

    /// The stored token — spelled out rather than derived from the variant name, so
    /// renaming a variant cannot silently orphan a saved preference. Paired with
    /// [`from_token`](Pov::from_token).
    fn token(self) -> &'static str {
        match self {
            Pov::ToMove => "to-move",
            Pov::White => "white",
            Pov::Black => "black",
        }
    }

    fn from_token(token: &str) -> Option<Pov> {
        Pov::ALL.into_iter().find(|p| p.token() == token)
    }
}

/// How the user enters their moves — the persisted half of voice mode's *input* side.
///
/// It does not change what a move means: a drawn arrow and a spoken one resolve to
/// the same [`arrow::Arrow`](blindfold_core::arrow::Arrow). It decides only whether
/// the microphone arms itself, and — crucially — how the record control behaves from
/// one puzzle to the next (see [`arms_next`](Input::arms_next)).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Input {
    /// Draw on the board. The mic stays off, and the record control resets to off on
    /// every new puzzle, so the user arms it by hand each time they want to speak. The
    /// default — speaking is opt-in, and a mic that armed itself unbidden would be a
    /// surprise (and a permission prompt).
    #[default]
    Physical,
    /// Speak the moves. The mic's last state carries across puzzles, so once the user
    /// arms it, it re-arms itself on each new puzzle until they turn it off — the
    /// hands-free loop the project exists for.
    Audio,
}

impl Input {
    /// Every value, in the order the settings menu lists them.
    pub const ALL: [Input; 2] = [Input::Physical, Input::Audio];

    /// Whether the mic should be listening at the start of a *new* puzzle, given the
    /// user's last-set intent (`last_on`).
    ///
    /// This is the whole behavioural difference between the two modes: [`Physical`](
    /// Input::Physical) always resets the record control to off, so the user re-arms it
    /// each puzzle; [`Audio`](Input::Audio) carries the last state, so an armed mic
    /// re-arms itself. Pure so a native test pins it.
    pub fn arms_next(self, last_on: bool) -> bool {
        match self {
            Input::Physical => false,
            Input::Audio => last_on,
        }
    }

    /// The label the settings menu shows for this choice.
    pub fn label(self) -> &'static str {
        match self {
            Input::Physical => "Draw",
            Input::Audio => "Speak",
        }
    }

    fn token(self) -> &'static str {
        match self {
            Input::Physical => "physical",
            Input::Audio => "audio",
        }
    }

    fn from_token(token: &str) -> Option<Input> {
        Input::ALL.into_iter().find(|i| i.token() == token)
    }
}

/// How the puzzle and the verdict are delivered — the persisted half of voice mode's
/// *output* side.
///
/// [`speaks`](Output::speaks) is the one thing the rest of the app asks of it: whether
/// a new puzzle's roster and each verdict are read aloud *automatically*. It is
/// deliberately not the only path to speech — the roster panel always carries a speak
/// button (see [`crate::panel`]) — so this setting is "talk on its own", not "talk at
/// all".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Output {
    /// Read on screen only. Nothing is spoken unless the user asks (the roster's speak
    /// button). The default: audio needs a user gesture to begin, and a page that
    /// talks on load is a surprise.
    #[default]
    Visual,
    /// Read aloud. A new puzzle's roster and each verdict are spoken automatically.
    Audio,
}

impl Output {
    /// Every value, in the order the settings menu lists them.
    pub const ALL: [Output; 2] = [Output::Visual, Output::Audio];

    /// Whether announcements are spoken automatically — the new puzzle's roster and
    /// each verdict. The manual speak button does not consult this.
    pub fn speaks(self) -> bool {
        matches!(self, Output::Audio)
    }

    /// The label the settings menu shows for this choice.
    pub fn label(self) -> &'static str {
        match self {
            Output::Visual => "Show",
            Output::Audio => "Read aloud",
        }
    }

    fn token(self) -> &'static str {
        match self {
            Output::Visual => "visual",
            Output::Audio => "audio",
        }
    }

    fn from_token(token: &str) -> Option<Output> {
        Output::ALL.into_iter().find(|o| o.token() == token)
    }
}

/// The side drawn along the bottom edge — the one facing the user — after applying
/// `pov` and a per-puzzle `flipped`.
///
/// Named `facing` rather than `orientation` so the call site reads as "which side →
/// geometry" (`square::Orientation(settings::facing(..))`) instead of repeating the
/// word with two meanings. The flip is transient — a quick inversion of the current
/// view that resets when the next puzzle loads — so it is not part of the persisted
/// [`Pov`]; it is layered on here. Kept a pure function of its three inputs so a
/// native test pins the sign of the flip, the same care [`crate::square`] takes.
pub fn facing(pov: Pov, solver: shakmaty::Color, flipped: bool) -> shakmaty::Color {
    let base = pov.side(solver);
    if flipped {
        base.other()
    } else {
        base
    }
}

/// The stored POV, or [`Pov::ToMove`] for a browser that has none or a value we no
/// longer recognise. A corrupt token is not something the user can act on, so the
/// default is the right recovery rather than an error.
pub fn load_pov() -> Pov {
    storage::read(constants::POV_STORAGE_KEY)
        .and_then(|raw| Pov::from_token(&raw))
        .unwrap_or_default()
}

/// Persist the POV. Silent if `localStorage` is unavailable — the preference then
/// simply does not survive a reload, a graceful degradation rather than a failure.
pub fn save_pov(pov: Pov) {
    storage::write(constants::POV_STORAGE_KEY, pov.token());
}

/// Whether the puzzle and verdict are read aloud. Off unless the browser has
/// explicitly stored `on` — audio starts silent and is opt-in, both because it needs
/// a user gesture to begin and because a page that talks on load is a surprise.
pub fn load_sound() -> bool {
    storage::read(constants::SOUND_STORAGE_KEY).as_deref() == Some(SOUND_ON)
}

/// Persist the read-aloud preference. Silent if `localStorage` is unavailable, like
/// [`save_pov`].
pub fn save_sound(on: bool) {
    storage::write(
        constants::SOUND_STORAGE_KEY,
        if on { SOUND_ON } else { SOUND_OFF },
    );
}

/// The stored tokens for the read-aloud flag — spelled out rather than
/// `bool::to_string`, for the same reason [`Pov::token`] is: the on-disk value is a
/// contract, not an incidental of how a `bool` prints.
const SOUND_ON: &str = "on";
const SOUND_OFF: &str = "off";

/// The stored input mode, or [`Input::Physical`] for a browser that has none or an
/// unrecognised value — the safe default, since it never arms the mic unbidden.
pub fn load_input() -> Input {
    storage::read(constants::INPUT_STORAGE_KEY)
        .and_then(|raw| Input::from_token(&raw))
        .unwrap_or_default()
}

/// Persist the input mode. Silent if `localStorage` is unavailable, like [`save_pov`].
pub fn save_input(input: Input) {
    storage::write(constants::INPUT_STORAGE_KEY, input.token());
}

/// The stored output mode, or [`Output::Visual`] for a browser that has none or an
/// unrecognised value — the safe default, since it never speaks unbidden.
pub fn load_output() -> Output {
    storage::read(constants::OUTPUT_STORAGE_KEY)
        .and_then(|raw| Output::from_token(&raw))
        .unwrap_or_default()
}

/// Persist the output mode. Silent if `localStorage` is unavailable, like [`save_pov`].
pub fn save_output(output: Output) {
    storage::write(constants::OUTPUT_STORAGE_KEY, output.token());
}

/// How many seconds of silence end a spoken line, clamped to a sane range.
///
/// Stored as the decimal number of seconds. A missing, malformed, or out-of-range
/// value falls back to [`constants::SILENCE_DEFAULT_SECS`] rather than erroring — a
/// corrupt preference is not something the user can act on.
pub fn load_silence() -> u32 {
    storage::read(constants::SILENCE_STORAGE_KEY)
        .and_then(|raw| raw.parse::<u32>().ok())
        .map(clamp_silence)
        .unwrap_or(constants::SILENCE_DEFAULT_SECS)
}

/// Persist the silence timeout, clamped so a stored value can never fall outside the
/// range the stepper allows. Silent if `localStorage` is unavailable, like [`save_pov`].
pub fn save_silence(secs: u32) {
    storage::write(
        constants::SILENCE_STORAGE_KEY,
        &clamp_silence(secs).to_string(),
    );
}

/// Clamp a silence timeout to `[MIN, MAX]`. Pure and public so the stepper's bounds
/// and the loader's recovery are the one rule, native-tested rather than trusted.
pub fn clamp_silence(secs: u32) -> u32 {
    secs.clamp(constants::SILENCE_MIN_SECS, constants::SILENCE_MAX_SECS)
}
