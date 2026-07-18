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
use shakmaty::Position as _;

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
    /// The user gave up and asked to see the answer. Carries the same reveal as
    /// [`Solved`](Solve::Solved) — the plies to step through — but built from the
    /// puzzle's *stored* solution rather than the user's line, since there is no
    /// winning line of theirs to play out. A concession, scored as a loss (see
    /// [`Attempt::give_up`]); distinct from `Solved` so the board is revealed and
    /// walkable without claiming the user found the mate.
    GaveUp(Vec<mate::Step>),
    /// The line mates, but before its last arrow — so some arrows the user drew
    /// are never played against any defense. Treated as a miss, not a solve: the
    /// user committed to moves past the mate, which is a wrong line even though a
    /// prefix of it mates. `mate_at` is how many arrows actually delivered the
    /// mate, kept for tests; it is deliberately not shown, since revealing it would
    /// leak the puzzle's depth.
    Overshot { mate_at: usize },
    /// Some defense survives it.
    Refuted {
        defense: Vec<arrow::Arrow>,
        reason: mate::Reason,
    },
    /// A last-rank move with no promotion piece chosen — the per-move control left
    /// at its "no promotion" default on what is actually a pawn promotion. An
    /// incomplete *entry*, not a wrong answer: it does not score (see [`Attempt::
    /// submit`]), so a user who forgot to pick a piece is hinted and can fix it and
    /// still get credit, rather than eating an unrecoverable rating loss.
    ///
    /// Read off the same necessary-not-sufficient geometry as the promotion control
    /// ([`arrow::Arrow::could_be_promotion`]), so a genuinely illegal non-pawn move
    /// sharing that geometry lands here too — the safe direction, since the worst
    /// case is not penalising an ambiguous illegal input, and the hint's wording
    /// ("*if* a pawn makes that move…") stays honest either way.
    Incomplete(arrow::Arrow),
    /// We declined to find out. Not a wrong answer, and never reported as one —
    /// see [`mate::Verdict::TooComplex`]. No database puzzle can reach it.
    Unjudged(mate::Limit),
}

impl Solve {
    /// The plies the reveal steps through, for the two states that reveal the board
    /// ([`Solved`](Solve::Solved) and [`GaveUp`](Solve::GaveUp)); `None` for a verdict
    /// that keeps the board hidden. One accessor so "is this revealed, and what does it
    /// walk?" is a single question, not two matches that could disagree.
    pub fn steps(&self) -> Option<&[mate::Step]> {
        match self {
            Solve::Solved(steps) | Solve::GaveUp(steps) => Some(steps),
            _ => None,
        }
    }
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

/// One ply in the reveal's move list: its SAN and the reveal cursor it corresponds
/// to.
///
/// `at` is the 1-based cursor value — the same index [`Attempt::ply`] holds and
/// [`Attempt::step_to`] takes — so a click on this ply jumps the board straight to
/// the position after it, with no off-by-one at the call site.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ply {
    pub at: usize,
    pub san: String,
}

/// One full move of the reveal — a move number and the ply for each side. Either
/// side can be absent: a line can begin on Black's move (so `white` is `None` in the
/// first row) or end on White's (so `black` is `None` in the last).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub number: u32,
    pub white: Option<Ply>,
    pub black: Option<Ply>,
}

/// The reveal as a Lichess-style move list: SAN for every ply, grouped into
/// numbered rows by side.
///
/// Only meaningful once the board is revealed, so this leaks nothing about depth —
/// by the time it renders, the mate is already on screen. SAN (`Qh5#`, `exd5`)
/// rather than the coordinate arrows the "Your line" panel shows: this is analysis a
/// chess player reads, and the position it is read against is visible.
///
/// A pure function of the start position and the plies, here rather than in the
/// component so a native test pins the numbering — which side leads, and how the
/// fullmove count advances — the same reason the rest of the reveal's arithmetic
/// lives in `session` rather than in markup.
pub fn movelist(start: &shakmaty::Chess, steps: &[mate::Step]) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        // The position the ply was played from: the start for the first ply, the
        // previous ply's result thereafter. Its turn and fullmove number are what
        // place this ply in the list.
        let before = if i == 0 {
            start.clone()
        } else {
            steps[i - 1].after.clone()
        };
        let number = before.fullmoves().get();
        let white_to_move = before.turn() == shakmaty::Color::White;
        let san = shakmaty::san::SanPlus::from_move(before, step.played).to_string();
        let ply = Ply { at: i + 1, san };

        // White always opens a fresh row. Black attaches to the current row when it
        // is White's move that is missing its reply; otherwise (a line that opens on
        // Black) it starts its own row so the number still reads correctly.
        match rows.last_mut() {
            Some(last) if !white_to_move && last.number == number && last.black.is_none() => {
                last.black = Some(ply);
            }
            _ if white_to_move => rows.push(Row {
                number,
                white: Some(ply),
                black: None,
            }),
            _ => rows.push(Row {
                number,
                white: None,
                black: Some(ply),
            }),
        }
    }
    rows
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
        // The mate arrived before the last arrow, so arrows past it are never
        // played against any defense — the user drew moves that do not belong. A
        // prefix mates, but the line as drawn is wrong. (`judge` returns the moment
        // the frontier empties, so it never even resolves the surplus arrows.)
        mate::Verdict::Mates { moves } if moves < line.len() => Solve::Overshot { mate_at: moves },
        mate::Verdict::Mates { .. } => Solve::Solved(
            mate::playback(&position, line).expect("judge just proved this line mates"),
        ),
        // An illegal last-rank move with no piece chosen is almost always a pawn
        // promotion the user left at the control's default, not a wrong answer. Peel
        // it off before the generic refutation so `submit` can decline to score it.
        mate::Verdict::Refuted {
            reason: mate::Reason::Illegal(a),
            ..
        } if is_unfinished_promotion(&a, position.turn()) => Solve::Incomplete(a),
        mate::Verdict::Refuted { defense, reason } => Solve::Refuted { defense, reason },
        mate::Verdict::TooComplex { reason } => Solve::Unjudged(reason),
    }
}

