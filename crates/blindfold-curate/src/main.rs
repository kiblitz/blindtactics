//! Turn the Lichess puzzle dump into a verified blindfold puzzle set.
//!
//! ```text
//! blindfold-curate <lichess_db_puzzle.csv.zst> [output-dir]
//! ```
//!
//! # What this is for
//!
//! The dump has ~6M puzzles and ~1.9M carry a `mateInN` tag. Almost none of them are
//! usable here, and the tag cannot tell you which — for two independent reasons.
//!
//! Chess: Lichess stores exactly one engine-chosen defense per puzzle, so a line that
//! mates in *its* record may lose to a defense nobody wrote down. Our arrow UI cannot
//! branch, so a puzzle is only usable if the same arrows mate against **every**
//! defense. ~35% of `mateIn4` rows survive that.
//!
//! Blindfold: the user never sees the board, so a position with 32 pieces on it is
//! not a puzzle, it is a memory test. Rating does not track this and is if anything
//! anti-correlated — the cheapest mate-in-1s are opening traps with full material.
//!
//! So this tool exists to throw things away, on both counts.
//!
//! # Structure
//!
//! All the judgement lives elsewhere: `blindfold-core` for the conversion and the
//! proof, `gather`/`select` in this crate's lib for the policy. What is left here is
//! argument handling, a thread pool, and a file writer — the parts that cannot be
//! unit-tested and therefore should hold nothing worth testing.
//!
//! That the app and this tool share `blindfold-core` is the point: the database and
//! the app must agree about what "solved" means, and calling the same code is the
//! only way to guarantee it.

use blindfold_core::puzzle;
use blindfold_curate::constants;
use blindfold_curate::dump;
use blindfold_curate::gather;
use blindfold_curate::select;
use rayon::iter::IntoParallelIterator as _;
use rayon::iter::ParallelIterator as _;

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

fn run(dump_path: &str, out_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("reading {dump_path}");
    let reader = std::io::BufReader::new(dump::Archive::open(dump_path)?);
    let mut pool = gather::of_rows(reader, |pool| {
        let have: Vec<String> = constants::DEPTHS
            .iter()
            .map(|d| format!("{d}:{}", pool.candidates(*d)))
            .collect();
        println!(
            "  scanned {} rows, candidates {}",
            pool.scanned,
            have.join(" ")
        );
    })?;

    let r = &pool.rejected;
    println!(
        "scanned {} rows; rejected {} ({} malformed, {} mislabelled, {} too heavy to \
         hold, {} drawish)",
        pool.scanned,
        r.total(),
        r.malformed,
        r.mislabelled,
        r.too_heavy,
        r.drawish
    );
    if !pool.is_full() {
        println!("  note: ran out of dump before every depth filled");
    }

    std::fs::create_dir_all(out_dir)?;
    let mut total = 0usize;

    for depth in constants::DEPTHS {
        // `remove`, not `get().cloned()`: the pool is ours and each depth is read
        // exactly once, so cloning up to 6,000 puzzles per tier buys nothing.
        let found = pool.by_depth.remove(&depth).unwrap_or_default();
        println!("\nmate in {depth}: verifying {} candidates", found.len());

        // The expensive half. Each `verify` re-proves the puzzle from scratch —
        // legal, linear, mates at exactly the claimed depth, and no shorter mate
        // exists — and they are independent, so this is embarrassingly parallel.
        let verified: Vec<puzzle::Puzzle> = found
            .into_par_iter()
            .filter(|p| p.verify().is_ok())
            .collect();
        println!("mate in {depth}: {} verified", verified.len());
        if verified.len() < constants::PER_DEPTH {
            println!(
                "  warning: only {} survived, wanted {} — the file will be short",
                verified.len(),
                constants::PER_DEPTH
            );
        }

        let kept = select::by_rating_spread(verified, constants::PER_DEPTH);
        let path = std::path::Path::new(out_dir).join(constants::file_name(depth));
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
