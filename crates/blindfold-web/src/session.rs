//! Which puzzle the user is on, and how a replay is read.
//!
//! No Leptos here, and no DOM: these are plain values, so the rules about which
//! puzzle comes next and what a replay shows are testable under native
//! `cargo test`. The components in [`crate::app`] hold them in signals.
//!
//! **The attempt itself — the drawn line, the verdict, the replay cursor, and the
//! timer epoch that guards it — is *not* here.** It lives as four signals
//! (`arrows`, `solve`, `ply`, `epoch`) in [`crate::app`], written by three
//! different components, which is the granularity Leptos wants. That is a real gap
//! and worth naming: `Session` guards its own cursor invariant ("always admitted
//! by `filter`") in one place, while the attempt's equivalent is a hand-rolled
//! `reset` closure whose own comment admits the risk. Folding them into an
//! `Attempt` value with `reset`/`draw`/`undo` would be the consistent thing, and
//! would make the reveal's clock natively testable. Deferred, not rejected — see
//! CLAUDE.md.
//!
//! It deliberately does **not** wrap [`blindfold_core::mate::judge`]. Validating
//! a submission is exactly that call — the same one the curation tool makes — and
//! a wrapper here would be a second opinion that could drift from the database's.

use blindfold_core::arrow;
use blindfold_core::mate;
use blindfold_core::puzzle;

/// Which depths the user wants to see.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    /// Every puzzle, in depth order.
    All,
    /// Only mates in this many moves.
    Depth(usize),
}

impl Filter {
    fn admits(self, p: &puzzle::Puzzle) -> bool {
        match self {
            Filter::All => true,
            Filter::Depth(d) => p.depth == d,
        }
    }
}

/// The user's puzzle set and their place in it.
#[derive(Clone, Debug)]
pub struct Session {
    puzzles: Vec<puzzle::Puzzle>,
    /// Index into `puzzles` of the puzzle on screen. Always admitted by `filter`,
    /// an invariant every mutator restores rather than one callers must keep.
    at: usize,
    filter: Filter,
}

impl Session {
    /// Panics on an empty set: the database is compiled in and re-proved by
    /// `tests/database.rs`, so "no puzzles" is a broken build, not a state the UI
    /// could render something sensible for.
    pub fn new(puzzles: Vec<puzzle::Puzzle>) -> Self {
        assert!(!puzzles.is_empty(), "the embedded database is never empty");
        Self {
            puzzles,
            at: 0,
            filter: Filter::All,
        }
    }

    pub fn current(&self) -> &puzzle::Puzzle {
        &self.puzzles[self.at]
    }

    pub fn filter(&self) -> Filter {
        self.filter
    }

    /// Every depth present in the set, ascending — what the filter control offers.
    pub fn depths(&self) -> Vec<usize> {
        let mut depths: Vec<usize> = self.puzzles.iter().map(|p| p.depth).collect();
        depths.sort_unstable();
        depths.dedup();
        depths
    }

    /// How many puzzles the current filter admits — the "of 100" in "12 of 100".
    ///
    /// Not `len`: this is not a collection, and clippy is right that a `len`
    /// without an `is_empty` is a smell. `is_empty` would be the wrong method to
    /// add, because it can never be true — `new` rejects an empty set and `show`
    /// ignores a filter that admits nothing, so a session always has a current
    /// puzzle. `total` pairs with [`Session::ordinal`], which is the only place
    /// either is read.
    pub fn total(&self) -> usize {
        self.puzzles
            .iter()
            .filter(|p| self.filter.admits(p))
            .count()
    }

    /// The current puzzle's 1-based place within the filter, for "12 of 100".
    pub fn ordinal(&self) -> usize {
        self.puzzles[..self.at]
            .iter()
            .filter(|p| self.filter.admits(p))
            .count()
            + 1
    }

    /// Move to the next puzzle the filter admits, wrapping at the end.
    ///
    /// Wraps rather than stopping because there is nothing useful at the end of a
    /// tier — the user is drilling, not reading to a conclusion.
    pub fn advance(&mut self) {
        let n = self.puzzles.len();
        for step in 1..=n {
            let next = (self.at + step) % n;
            if self.filter.admits(&self.puzzles[next]) {
                self.at = next;
                return;
            }
        }
        // Unreachable while `at` is admitted, which `show` guarantees: the loop
        // above visits every index including `at` itself.
    }

