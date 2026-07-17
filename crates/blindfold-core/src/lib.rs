//! Pure logic for the blindfold chess trainer.
//!
//! This crate holds everything worth testing and depends on no UI, no DOM, and no
//! I/O — so its whole test suite runs under plain `cargo test`, instantly, with no
//! browser or wasm toolchain in the loop.
//!
//! It is shared by the offline curation tool and the browser app, so that the
//! database and the live app cannot drift apart about what "solved" means: both
//! call [`mate::judge`].
//!
//! Start at [`arrow`] — the decision to make arrows, not moves, the unit of
//! identity explains most of the rest.

pub mod arrow;
pub mod constants;
pub mod lichess;
pub mod mate;
pub mod position;
pub mod puzzle;
pub mod roster;
