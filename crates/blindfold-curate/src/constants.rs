//! Named constants for the curation tool.
//!
//! Separate from `blindfold_core::constants`, which holds facts about *chess*. These
//! are policy: how many puzzles we want and where they go. Nothing here can change
//! whether a puzzle is correct, only which correct ones we keep.

/// Alias for core's theme prefix, so the cheap pre-filter and the real theme test
/// cannot drift apart. Aliased rather than re-typed: `"mateIn"` written twice is
/// two things to keep in step.
pub const MATE_THEME_HINT: &str = blindfold_core::constants::LICHESS_MATE_THEME_PREFIX;

/// Mate depths we curate, and the order the database files are written in.
pub const DEPTHS: [usize; 4] = [1, 2, 3, 4];

/// How many verified puzzles to keep per depth.
///
/// Deliberately small: the app is what we want to iterate on, and the database can
/// be regenerated at any size by re-running this. ~11k mate-in-4s survive filtering
/// against this target of 100, so the tight tier has ample room.
pub const PER_DEPTH: usize = 100;

/// How many candidates to gather per depth before verifying.
///
/// Verification is the expensive half, so this bounds the work rather than the
/// yield. Sized off the measured survival rate: mate-in-4 is the worst tier at
/// ~35%, so 400 candidates yields ~140 — comfortably past [`PER_DEPTH`] without
/// verifying thousands of rows we would then throw away.
pub const CANDIDATES_PER_DEPTH: usize = 400;

/// Where the curated database is written, relative to the workspace root.
pub const DATABASE_DIR: &str = "database";

/// Filename stem for a depth's puzzle file: `mate_in_2.jsonl`.
pub const FILE_STEM: &str = "mate_in";

/// Progress is printed every this many rows. The dump is ~6M lines and a silent
/// multi-minute scan is indistinguishable from a hang.
pub const PROGRESS_EVERY: usize = 500_000;
