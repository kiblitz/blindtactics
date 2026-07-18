//! Which puzzle the user is on, and how a solved line is stepped through.
//!
//! No Leptos here, and no DOM: these are plain values, so the rules about which
//! puzzle comes next and what the reveal shows are testable under native
//! `cargo test`. The components in [`crate::app`] hold them in signals.
//!
//! The attempt itself — the drawn line, the verdict, and the reveal cursor — is
//! [`Attempt`], a plain value here rather than loose signals in [`crate::app`].
//! `app` holds exactly one `RwSignal<Attempt>` and mutates it through the methods
//! below; the reset invariant ("these move together, or the board ends up revealed
//! on a fresh puzzle") lives in `Attempt::reset` where a native test can reach it.
//!
//! It deliberately does **not** wrap [`blindfold_core::mate::judge`]. Validating a
//! submission is exactly that call — the same one the curation tool makes — and a
//! wrapper here would be a second opinion that could drift from the database's.

use crate::constants;
use crate::rating;
use blindfold_core::arrow;
use blindfold_core::mate;
use blindfold_core::puzzle;

/// The user's puzzle set and their place in it.
///
/// There is no tier or filter: a puzzle is just a puzzle, and its depth is never
/// shown. The next one is picked at random from those rated near the user (see
/// [`choose_near`]), so difficulty tracks the user's Elo without the sequence
/// being predictable.
#[derive(Clone, Debug)]
pub struct Session {
    puzzles: Vec<puzzle::Puzzle>,
    /// Index into `puzzles` of the puzzle on screen.
    at: usize,
}

impl Session {
    /// Panics on an empty set: the database is compiled in and re-proved by
    /// `tests/database.rs`, so "no puzzles" is a broken build, not a state the UI
    /// could render something sensible for.
    pub fn new(puzzles: Vec<puzzle::Puzzle>) -> Self {
        assert!(!puzzles.is_empty(), "the embedded database is never empty");
        Self { puzzles, at: 0 }
    }

    pub fn current(&self) -> &puzzle::Puzzle {
        &self.puzzles[self.at]
    }

    /// How many puzzles there are — shown so the set does not feel bottomless.
    pub fn total(&self) -> usize {
        self.puzzles.len()
    }

    /// Move to a random puzzle rated near `rating`, never the current one. `r` is
    /// the randomness, in `0.0..1.0` (the caller supplies `Math::random`).
    pub fn advance(&mut self, rating: u32, r: f64) {
        self.at = choose_near(&self.puzzles, rating, Some(self.at), r);
    }

    /// Seat the first puzzle near `rating`, with nothing excluded — used once on
    /// load so even the opening puzzle is random rather than always index 0.
    pub fn reseat(&mut self, rating: u32, r: f64) {
        self.at = choose_near(&self.puzzles, rating, None, r);
    }
}

/// Index of the puzzle to serve: uniformly at random among the
/// [`constants::SELECTION_POOL`] puzzles whose rating is nearest `rating`,
/// excluding `exclude`. `r` is the randomness, in `0.0..1.0`.
///
/// A pure function of its inputs so a test can pin it: ties in rating distance
/// break by index, so the choice is fully determined by `r`. "Nearest N then pick
/// one" rather than a fixed window because it always yields a candidate — a window
/// can be empty between rating clusters — and it needs no widening logic.
pub fn choose_near(
    puzzles: &[puzzle::Puzzle],
    rating: u32,
    exclude: Option<usize>,
    r: f64,
) -> usize {
    let mut candidates: Vec<usize> = (0..puzzles.len()).filter(|&i| Some(i) != exclude).collect();
    candidates.sort_by_key(|&i| (puzzles[i].rating.abs_diff(rating), i));
    candidates.truncate(constants::SELECTION_POOL);

    // Non-empty whenever there is more than one puzzle: `exclude` drops at most
    // one, and `new` forbids an empty set. Guard the arithmetic anyway — a
    // one-puzzle database would otherwise index out of bounds.
    let Some(last) = candidates.len().checked_sub(1) else {
        return exclude.unwrap_or(0);
    };
    let pick = ((r.clamp(0.0, 1.0) * candidates.len() as f64) as usize).min(last);
    candidates[pick]
}

/// What came back from submitting a line.
///
/// A rendering of [`mate::Verdict`], and the reason it exists rather than the
/// `Verdict` being rendered directly: [`Solve::Solved`] carries the replay, which
/// costs a search to produce, and recomputing it on every re-render of the board
/// would be absurd.
///
/// `PartialEq` is what lets a [`leptos::prelude::Memo`] deduplicate it: stepping
/// the reveal changes `ply` many times while `solve` stays the same
/// `Solved(steps)`, and without dedup the verdict would re-render — and
/// re-announce, under `aria-live` — on every step.
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
/// Total rather than panicking on an out-of-range `ply`: degrading to "show the
/// start" beats an index panic that takes the page down.
pub fn step_at(steps: &[mate::Step], ply: usize) -> Option<&mate::Step> {
    ply.checked_sub(1).and_then(|i| steps.get(i))
}

