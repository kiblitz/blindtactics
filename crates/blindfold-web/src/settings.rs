//! The user's display preferences, and where they are kept.
//!
//! Like [`crate::rating`], the logic is browser-free and native-tested; only
//! [`load`]/[`save`] touch `localStorage` (via [`crate::storage`]). One preference
//! for now — which side of the board faces the user — but the module is the seam a
//! settings menu grows against, so a second one is a variant here, not a rewrite.

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
pub fn load() -> Pov {
    storage::read(constants::POV_STORAGE_KEY)
        .and_then(|raw| Pov::from_token(&raw))
        .unwrap_or_default()
}

/// Persist the POV. Silent if `localStorage` is unavailable — the preference then
/// simply does not survive a reload, a graceful degradation rather than a failure.
pub fn save(pov: Pov) {
    storage::write(constants::POV_STORAGE_KEY, pov.token());
}
