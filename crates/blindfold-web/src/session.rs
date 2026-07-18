//! Which puzzle the user is on, and how a replay is read.
//!
//! No Leptos here, and no DOM: these are plain values, so the rules about which
//! puzzle comes next and what a replay shows are testable under native
//! `cargo test`. The components in [`crate::app`] hold them in signals.
//!
//! The attempt itself — the drawn line, the verdict, the replay cursor, and the
//! timer epoch that guards it — is [`Attempt`], a plain value here rather than four
//! loose signals in [`crate::app`]. `app` holds exactly one `RwSignal<Attempt>` and
//! mutates it through the methods below; the reset invariant ("these move together,
//! or the board ends up revealed on a fresh puzzle") lives in `Attempt::reset`
//! where a native test can reach it, not in a hand-rolled closure it cannot. Both
//! of the reveal's historic bugs lived in this cursor, so its transitions are
//! pinned here.
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
///
/// `PartialEq` is what lets a [`leptos::prelude::Memo`] deduplicate it: the reveal
/// advances `ply` many times while `solve` stays the same `Solved(steps)`, and
/// without dedup the verdict would re-render — and re-announce, under `aria-live`
/// — on every tick.
#[derive(Clone, PartialEq, Eq, Debug)]
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

/// The user's attempt at the current puzzle: the line drawn, the verdict once
/// submitted, and the reveal's cursor.
///
/// One value, not four loose signals. The reason is testability: both reactive
/// bugs this project has hit lived in the reveal's cursor — the replay froze after
/// one ply, and a stale timer stepped the wrong attempt — and while these lived as
/// separate signals in `app`, kept in step by a hand-rolled `reset` closure, no
/// native test could reach them. Here the transitions are plain methods on a plain
/// value, and [`crate::session`]'s tests drive them directly. `app` holds exactly
/// one `RwSignal<Attempt>` and drives its reveal with [`Attempt::tick`]; the
/// browser test covers the Leptos wiring that a native test still cannot.
///
/// Fields are private because the invariant — `ply`, `solve` and `epoch` move
/// together — is the whole point, and a caller reaching past the methods could
/// break it exactly the way the four loose signals could.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Attempt {
    arrows: Vec<arrow::Arrow>,
    solve: Option<Solve>,
    /// How many plies of a solved line have been played out. Meaningful only while
    /// `solve` is `Solved`.
    ply: usize,
    /// Identity of the current attempt. A reveal timer captures the epoch it was
    /// armed under and refuses to step an attempt it no longer belongs to — a
    /// counter rather than the puzzle's id, because "same puzzle, resubmitted" is a
    /// new attempt too.
    epoch: u64,
}

impl Attempt {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn arrows(&self) -> &[arrow::Arrow] {
        &self.arrows
    }

    pub fn solve(&self) -> Option<&Solve> {
        self.solve.as_ref()
    }

    pub fn ply(&self) -> usize {
        self.ply
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The line has been solved: the board is revealed and drawing is locked.
    pub fn is_solved(&self) -> bool {
        matches!(self.solve, Some(Solve::Solved(_)))
    }

    /// The replay's plies, or `None` when the attempt is not a solve.
    pub fn steps(&self) -> Option<&[mate::Step]> {
        match &self.solve {
            Some(Solve::Solved(steps)) => Some(steps),
            _ => None,
        }
    }

    /// Append one arrow to the line. Ignored once solved: the board is locked, and
    /// a stray draw must not extend a line that has already been judged.
    pub fn draw(&mut self, arrow: arrow::Arrow) {
        if self.is_solved() {
            return;
        }
        self.arrows.push(arrow);
    }

    /// Drop the last arrow. Ignored once solved, for the same reason as [`draw`].
    pub fn undo(&mut self) {
        if self.is_solved() {
            return;
        }
        self.arrows.pop();
    }

    /// Drop every arrow. Ignored once solved.
    pub fn clear(&mut self) {
        if self.is_solved() {
            return;
        }
        self.arrows.clear();
    }

    /// Toggle the promotion piece on the arrow at `index`. Tapping the chosen role
    /// again clears it, so a misread of the roster is one tap to undo rather than a
    /// cleared line. A no-op if `index` is out of range or the line is locked.
    pub fn toggle_promotion(&mut self, index: usize, role: shakmaty::Role) {
        if self.is_solved() {
            return;
        }
        if let Some(a) = self.arrows.get_mut(index) {
            a.promotion = (a.promotion != Some(role)).then_some(role);
        }
    }

    /// Record a verdict and start its reveal at ply 0. Bumps the epoch, so a timer
    /// from a previous submission of the same puzzle cannot step this one.
    pub fn submit(&mut self, verdict: Solve) {
        self.solve = Some(verdict);
        self.ply = 0;
        self.epoch += 1;
    }

    /// Clear the line and the verdict and start a fresh attempt. Bumps the epoch so
    /// a reveal timer still in flight cannot step the new attempt forward — the
    /// guard the old hand-rolled `reset` closure needed and, in an earlier version,
    /// got wrong.
    pub fn reset(&mut self) {
        self.arrows.clear();
        self.solve = None;
        self.ply = 0;
        self.epoch += 1;
    }

    /// Advance the reveal by one ply, but only if this call still belongs to the
    /// current attempt (`epoch`), has not already been applied (`ply`), and there is
    /// a ply left to reveal. Returns whether it advanced.
    ///
    /// Three guards. `epoch` is identity: a timer outlives the attempt that armed
    /// it, so one in flight when the user hits "Next" must not step the next reveal.
    /// `ply` is idempotence: the reveal effect can arm a second timer for a ply it
    /// already armed, and two unguarded increments would skip a move. The first
    /// version had only the ply check and a comment claiming it was the ownership
    /// check; it was not, and a timer armed at ply 0 sailed through it because
    /// `reset` puts ply back to 0 too.
    ///
    /// The bound (`at < steps`) lives here, not only in `app`'s reveal effect that
    /// drives it: `app` already declines to arm a timer past the last step, but a
    /// value that stays correct without trusting its one caller is the whole reason
    /// the cursor was extracted. Without it, one tick past the end pushes `ply`
    /// beyond the steps and [`step_at`] falls back to the *start* position, flashing
    /// the board back mid-reveal.
    pub fn tick(&mut self, at: usize, epoch: u64) -> bool {
        let remaining = self.steps().is_some_and(|steps| at < steps.len());
        if self.epoch == epoch && self.ply == at && remaining {
            self.ply += 1;
            true
        } else {
            false
        }
    }
}