/// Judge `line` against `puzzle`, and on a solve, work out what to reveal.
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
/// Deliberately does not reveal the board, and — since a puzzle never advertises
/// its depth — does not reveal the move count either. Being told "that fails" and
/// shown the position would end the puzzle; being told *how* it fails is the
/// lesson, and keeps the position in the user's head where the exercise wants it.
///
/// Here rather than in [`crate::line`] because it is the interpretation of a
/// [`mate::Reason`] — pure, and decision logic, not markup. In the component it
/// was structurally unreachable from a native test, and two of its arms are traps
/// the tests must pin: stalemate must not be phrased as "no mate" (the classic
/// mate-solver conflation), and a pawn dragged to the last rank without a chosen
/// piece must get the promotion hint rather than a bare "illegal".
pub fn explain(reason: &mate::Reason, solver: shakmaty::Color) -> String {
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
            "Some defense survives — this line does not force mate.".to_string()
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
/// One value, not loose signals. The reason is testability: the reveal bugs this
/// project has hit lived in this cursor, and while it lived as separate signals in
/// `app`, kept in step by a hand-rolled `reset` closure, no native test could
/// reach it. Here the transitions are plain methods on a plain value, and
/// [`crate::session`]'s tests drive them directly.
///
/// The reveal is stepped by hand, not animated: `submit` lands the cursor on the
/// final position and [`step_back`](Attempt::step_back) /
/// [`step_forward`](Attempt::step_forward) walk it, the way a Lichess analysis
/// board does. So there is no timer, and none of the epoch/idempotence guarding a
/// timer needed.
///
/// Fields are private because the invariants — `ply` stays within the reveal, and
/// `scored` flips exactly once per puzzle — are the whole point, and a caller
/// reaching past the methods could break them.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Attempt {
    arrows: Vec<arrow::Arrow>,
    solve: Option<Solve>,
    /// Which ply of a solved line the board is showing. `0` is the start position
    /// the user was holding; `steps.len()` is the mate. Meaningful only while
    /// `solve` is `Solved`.
    ply: usize,
    /// Whether this puzzle has already moved the user's rating. Set by the first
    /// definitive submission and not cleared until [`reset`](Attempt::reset) starts
    /// a new puzzle, so retrying a missed puzzle cannot farm or bleed rating.
    scored: bool,
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

    /// Set the promotion piece on the arrow at `index`. A no-op if `index` is out
    /// of range or the line is locked. Unlike a toggle, this always sets the given
    /// role — the board's promotion popup offers a definite choice, and a cancel is
    /// handled by removing the arrow, not by clearing its promotion.
    pub fn set_promotion(&mut self, index: usize, role: shakmaty::Role) {
        if self.is_solved() {
            return;
        }
        if let Some(a) = self.arrows.get_mut(index) {
            a.promotion = Some(role);
        }
    }

    /// Record a verdict and, on a solve, land the reveal on the final position.
    ///
    /// Returns the [`rating::Outcome`] to apply *only* for the first definitive
    /// submission on this puzzle — a solve or a refutation, but not an `Unjudged`
    /// which is not the user's fault. Later submissions return `None`, so failing
    /// and then solving still counts as the miss it was, and re-solving a puzzle
    /// does not inflate the rating.
    pub fn submit(&mut self, verdict: Solve) -> Option<rating::Outcome> {
        let outcome = match &verdict {
            Solve::Solved(_) => Some(rating::Outcome::Solved),
            Solve::Refuted { .. } => Some(rating::Outcome::Failed),
            Solve::Unjudged(_) => None,
        };
        self.ply = match &verdict {
            Solve::Solved(steps) => steps.len(),
            _ => 0,
        };
        self.solve = Some(verdict);

        match outcome {
            Some(o) if !self.scored => {
                self.scored = true;
                Some(o)
            }
            _ => None,
        }
    }

    /// Clear the line and the verdict and start a fresh attempt at a new puzzle.
    /// Resets the reveal cursor and the scored flag, so the next puzzle rates on
    /// its own first submission.
    pub fn reset(&mut self) {
        self.arrows.clear();
        self.solve = None;
        self.ply = 0;
        self.scored = false;
    }

    /// Step the reveal one ply toward the start. Saturates at `0` (the position the
    /// user was holding).
    pub fn step_back(&mut self) {
        self.ply = self.ply.saturating_sub(1);
    }

    /// Step the reveal one ply toward the mate. Stops at the last step, never past
    /// it — one past the end would fall [`step_at`] back to the start position.
    pub fn step_forward(&mut self) {
        if let Some(steps) = self.steps() {
            if self.ply < steps.len() {
                self.ply += 1;
            }
        }
    }

    /// Whether stepping back would move the reveal — for disabling the control.
    pub fn can_step_back(&self) -> bool {
        self.steps().is_some() && self.ply > 0
    }

    /// Whether stepping forward would move the reveal — for disabling the control.
    pub fn can_step_forward(&self) -> bool {
        self.steps().is_some_and(|steps| self.ply < steps.len())
    }
}
