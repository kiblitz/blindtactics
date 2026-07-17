//! Piece artwork.
//!
//! The user asked for "an actual knight picture", not a letter. These are
//! Cburnett's pieces — the set Lichess ships — which are **GPLv2-or-later**; the
//! "or later" is what lets a GPL-3.0-or-later project take them, so this is a
//! compatible use rather than a favour we are taking. See
//! `assets/pieces/README.md` for attribution and the licence's source.
//!
//! Compiled in rather than fetched, for the same reason as [`crate::database`]:
//! twelve requests to save 7 KiB is a bad trade, and a piece that fails to load
//! is a bug the reveal cannot recover from.
//!
//! This is the *only* place artwork lives. The roster panel and the board both
//! draw from it, so a knight is the same knight whether it is being announced or
//! revealed.

/// The `<svg>` element for a piece, ready to be dropped into the DOM.
///
/// Total for all twelve is ~7.4 KiB, so the table is exhaustive rather than lazy.
pub fn svg(color: shakmaty::Color, role: shakmaty::Role) -> &'static str {
    match (color, role) {
        (shakmaty::Color::White, shakmaty::Role::King) => include_str!("../assets/pieces/wK.svg"),
        (shakmaty::Color::White, shakmaty::Role::Queen) => include_str!("../assets/pieces/wQ.svg"),
        (shakmaty::Color::White, shakmaty::Role::Rook) => include_str!("../assets/pieces/wR.svg"),
        (shakmaty::Color::White, shakmaty::Role::Bishop) => include_str!("../assets/pieces/wB.svg"),
        (shakmaty::Color::White, shakmaty::Role::Knight) => include_str!("../assets/pieces/wN.svg"),
        (shakmaty::Color::White, shakmaty::Role::Pawn) => include_str!("../assets/pieces/wP.svg"),
        (shakmaty::Color::Black, shakmaty::Role::King) => include_str!("../assets/pieces/bK.svg"),
        (shakmaty::Color::Black, shakmaty::Role::Queen) => include_str!("../assets/pieces/bQ.svg"),
        (shakmaty::Color::Black, shakmaty::Role::Rook) => include_str!("../assets/pieces/bR.svg"),
        (shakmaty::Color::Black, shakmaty::Role::Bishop) => include_str!("../assets/pieces/bB.svg"),
        (shakmaty::Color::Black, shakmaty::Role::Knight) => include_str!("../assets/pieces/bN.svg"),
        (shakmaty::Color::Black, shakmaty::Role::Pawn) => include_str!("../assets/pieces/bP.svg"),
    }
}
