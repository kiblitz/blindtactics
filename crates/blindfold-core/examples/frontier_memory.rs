//! Measures `judge`'s real peak working set at [`constants::MAX_FRONTIER`].
//!
//! This is committed rather than thrown away because the constant's justification
//! is a memory budget, and that budget has now been gotten wrong twice by
//! multiplying the bound by `size_of::<Branch>()` and calling it done. That
//! product is not the peak: `judge` holds the old frontier alive while the new one
//! doubles its way up, and the `defense` vectors allocate off to the side. The
//! paper figure said ~150 MB where the truth was 527 MB.
//!
//! So: if you change `MAX_FRONTIER`, `Branch`, or how the frontier is advanced,
//! run this and put the number it prints into the constant's doc.
//!
//! `cargo run --release --example frontier_memory`
//!
//! Release matters — a debug build's peak is not the one that ships. The reading
//! is the host's peak working set, which is a proxy for wasm linear memory, not a
//! measurement of it; it is the right order of magnitude and that is what the
//! bound turns on. Windows-only, hence the `cfg`: it exists to be run by hand
//! during tuning, not by CI.

use blindfold_core::arrow;
use blindfold_core::constants;
use blindfold_core::mate;
use blindfold_core::position;

#[cfg(windows)]
fn peak_mb() -> f64 {
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(p: isize, c: *mut Pmc, cb: u32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
    }
    #[repr(C)]
    #[derive(Default)]
    struct Pmc {
        cb: u32,
        faults: u32,
        peak_ws: usize,
        ws: usize,
        peak_paged: usize,
        paged: usize,
        peak_nonpaged: usize,
        nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    let mut pmc = Pmc {
        cb: std::mem::size_of::<Pmc>() as u32,
        ..Default::default()
    };
    // SAFETY: `Pmc` is `#[repr(C)]` and laid out as PROCESS_MEMORY_COUNTERS on
    // x64 (72 bytes: cb@0, faults@4, peak_ws@8, ...). The kernel validates the
    // `cb` we hand it against its own expectation and fails the call if they
    // disagree, so a layout mistake surfaces as the assert below rather than as
    // a silent misread.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut pmc,
            std::mem::size_of::<Pmc>() as u32,
        )
    };
    // On failure the struct is left untouched, so `peak_ws` would still be 0 and
    // this would report a serene "0 MB" — which, in the one tool whose job is to
    // justify a memory bound, reads as "loads of headroom" and would invite
    // someone to lower MAX_FRONTIER on the strength of a failed syscall.
    assert!(
        ok != 0,
        "GetProcessMemoryInfo failed; the reading is not real"
    );
    pmc.peak_ws as f64 / BYTES_PER_MB
}

/// Everywhere else. The measurement is a hand-tuning aid, not a test, so a stub
/// beats dragging in a cross-platform memory crate — but it must still *compile*,
/// since `--all-targets` builds examples and CI will not be running Windows.
#[cfg(not(windows))]
fn peak_mb() -> f64 {
    f64::NAN
}

const BYTES_PER_MB: f64 = 1_048_576.0;

/// An unpinned copy of `tests/common/mod.rs::UNBOUNDED_FRONTIER` — an example
/// cannot import test fixtures, and promoting the fixture into the library to
/// share it would be a worse trade than this duplication.
///
/// Nothing keeps the two in step, so treat that as a real hazard rather than a
/// note: the position is load-bearing precisely because *no defense ever refutes*
/// it, which is the only reason the frontier grows to the bound at all. A copy
/// that drifted into a refutable position would still run, still print a number,
/// and quietly report a small peak — reassurance that the bound is generous, from
/// a position that never tested it.
const UNBOUNDED_FRONTIER: &str = "k7/1b6/4b3/8/2b3b1/8/8/B5K1 w - - 0 1";

/// White's dark-squared bishop shuffling a1<->b2. Long enough to reach the bound.
const SHUFFLE: &str = "a1b2 b2a1 a1b2 b2a1 a1b2 b2a1";

/// What one `mate::Branch` costs, mirrored here because `Branch` is private to
/// `mate`. Computed rather than written down: the entire point of this example is
/// that a hand-carried size is how the bound got mis-sized twice.
const fn branch_bytes() -> usize {
    std::mem::size_of::<(shakmaty::Chess, Vec<arrow::Arrow>)>()
}

fn main() {
    if cfg!(not(windows)) {
        eprintln!(
            "note: peak working set is only implemented on Windows; \
             figures will print as NaN."
        );
    }

    let pos = position::of_fen(UNBOUNDED_FRONTIER).expect("legal");
    let line: Vec<arrow::Arrow> = SHUFFLE
        .split_whitespace()
        .map(|t| t.parse().expect("valid"))
        .collect();

    println!("MAX_FRONTIER = {}", constants::MAX_FRONTIER);
    println!("baseline peak WS = {:.0} MB", peak_mb());

    let v = mate::judge(&pos, &line);

    println!("verdict = {v:?}");
    println!("peak WS after judge = {:.0} MB", peak_mb());
    println!(
        "flat frontier at bound = {:.0} MB ({} B/branch)",
        (constants::MAX_FRONTIER * branch_bytes()) as f64 / BYTES_PER_MB,
        branch_bytes()
    );
}
