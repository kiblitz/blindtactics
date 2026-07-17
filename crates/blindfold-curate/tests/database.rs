//! The committed database re-proved from scratch.
//!
//! This is the safety net the whole project leans on. `database/*.jsonl` is
//! generated data committed to the repo, which means nothing about it is checked by
//! the compiler and nothing stops a bad regeneration, a hand-edit, or a bad merge
//! from shipping a puzzle that cannot be solved. A blindfold trainer fails silently
//! when that happens: the user cannot see the board, so a puzzle with no mate looks
//! exactly like a puzzle they are not good enough to solve.
//!
//! So every committed puzzle is re-proved here with the same solver the app runs —
//! not spot-checked, and not trusted because the curation tool said so.
//!
//! This lives in `blindfold-curate` rather than `blindfold-core` for two reasons:
//! core is deliberately I/O-free, and this needs the `magics` feature that only the
//! native tools enable.

use blindfold_core::puzzle;
use blindfold_curate::constants;

/// The database is at the workspace root; this crate is two levels down. Resolved
/// from `CARGO_MANIFEST_DIR` rather than the process CWD, which differs between
/// `cargo test` at the root and inside the crate.
///
/// Built from the same constants the tool writes with, so a rename cannot leave this
/// test silently reading a directory of files nobody generates any more.
fn database_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(constants::DATABASE_DIR)
}

fn load(depth: usize) -> Vec<puzzle::Puzzle> {
    let path = database_dir().join(format!("{}_{depth}.jsonl", constants::FILE_STEM));
    let contents =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    puzzle::of_jsonl(&contents).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// The one that matters. `verify` re-proves each puzzle end to end: the FEN is a
/// legal position, the stored line is linear (mates against *every* defense, not
/// just the one Lichess recorded), it mates at exactly the claimed depth, and no
/// shorter mate exists.
#[test]
fn every_committed_puzzle_is_provably_solvable() {
    for depth in constants::DEPTHS {
        for p in load(depth) {
            if let Err(e) = p.verify() {
                panic!("mate_in_{depth}.jsonl: puzzle {} is invalid: {e:?}", p.id);
            }
        }
    }
}

/// A puzzle's depth decides which file it lives in and what the UI tells the user to
/// look for. `verify` proves depth is *self*-consistent, but not that the file agrees
/// — a mate-in-3 in `mate_in_4.jsonl` verifies happily and still lies to the user.
#[test]
fn each_file_holds_only_the_depth_it_names() {
    for depth in constants::DEPTHS {
        for p in load(depth) {
            assert_eq!(
                p.depth, depth,
                "puzzle {} is in mate_in_{depth}.jsonl",
                p.id
            );
            assert_eq!(
                p.solution.len(),
                depth,
                "puzzle {}: {} arrows for a mate in {depth}",
                p.id,
                p.solution.len()
            );
        }
    }
}

#[test]
fn every_depth_has_a_full_set() {
    for depth in constants::DEPTHS {
        assert_eq!(
            load(depth).len(),
            constants::PER_DEPTH,
            "mate_in_{depth}.jsonl"
        );
    }
}

/// Ids are how the app will address a puzzle — a bookmark, a "next", a URL. Two
/// puzzles sharing one is a bug that would surface as a mysterious wrong board long
/// after curation.
#[test]
fn ids_are_unique_across_the_whole_database() {
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    for depth in constants::DEPTHS {
        for p in load(depth) {
            if let Some(prev) = seen.insert(p.id.clone(), depth) {
                panic!("id {} appears in mate_in_{prev} and mate_in_{depth}", p.id);
            }
        }
    }
}

/// The database is the app's only content, and `select::by_rating_spread` exists to
/// keep a tier from being uniformly trivial or uniformly brutal. If a regeneration
/// ever collapses that spread, the app gets worse in a way no correctness test would
/// notice — every puzzle would still be perfectly valid.
#[test]
fn each_depth_spans_a_range_of_ratings() {
    for depth in constants::DEPTHS {
        let ratings: Vec<u32> = load(depth).iter().map(|p| p.rating).collect();
        let (lo, hi) = (
            *ratings.iter().min().expect("non-empty"),
            *ratings.iter().max().expect("non-empty"),
        );
        assert!(
            hi - lo > 500,
            "mate_in_{depth}: ratings span only {lo}-{hi}"
        );
    }
}