    /// Narrow to `filter` and land on its first puzzle.
    ///
    /// Jumps rather than staying put, because the current puzzle usually is not
    /// in the new tier, and a filter that leaves a mate-in-1 on screen while
    /// claiming to show mate-in-4 is worse than no filter. A `filter` that admits
    /// nothing is ignored — it cannot arise from the UI, which builds its
    /// controls from [`Session::depths`].
    pub fn show(&mut self, filter: Filter) {
        let Some(first) = self.puzzles.iter().position(|p| filter.admits(p)) else {
            return;
        };
        self.filter = filter;
        self.at = first;
    }
}

/// What came back from submitting a line.
///
/// A rendering of [`mate::Verdict`], and the reason it exists rather than the
/// `Verdict` being rendered directly: [`Solve::Solved`] carries the replay, which
/// costs a search to produce, and recomputing it on every re-render of an
/// animating board would be absurd.
#[derive(Clone, Debug)]
pub enum Solve {
    /// The line mates against every defense. Carries the reveal.
    Solved(Vec<mate::Step>),
    /// Some defense survives it.
    Refuted {
        defense: Vec<arrow::Arrow>,
        reason: mate::Reason,
    },
    /// We declined to find out. Not a wrong answer, and never reported as one —
    /// see [`mate::Verdict::TooComplex`]. No database puzzle can reach it.
    Unjudged(mate::Limit),
}

/// What to show at `ply` of a replay: the step just played, or `None` at the
/// start, before any of it has run.
///
/// Its own function because [`crate::app`] needs it twice — once for the position
/// to draw, once for the square to light — and two copies of an off-by-one is one
/// too many. `None` at `ply == 0` is the puzzle's own position, which is what the
/// user was holding in their head.
///
/// Total rather than panicking on an out-of-range `ply`: the reveal's clock is a
/// timer, and a timer is the one part of this app that can be wrong about what
/// state it is in. Degrading to "show the start" beats an index panic that takes
/// the page down mid-reveal.
pub fn step_at(steps: &[mate::Step], ply: usize) -> Option<&mate::Step> {
    ply.checked_sub(1).and_then(|i| steps.get(i))
}

/// Judge `line` against `puzzle`, and on a solve, work out what to animate.
///
/// The one place the app decides right from wrong, and it is one `judge` call —
/// no comparison against `puzzle.solution`. That is the difference between a
/// trainer that accepts any mate the user finds and one that only accepts the
/// mate Lichess happened to record; the database has duals by design, and a
/// blindfold user cannot see that they were right.
pub fn solve(puzzle: &puzzle::Puzzle, line: &[arrow::Arrow]) -> Solve {
    let position = puzzle.position().expect("the database is verified");
    match mate::judge(&position, line) {
        mate::Verdict::Mates { .. } => Solve::Solved(
            mate::playback(&position, line).expect("judge just proved this line mates"),
        ),
        mate::Verdict::Refuted { defense, reason } => Solve::Refuted { defense, reason },
        mate::Verdict::TooComplex { reason } => Solve::Unjudged(reason),
    }
}

/// Turn a refutation into a sentence a blindfold user can act on.
///
/// Deliberately does not reveal the board. Being told "that fails" and shown the
/// position would end the puzzle; being told *how* it fails is the lesson, and
/// keeps the position in the user's head where the exercise wants it.
///
/// Here rather than in [`crate::line`] because it is the interpretation of a
/// [`mate::Reason`] — pure, and decision logic, not markup. In the component it
/// was structurally unreachable from a native test, and two of its arms are traps
/// the tests must pin: stalemate must not be phrased as "no mate" (the classic
/// mate-solver conflation), and a pawn dragged to the last rank without a chosen
/// piece must get the promotion hint rather than a bare "illegal".
pub fn explain(reason: &mate::Reason, depth: usize, solver: shakmaty::Color) -> String {
    match reason {
        mate::Reason::Illegal(a) if a.could_be_promotion(solver) && a.promotion.is_none() => {
            format!(
                "Arrow {a} has no legal reading. If a pawn makes that move it has to \
                 promote — pick what it becomes."
            )
        }
        mate::Reason::Illegal(a) => {
            format!("Arrow {a} is not legal against every defense.")
        }
        mate::Reason::NoMate => {
            format!("Some defense survives all {depth} moves.")
        }
        // Stalemate is a draw, and a draw refutes a mate as surely as survival
        // does. Named explicitly because conflating the two is the classic
        // mate-solver bug, and a user told "no mate" after stalemating would go
        // looking for the wrong mistake.
        mate::Reason::Stalemate => "Some defense is stalemated — a draw, not a mate.".to_string(),
    }
}
