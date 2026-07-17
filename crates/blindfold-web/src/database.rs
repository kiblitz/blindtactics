//! The curated puzzle set, compiled into the binary.
//!
//! `include_str!` rather than a `fetch`, for three reasons. The set is ~46 KiB, so
//! a request would cost a round trip to save nothing; the app is a static site
//! with no server to ask; and a fetch can fail at runtime, which would mean a
//! blank board with no puzzles and an error path to design for. Embedding makes
//! "the database is missing" a link error instead.
//!
//! # The one seam this module has
//!
//! `include_str!` needs string literals, so the paths below are typed out rather
//! than built from `blindfold_curate::constants::file_name`, which is what the
//! curation tool *writes* with. Those are two spellings of the same names and
//! nothing in the type system holds them together. `tests/database.rs` is what
//! does — it checks the embedded set against the same invariants the committed
//! files are held to, so a rename that misses this file fails the suite rather
//! than shipping a tier of puzzles nobody can reach.

use blindfold_core::puzzle;

/// The database, in depth order, exactly as committed.
const FILES: [&str; 4] = [
    include_str!("../../../database/mate_in_1.jsonl"),
    include_str!("../../../database/mate_in_2.jsonl"),
    include_str!("../../../database/mate_in_3.jsonl"),
    include_str!("../../../database/mate_in_4.jsonl"),
];

/// Every puzzle, in depth order.
///
/// Panics if the embedded JSONL does not parse. That is a deliberate choice over
/// returning a `Result`: the input is not user data, it is a file the build just
/// compiled in and that `tests/database.rs` re-proves, so a parse failure is a
/// broken build rather than a condition the UI could do anything about.
pub fn load() -> Vec<puzzle::Puzzle> {
    FILES
        .iter()
        .flat_map(|contents| {
            puzzle::of_jsonl(contents).expect("the embedded database is generated and tested")
        })
        .collect()
}
