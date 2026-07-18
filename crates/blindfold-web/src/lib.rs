//! The blindfold trainer's browser app.
//!
//! Thin by design. Everything with a right answer lives in `blindfold-core`,
//! which is tested under plain native `cargo test` with no browser in the loop —
//! so what is here is markup, event plumbing, and the arithmetic of where a
//! square is on screen.
//!
//! A library as well as a binary so `tests/` can reach [`square`], [`session`],
//! [`settings`] and [`database`]. An integration test cannot import a *binary* crate's
//! modules, and the same omission in `blindfold-curate` once left its riskiest
//! module — the streaming filter — untestable while a far simpler one had seven
//! tests. The split must follow the risk: `main.rs` mounts the app and does
//! nothing else.

pub mod app;
pub mod board;
pub mod constants;
pub mod database;
pub mod line;
pub mod panel;
pub mod pieces;
pub mod rating;
pub mod session;
pub mod settings;
pub mod square;
pub mod storage;