/// The playback of a puzzle's own stored solution — the plies a give-up reveals.
///
/// Give-up has no line of the user's to play out, so it reveals the puzzle's stored
/// answer instead. Extracted here, beside [`solve`], so the "the stored solution
/// mates from the start position" invariant is native-tested rather than encoded
/// inline in the component — the same rule that keeps [`solve`]'s own playback here.
pub fn reveal(puzzle: &puzzle::Puzzle) -> Vec<mate::Step> {
    let position = puzzle.position().expect("the database is verified");
    mate::playback(&position, &puzzle.solution).expect("the stored solution mates")
}

/// Whether an illegal arrow looks like a pawn promotion left with no piece chosen —
/// the per-move control at its "no promotion" default on a last-rank move.
///
/// The one predicate behind both [`Solve::Incomplete`] (which declines to score it)
/// and [`explain`]'s promotion hint, so the classification and its wording cannot
/// drift: if they disagreed, an `Incomplete` would render through `explain`'s generic
/// "not legal" arm instead of the promotion hint. Necessary, not sufficient (see
/// [`arrow::Arrow::could_be_promotion`]) — a non-pawn move sharing the geometry
/// matches too, which is the safe direction for both uses.
fn is_unfinished_promotion(a: &arrow::Arrow, solver: shakmaty::Color) -> bool {
    a.could_be_promotion(solver) && a.promotion.is_none()
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
        mate::Reason::Illegal(a) if is_unfinished_promotion(a, solver) => {
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
    /// Whether the board is flipped from the point-of-view preference for *this*
    /// puzzle. A transient view toggle, so it lives here (reset per puzzle by
    /// [`reset`](Attempt::reset)) rather than in the persisted [`crate::settings`],
    /// and is deliberately not locked once revealed: flipping to view the mate from
    /// the other side is useful precisely after the reveal.
    flipped: bool,
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

    /// Whether the board is flipped from the point-of-view preference for this
    /// puzzle.
    pub fn flipped(&self) -> bool {
        self.flipped
    }

    /// Flip the board for this puzzle, or flip it back. Unlike the drawing edits
    /// this is *not* locked once revealed — flipping to read the revealed mate from
    /// the other side is a legitimate thing to do after the board appears.
    pub fn flip(&mut self) {
        self.flipped = !self.flipped;
    }

    /// The line has been solved: the user found the mate. Distinct from
    /// [`is_revealed`](Attempt::is_revealed) — giving up also reveals the board, but is
    /// not a solve. It is the predicate form of "the user found the mate": the win
    /// message keys on that distinction (the [`Verdict`](crate::line) component reaches
    /// it by matching the [`Solve::Solved`] variant directly, since it must tell the
    /// reveal states apart anyway), while the board reveal and the drawing lock — which
    /// a give-up shares — gate on `is_revealed`.
    pub fn is_solved(&self) -> bool {
        matches!(self.solve, Some(Solve::Solved(_)))
    }

    /// The board is revealed and drawing is locked — the user either solved it or gave
    /// up. Both show the pieces and the stepped-through line, and both must stop
    /// further drawing, so the reveal-and-lock behaviour keys on this rather than on
    /// [`is_solved`](Attempt::is_solved).
    pub fn is_revealed(&self) -> bool {
        self.steps().is_some()
    }

    /// The replay's plies, or `None` when the board is not revealed. Present for both
    /// a solve and a give-up (see [`Solve::steps`]).
    pub fn steps(&self) -> Option<&[mate::Step]> {
        self.solve.as_ref().and_then(Solve::steps)
    }

    /// Append one arrow to the line. Ignored once the board is revealed: it is
    /// locked, and a stray draw must not extend a line that has already been judged
    /// (or a puzzle that has been given up on).
    pub fn draw(&mut self, arrow: arrow::Arrow) {
        if self.is_revealed() {
            return;
        }
        self.arrows.push(arrow);
    }

    /// Drop the last arrow. Ignored once revealed, for the same reason as [`draw`].
    pub fn undo(&mut self) {
        if self.is_revealed() {
            return;
        }
        self.arrows.pop();
    }

    /// Drop every arrow. Ignored once revealed.
    pub fn clear(&mut self) {
        if self.is_revealed() {
            return;
        }
        self.arrows.clear();
    }

    /// Set (or clear) the promotion piece on the arrow at `index`. A no-op if
    /// `index` is out of range or the line is locked. `None` is a real choice, not a
    /// cancel: the line's per-move promotion control defaults to "no promotion" and
    /// can be set back to it, so the move stays a plain move.
    pub fn set_promotion(&mut self, index: usize, role: Option<shakmaty::Role>) {
        if self.is_revealed() {
            return;
        }
        if let Some(a) = self.arrows.get_mut(index) {
            a.promotion = role;
        }
    }

    /// Record a verdict and, on a solve, land the reveal on the final position.
    ///
    /// Returns the [`rating::Outcome`] to apply *only* for the first definitive
    /// submission on this puzzle — a solve or a refutation, but not an `Unjudged` or
    /// an `Incomplete`, neither of which is a wrong answer. Later submissions return
    /// `None`, so failing and then solving still counts as the miss it was, and
    /// re-solving a puzzle does not inflate the rating.
    pub fn submit(&mut self, verdict: Solve) -> Option<rating::Outcome> {
        let outcome = match &verdict {
            Solve::Solved(_) => Some(rating::Outcome::Solved),
            // An overshoot mates but with junk arrows past the mate — a wrong line,
            // so it scores a loss like a refutation.
            Solve::Overshot { .. } | Solve::Refuted { .. } => Some(rating::Outcome::Failed),
            // An unfinished promotion is a fixable entry, not a wrong answer, and an
            // `Unjudged` is not the user's fault — neither scores, and neither latches
            // `scored`, so the corrected line can still rate. `GaveUp` never reaches
            // `submit` (it is produced only by [`give_up`](Attempt::give_up), which
            // scores it there); the arm is for exhaustiveness.
            Solve::Incomplete(_) | Solve::Unjudged(_) | Solve::GaveUp(_) => None,
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

    /// Concede the puzzle and reveal its stored solution, landing the cursor on the
    /// mate so it can be stepped back through. `steps` is the playback of the
    /// puzzle's own solution (the caller builds it — [`Attempt`] has no puzzle).
    ///
    /// Returns [`rating::Outcome::Failed`] to apply, but only for the *first* scoring
    /// event on this puzzle: giving up counts as a loss like a wrong submission, yet a
    /// user who already missed it (and was already scored) is not docked twice, so a
    /// later give-up just reveals. A no-op once the board is already revealed — you
    /// cannot give up on a puzzle you have solved or already conceded.
    ///
    /// Clears the drawn line: on a solve the arrows *are* the solution and stay on the
    /// board, but a give-up's arrows are whatever the user drew (often a wrong stab, or
    /// nothing), and leaving them would paint stray arrows over the revealed answer.
    pub fn give_up(&mut self, steps: Vec<mate::Step>) -> Option<rating::Outcome> {
        if self.is_revealed() {
            return None;
        }
        self.arrows.clear();
        self.ply = steps.len();
        self.solve = Some(Solve::GaveUp(steps));
        if self.scored {
            None
        } else {
            self.scored = true;
            Some(rating::Outcome::Failed)
        }
    }

    /// Clear the line and the verdict and start a fresh attempt at a new puzzle.
    /// Resets the reveal cursor, the scored flag, and the board flip — so the next
    /// puzzle rates on its own first submission and opens at the POV preference, the
    /// flip being a per-puzzle view choice.
    pub fn reset(&mut self) {
        self.arrows.clear();
        self.solve = None;
        self.ply = 0;
        self.scored = false;
        self.flipped = false;
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

    /// Jump the reveal straight to `ply` — the cursor a move list entry names (see
    /// [`Ply::at`]). Clamped to the reveal's length, and a no-op when the board is not
    /// revealed, so an out-of-range or stale index degrades to a sensible position
    /// rather than panicking.
    pub fn step_to(&mut self, ply: usize) {
        let Some(len) = self.steps().map(<[mate::Step]>::len) else {
            return;
        };
        self.ply = ply.min(len);
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
