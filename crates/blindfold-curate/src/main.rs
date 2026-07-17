//! Turn the Lichess puzzle dump into a verified blindfold puzzle set.
//!
//! ```text
//! blindfold-curate <lichess_db_puzzle.csv.zst> [output-dir]
//! ```
//!
//! # What this is for
//!
//! The dump has ~6M puzzles and ~1.9M carry a `mateInN` tag. Almost none of them
//! are usable here, and the tag cannot tell you which: Lichess stores exactly one
//! engine-chosen defense per puzzle, so a line that mates in *its* record may lose
//! to a defense nobody wrote down. Our arrow UI cannot branch, so a puzzle is only
//! usable if the same arrows mate against **every** defense.
//!
//! So this tool exists to throw things away. Measured on real data, ~35% of
//! `mateIn4` rows survive; the rest would have been shipped as broken puzzles that
//! tell a correct solver they were wrong.
//!
//! # Structure
//!
//! All the judgement lives in `blindfold-core` — `lichess::of_row` for the
//! conversion, `Puzzle::verify` for the proof. This crate is a streaming loop, a
//! thread pool, and a file writer. That is deliberate: the app and the database must
//! agree about what "solved" means, and the only way to guarantee that is for both
//! to call the same code.

use blindfold_core::lichess;
use blindfold_core::puzzle;
use blindfold_curate::constants;
use blindfold_curate::dump;
use blindfold_curate::select;
use rayon::iter::IntoParallelIterator as _;
use rayon::iter::ParallelIterator as _;
use std::io::BufRead as _;

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(dump) = args.next() else {
        eprintln!("usage: blindfold-curate <lichess_db_puzzle.csv.zst> [output-dir]");
        return std::process::ExitCode::FAILURE;
    };
    let out_dir = args
        .next()
        .unwrap_or_else(|| constants::DATABASE_DIR.to_owned());

    match run(&dump, &out_dir) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(dump: &str, out_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("reading {dump}");
    let candidates = gather(dump)?;

    std::fs::create_dir_all(out_dir)?;
    let mut total = 0usize;

    for depth in constants::DEPTHS {
        let found = candidates.get(&depth).cloned().unwrap_or_default();
        println!("\nmate in {depth}: verifying {} candidates", found.len());

        // The expensive half. Each `verify` re-proves the puzzle from scratch —
        // legal, linear, mates at exactly the claimed depth, and no shorter mate
        // exists — and they are independent, so this is embarrassingly parallel.
        let verified: Vec<puzzle::Puzzle> = found
            .into_par_iter()
            .filter(|p| p.verify().is_ok())
            .collect();

        let kept = select::by_rating_spread(verified, constants::PER_DEPTH);
        let path =
            std::path::Path::new(out_dir).join(format!("{}_{depth}.jsonl", constants::FILE_STEM));
        std::fs::write(&path, puzzle::to_jsonl(&kept)?)?;

        let ratings = match (kept.first(), kept.last()) {
            (Some(a), Some(b)) => format!("{}-{}", a.rating, b.rating),
            _ => "none".to_owned(),
        };
        println!(
            "mate in {depth}: kept {} (ratings {ratings}) -> {}",
            kept.len(),
            path.display()
        );
        total += kept.len();
    }

    println!("\n{total} puzzles written to {out_dir}/");
    Ok(())
}

/// Stream the dump, collecting unverified candidates per depth.
///
/// Stops as soon as every depth has [`constants::CANDIDATES_PER_DEPTH`]. Nothing is
/// verified here — verification costs ~25ms for a mate-in-4 against ~1µs for this
/// loop, so the two are kept apart and only the survivors of the cheap filter pay
/// for the expensive one.
fn gather(
    dump: &str,
) -> Result<std::collections::BTreeMap<usize, Vec<puzzle::Puzzle>>, Box<dyn std::error::Error>> {
    let reader = std::io::BufReader::new(dump::Archive::open(dump)?);

    let themes: Vec<(usize, String)> = constants::DEPTHS
        .iter()
        .map(|&d| (d, lichess::mate_theme(d)))
        .collect();

    let mut out: std::collections::BTreeMap<usize, Vec<puzzle::Puzzle>> = Default::default();
    let mut scanned = 0usize;
    let mut rejected = 0usize;

    for line in reader.lines() {
        let line = line?;
        scanned += 1;
        if scanned.is_multiple_of(constants::PROGRESS_EVERY) {
            let have: Vec<String> = constants::DEPTHS
                .iter()
                .map(|d| format!("{d}:{}", out.get(d).map_or(0, Vec::len)))
                .collect();
            println!("  scanned {scanned} rows, candidates {}", have.join(" "));
        }

        // Cheap reject first: ~97% of rows are not mates at all, and this check is a
        // substring scan against a line we already have in hand.
        if !line.contains(constants::MATE_THEME_HINT) {
            continue;
        }
        let Ok(row) = lichess::Row::of_csv(&line) else {
            rejected += 1;
            continue;
        };
        let Some((depth, _)) = themes.iter().find(|(_, theme)| row.has_theme(theme)) else {
            continue;
        };
        let depth = *depth;

        let bucket = out.entry(depth).or_default();
        if bucket.len() >= constants::CANDIDATES_PER_DEPTH {
            // Enough of this depth. If every depth is full, we are done — no reason
            // to read the remaining millions of rows.
            if constants::DEPTHS
                .iter()
                .all(|d| out.get(d).map_or(0, Vec::len) >= constants::CANDIDATES_PER_DEPTH)
            {
                println!("  all depths full after {scanned} rows");
                break;
            }
            continue;
        }

        match lichess::of_row(&row) {
            Ok(p) if p.depth == depth => bucket.push(p),
            // A row whose line length disagrees with its own theme tag. Not our
            // problem to fix; just not a candidate.
            Ok(_) => rejected += 1,
            Err(_) => rejected += 1,
        }
    }

    println!("scanned {scanned} rows, {rejected} malformed or mislabelled");
    Ok(out)
}
