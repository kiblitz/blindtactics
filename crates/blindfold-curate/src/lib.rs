//! Offline curation: the Lichess puzzle dump in, a verified blindfold puzzle set out.
//!
//! See `main.rs` for what the tool is for. This library target exists so the modules
//! below are reachable from `tests/` — an integration test cannot import a *binary*
//! crate's modules, and without it `constants::PER_DEPTH` and the database test's
//! idea of how many puzzles a file holds would be two numbers with nothing keeping
//! them in step.

pub mod constants;
pub mod dump;
pub mod select;
