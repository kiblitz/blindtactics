//! Named constants for the curation tool.
//!
//! Separate from `blindfold_core::constants`, which holds facts about *chess*. These
//! are policy: what we want a puzzle set to look like and where it goes. Nothing here
//! can change whether a puzzle is correct, only which correct ones we keep.

/// Alias for core's theme prefix, so the cheap pre-filter and the real theme test
/// cannot drift apart. Aliased rather than re-typed: `"mateIn"` written twice is
/// two things to keep in step.
pub const MATE_THEME_HINT: &str = blindfold_core::constants::LICHESS_MATE_THEME_PREFIX;

/// Mate depths we curate, and the order the database files are written in.
///
/// The length is tied to core's ceiling: `Puzzle::verify` rejects anything deeper
/// than `MAX_DEPTH`, so a fifth entry here would produce a tier where every puzzle
/// fails verification and the file comes out empty. Better a compile error.
pub const DEPTHS: [usize; blindfold_core::constants::MAX_DEPTH] = [1, 2, 3, 4];

/// How many verified puzzles to keep per depth.
///
/// Deliberately small: the app is what we want to iterate on, and the database can
/// be regenerated at any size by re-running this.
pub const PER_DEPTH: usize = 100;

/// How many candidates to gather per depth before verifying.
///
/// Sized for **choice, not survival**. The tempting sum is "400 candidates × ~35%
/// mate-in-4 survival ≈ 140, comfortably past `PER_DEPTH`" — and that is exactly the
/// wrong target. A pool of 140 to pick 100 from is a 71% keep rate: `select` stops
/// selecting and starts rounding down, and the roster gate below has nothing to
/// choose between.
///
/// So this is sized off the *scarcest* thing we filter on rather than the tier's
/// survival rate. Only ~10% of verified puzzles come in under
/// [`MAX_ROSTER_SQUARES`], so hitting `PER_DEPTH` needs roughly 10x the survivors,
/// which needs roughly 30x the candidates at mate-in-4's rate.
///
/// The economy this used to protect was imaginary: verification is ~13 ms for a
/// mate-in-4 and `run` already parallelizes it, so even 6,000 candidates across four
/// depths is a couple of minutes on one core and seconds on twelve — for a tool that
/// runs once, offline, against a 302 MB download.
pub const CANDIDATES_PER_DEPTH: usize = 6_000;

/// The most squares a puzzle's roster may name.
///
/// The gate that makes this a *blindfold* trainer rather than a memory test. The user
/// never sees the board, so every occupied square is something they must hold in their
/// head before they can begin to think about mate. Chess validity does not bound this
/// at all: the first cut of this database shipped a mate-in-**one** with all 32 pieces
/// on the board, rated 1029, whose roster ran to twelve lines.
///
/// 14 is an honest ceiling rather than an ideal. Sparse positions are scarce — a
/// tighter gate of ~10 is simply not reachable at [`PER_DEPTH`] for mate-in-4, which
/// is the tier with the smallest pool to begin with. Lower it if the pool ever grows;
/// `each_depth_fits_in_a_head` in `tests/database.rs` is what holds it.
pub const MAX_ROSTER_SQUARES: usize = 14;

/// Reject a candidate whose halfmove clock is this high or higher.
///
/// Measured on the position the user is **shown** — the one after the row's setup
/// move — not on the row's FEN, which is a ply earlier and whose clock is therefore
/// one lower for a quiet setup move. That is the `C` in CLAUDE.md's derivation: the
/// clock at ply 0 of what the solver actually faces.
///
/// shakmaty implements no 50-move rule, so our solver cannot see a draw the defender
/// could claim. From `C = 94`, an all-quiet mate-in-4 lets the defender reach 99 and
/// declare a move making it 100 — claimable under FIDE 9.3(a) — on their last turn
/// before the mate. A mate the defender can simply decline to lose is not a mate.
///
/// 94 is derived in CLAUDE.md and it is **not** `100 - 7`: the mating ply is the
/// solver's, and mate ends the game (FIDE 5.1.1), so the binding ply is the
/// defender's last turn, not the mate. Read the derivation before touching this.
///
/// It lives in curation rather than in `judge`, which must stay a pure function of
/// exactly the four things the roster carries.
pub const MAX_HALFMOVE_CLOCK: u32 = 94;

/// The narrowest rating range a depth's file may span before we call the spread
/// broken.
///
/// A tripwire on [`select::by_rating_spread`](crate::select::by_rating_spread), not a
/// target: real spreads run ~1400-1900, so this fires only on a collapse, not on
/// drift.
pub const MIN_RATING_SPAN: u32 = 500;

/// Where the curated database is written, relative to the workspace root.
pub const DATABASE_DIR: &str = "database";

/// The file a depth's puzzles live in: `mate_in_2.jsonl`.
///
/// A function rather than a `FILE_STEM` constant because the stem was never the whole
/// name — the `_` and the `.jsonl` were re-typed at every call site, so a reader and
/// a writer could still drift on the two parts the constant did not cover.
pub fn file_name(depth: usize) -> String {
    format!("mate_in_{depth}.jsonl")
}

/// Progress is printed every this many rows. The dump is ~6M lines and a silent
/// multi-minute scan is indistinguishable from a hang.
pub const PROGRESS_EVERY: usize = 500_000;
