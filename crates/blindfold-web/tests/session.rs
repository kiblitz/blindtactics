//! Tests for puzzle navigation and submission.
//!
//! Built from the real embedded database rather than fixtures: `Session`'s job is
//! to walk *that* set, and its interesting states — a tier that runs out, a filter
//! that admits nothing near the cursor — depend on the real distribution of
//! depths.

use blindfold_core::arrow;
use blindfold_core::mate;
use blindfold_core::puzzle;
use blindfold_web::database;
use blindfold_web::session;

fn session() -> session::Session {
    session::Session::new(database::load())
}

/// How many puzzles the embedded database holds at `depth`.
///
/// Measured from the database rather than spelled as `100`, which is
/// `blindfold_curate::constants::PER_DEPTH` — a curation *policy* this crate does
/// not own and cannot see. The set is explicitly meant to be regenerated at a
/// different size, and these tests are about `Session`'s walking of it, not about
/// how big it is.
fn embedded(depth: usize) -> usize {
    database::load().iter().filter(|p| p.depth == depth).count()
}

fn embedded_total() -> usize {
    database::load().len()
}

/// Everything the filter admits, in the order `advance` would visit it.
fn walk(s: &mut session::Session, steps: usize) -> Vec<String> {
    (0..steps)
        .map(|_| {
            let id = s.current().id.clone();
            s.advance();
            id
        })
        .collect()
}

#[test]
fn starts_on_the_first_puzzle_with_everything_in_scope() {
    let s = session();
    assert_eq!(s.filter(), session::Filter::All);
    assert_eq!(s.current().depth, 1, "the gentlest tier first");
    assert_eq!(s.total(), database::load().len());
    assert_eq!(s.ordinal(), 1);
}

#[test]
fn depths_are_the_tiers_the_database_actually_holds() {
    assert_eq!(session().depths(), vec![1, 2, 3, 4]);
}

/// The filter's whole point. A tier that leaves a mate-in-1 on screen while
/// claiming to show mate-in-4 is worse than no filter at all.
#[test]
fn choosing_a_tier_lands_on_that_tier() {
    let mut s = session();
    for depth in s.depths() {
        s.show(session::Filter::Depth(depth));
        assert_eq!(s.current().depth, depth);
        assert_eq!(s.ordinal(), 1, "a chosen tier starts at its first puzzle");
    }
}

/// `advance` must never leave the filter — the bug this guards is a scan that
/// walks off the end of a tier into the next one, silently serving mate-in-2s to
/// someone drilling mate-in-1.
#[test]
fn advancing_never_leaves_the_chosen_tier() {
    let mut s = session();
    for depth in [1, 2, 3, 4] {
        s.show(session::Filter::Depth(depth));
        let n = s.total();
        // A full lap and a bit, so the wrap is exercised too.
        for _ in 0..n + 3 {
            assert_eq!(s.current().depth, depth, "advance escaped tier {depth}");
            s.advance();
        }
    }
}

/// Wraps rather than stopping: the user is drilling, and there is nothing useful
/// at the end of a tier.
#[test]
fn a_tier_wraps_back_to_its_first_puzzle() {
    let mut s = session();
    s.show(session::Filter::Depth(3));
    let first = s.current().id.clone();
    let n = s.total();
    let lap = walk(&mut s, n);
    assert_eq!(s.current().id, first, "a full lap returns to the start");
    assert_eq!(
        lap.iter().collect::<std::collections::HashSet<_>>().len(),
        lap.len(),
        "a lap visits each puzzle once"
    );
}

/// `ordinal` and `len` are what "12 of 100" is built from, so they have to be
/// counted within the filter, not across the whole set.
#[test]
fn ordinal_counts_within_the_tier() {
    let mut s = session();
    s.show(session::Filter::Depth(4));
    assert_eq!(s.ordinal(), 1);
    s.advance();
    assert_eq!(s.ordinal(), 2, "the second mate-in-4, not the 102nd puzzle");
    assert_eq!(s.total(), embedded(4));

    s.show(session::Filter::All);
    assert_eq!(s.total(), embedded_total());
    assert!(
        embedded(4) < embedded_total(),
        "a tier must be a strict subset, or this test proves nothing"
    );
}

/// Under `All`, advancing walks every puzzle of every depth — the tiers are one
/// list, not four that stop at their own end.
#[test]
fn all_walks_the_whole_database() {
    let mut s = session();
    let n = s.total();
    let seen = walk(&mut s, n);
    assert_eq!(
        seen.iter().collect::<std::collections::HashSet<_>>().len(),
        embedded_total(),
        "every puzzle exactly once"
    );
}

/// The line the user drew is judged, never compared to the stored one. This is
/// the difference between a trainer and a lookup table, and it is checked here on
/// a real puzzle rather than trusted.
#[test]
fn the_stored_line_is_judged_a_mate() {
    let s = session();
    let p = s.current();
    assert!(matches!(
        session::solve(p, &p.solution),
        session::Solve::Solved(_)
    ));
}

/// A line that stops one move short must be refuted with `NoMate` — not accepted
/// because its prefix matched.
#[test]
fn a_line_cut_short_is_refuted() {
    let mut s = session();
    s.show(session::Filter::Depth(3));
    let p = s.current();
    let short = &p.solution[..p.solution.len() - 1];
    match session::solve(p, short) {
        session::Solve::Refuted { reason, .. } => {
            assert_eq!(reason, mate::Reason::NoMate);
        }
        other => panic!("a two-move line must not solve a mate in 3: {other:?}"),
    }
}

/// A refutation must carry the defense that beat the line, not just the fact that
/// one exists. For a blindfold user who cannot see the board, that replay is the
/// entire feedback — an empty `defense` would render "The defense that holds: "
/// with nothing after it.
#[test]
fn a_refutation_names_the_defense_that_holds() {
    let mut s = session();
    s.show(session::Filter::Depth(3));
    let p = s.current();
    let short = &p.solution[..p.solution.len() - 1];
    match session::solve(p, short) {
        session::Solve::Refuted { defense, .. } => {
            assert!(
                !defense.is_empty(),
                "a surviving line against a mate in 3 has at least one defending move"
            );
            assert!(
                defense.iter().all(|a| a.from != a.to),
                "every defending move is a real move, not a null placeholder"
            );
        }
        other => panic!("a two-move line must not solve a mate in 3: {other:?}"),
    }
}

/// The plies a solved mate-in-3 replays, for the `step_at` tests. A mate in N
/// replays `2N - 1` plies.
fn solved_steps() -> Vec<mate::Step> {
    let mut s = session();
    s.show(session::Filter::Depth(3));
    let p = s.current();
    match session::solve(p, &p.solution) {
        session::Solve::Solved(steps) => steps,
        other => panic!("the stored mate-in-3 line must solve: {other:?}"),
    }
}

/// `step_at(steps, ply)` shows the move *just played* — the `(ply - 1)`th step —
/// and `None` at ply 0, where nothing has been played and the board is still the
/// puzzle's own position. The off-by-one is the whole reason this function exists
/// instead of two copies at the call sites, so it is pinned at both ends.
#[test]
fn step_at_shows_the_ply_before_the_cursor() {
    let steps = solved_steps();
    assert!(steps.len() >= 5, "a mate in 3 replays 2*3 - 1 = 5 plies");

    assert!(
        session::step_at(&steps, 0).is_none(),
        "ply 0 is the start: nothing played yet"
    );
    assert_eq!(session::step_at(&steps, 1), Some(&steps[0]));
    assert_eq!(session::step_at(&steps, 2), Some(&steps[1]));

    let last = steps.len();
    assert_eq!(
        session::step_at(&steps, last),
        Some(&steps[last - 1]),
        "the final ply shows the mating move, not one past it"
    );
}

/// Out-of-range degrades to `None` rather than panicking: the reveal's clock is a
/// timer, the one part of the app that can be wrong about its own state, and an
/// index panic there takes the page down mid-reveal.
#[test]
fn step_at_is_total_past_the_end() {
    let steps = solved_steps();
    assert!(session::step_at(&steps, steps.len() + 1).is_none());
    assert!(session::step_at(&[], 0).is_none());
    assert!(session::step_at(&[], 5).is_none());
}

fn arrow(from: shakmaty::Square, to: shakmaty::Square) -> arrow::Arrow {
    arrow::Arrow {
        from,
        to,
        promotion: None,
    }
}

/// Stalemate must never be phrased as "no mate". It is a draw, which refutes a
/// mate as surely as survival does, but a user told "no mate" after stalemating
/// would hunt for a mate that was there — the classic mate-solver conflation, and
/// the reason `mate::Reason` keeps the two variants distinct all the way to here.
#[test]
fn explain_keeps_stalemate_distinct_from_no_mate() {
    let stalemate = session::explain(&mate::Reason::Stalemate, 3, shakmaty::Color::White);
    let no_mate = session::explain(&mate::Reason::NoMate, 3, shakmaty::Color::White);

    assert_ne!(
        stalemate, no_mate,
        "folding stalemate into no-mate sends the user looking for the wrong mistake"
    );
    assert!(
        stalemate.to_lowercase().contains("draw") || stalemate.to_lowercase().contains("stalemate"),
        "stalemate must be named as a draw, got: {stalemate}"
    );
    assert!(
        no_mate.contains('3'),
        "no-mate names the depth the defense survived, got: {no_mate}"
    );
}

/// A pawn dragged to the last rank with no piece chosen gets the promotion hint,
/// not a bare "illegal" — the user cannot see that what they moved was a pawn, so
/// "pick what it becomes" is the only actionable thing to say.
#[test]
fn explain_hints_at_promotion_for_an_unfinished_pawn_move() {
    let promoting = arrow(shakmaty::Square::G7, shakmaty::Square::G8);
    let hint = session::explain(&mate::Reason::Illegal(promoting), 2, shakmaty::Color::White);
    assert!(
        hint.to_lowercase().contains("promote"),
        "an unfinished promotion must hint at promotion, got: {hint}"
    );
}

/// An ordinary illegal arrow — one that could not be a promotion — gets the plain
/// message, not the promotion hint, or the hint would fire on moves it makes no
/// sense for.
#[test]
fn explain_does_not_cry_promotion_for_an_ordinary_illegal_move() {
    let ordinary = arrow(shakmaty::Square::E2, shakmaty::Square::E4);
    let message = session::explain(&mate::Reason::Illegal(ordinary), 2, shakmaty::Color::White);
    assert!(
        !message.to_lowercase().contains("promote"),
        "a non-promoting move must not get the promotion hint, got: {message}"
    );
    assert!(message.to_lowercase().contains("not legal"));
}

// --- Attempt: the state machine the reveal's clock runs on --------------------
//
// Folding the attempt out of `app`'s loose signals is what lets these run
// natively; the reveal's two historic bugs both lived in this cursor, and the
// browser test now covers only the Leptos wiring around it.

#[test]
fn a_fresh_attempt_is_empty_and_blind() {
    let a = session::Attempt::new();
    assert!(a.arrows().is_empty());
    assert!(a.solve().is_none());
    assert_eq!(a.ply(), 0);
    assert!(!a.is_solved());
    assert!(a.steps().is_none());
}

#[test]
fn drawing_undoing_and_clearing_edit_the_line() {
    let mut a = session::Attempt::new();
    a.draw(arrow(shakmaty::Square::E2, shakmaty::Square::E4));
    a.draw(arrow(shakmaty::Square::D2, shakmaty::Square::D4));
    assert_eq!(a.arrows().len(), 2);
    a.undo();
    assert_eq!(
        a.arrows(),
        [arrow(shakmaty::Square::E2, shakmaty::Square::E4)]
    );
    a.clear();
    assert!(a.arrows().is_empty());
}

/// Once solved the board is locked; a stray draw/undo/clear must not touch a line
/// that has already been judged.
#[test]
fn a_solved_line_is_locked_against_edits() {
    let mut a = session::Attempt::new();
    a.draw(arrow(shakmaty::Square::E2, shakmaty::Square::E4));
    a.submit(session::Solve::Solved(solved_steps()));
    assert!(a.is_solved());
    a.draw(arrow(shakmaty::Square::D2, shakmaty::Square::D4));
    a.undo();
    a.clear();
    assert_eq!(a.arrows().len(), 1, "edits are ignored once solved");
}

#[test]
fn toggle_promotion_sets_replaces_and_clears() {
    let mut a = session::Attempt::new();
    a.draw(arrow(shakmaty::Square::G7, shakmaty::Square::G8));
    a.toggle_promotion(0, shakmaty::Role::Queen);
    assert_eq!(a.arrows()[0].promotion, Some(shakmaty::Role::Queen));
    a.toggle_promotion(0, shakmaty::Role::Rook);
    assert_eq!(
        a.arrows()[0].promotion,
        Some(shakmaty::Role::Rook),
        "a different role replaces the choice"
    );
    a.toggle_promotion(0, shakmaty::Role::Rook);
    assert_eq!(a.arrows()[0].promotion, None, "the same role clears it");
    a.toggle_promotion(5, shakmaty::Role::Queen); // out of range: must not panic
}

#[test]
fn submit_starts_the_reveal_and_bumps_the_epoch() {
    let mut a = session::Attempt::new();
    let before = a.epoch();
    a.submit(session::Solve::Solved(solved_steps()));
    assert_eq!(a.ply(), 0);
    assert!(a.epoch() > before, "a new attempt has a new epoch");
    assert!(a.steps().is_some());
}

#[test]
fn reset_clears_everything_and_bumps_the_epoch() {
    let mut a = session::Attempt::new();
    a.draw(arrow(shakmaty::Square::E2, shakmaty::Square::E4));
    a.submit(session::Solve::Solved(solved_steps()));
    let before = a.epoch();
    a.reset();
    assert!(a.arrows().is_empty());
    assert!(a.solve().is_none());
    assert_eq!(a.ply(), 0);
    assert!(a.epoch() > before);
}

#[test]
fn tick_advances_one_ply_for_the_attempt_that_armed_it() {
    let mut a = session::Attempt::new();
    a.submit(session::Solve::Solved(solved_steps()));
    let e = a.epoch();
    assert!(a.tick(0, e), "the armed ply advances");
    assert_eq!(a.ply(), 1);
    assert!(a.tick(1, e));
    assert_eq!(a.ply(), 2);
}

/// Idempotence: a second timer armed for a ply already stepped must not step it
/// again, or the replay skips a move.
#[test]
fn a_repeated_tick_for_the_same_ply_is_ignored() {
    let mut a = session::Attempt::new();
    a.submit(session::Solve::Solved(solved_steps()));
    let e = a.epoch();
    assert!(a.tick(0, e));
    assert!(!a.tick(0, e), "the same ply must not advance twice");
    assert_eq!(a.ply(), 1);
}

/// Identity: a timer armed under one attempt must not step the next. This is the
/// exact bug the old ply-only guard let through — `reset` puts ply back to 0, so a
/// stale timer armed at ply 0 sailed through a check that only compared plies.
#[test]
fn a_stale_tick_from_a_previous_attempt_is_rejected() {
    let mut a = session::Attempt::new();
    a.submit(session::Solve::Solved(solved_steps()));
    let stale = a.epoch();
    a.reset(); // as if the user hit "Next"
    a.submit(session::Solve::Solved(solved_steps())); // a fresh attempt, back at ply 0
    let fresh = a.epoch();
    assert_ne!(stale, fresh);
    assert!(
        !a.tick(0, stale),
        "a timer from the previous attempt must not step the new reveal"
    );
    assert_eq!(a.ply(), 0);
    assert!(a.tick(0, fresh), "the current timer still advances it");
    assert_eq!(a.ply(), 1);
}

/// Driving the clock to the end reproduces a full reveal: a mate in N replays
/// `2N - 1` plies and then stops.
#[test]
fn ticking_through_the_reveal_walks_every_ply_once() {
    let mut a = session::Attempt::new();
    let steps = solved_steps();
    let n = steps.len();
    a.submit(session::Solve::Solved(steps));
    let e = a.epoch();
    for at in 0..n {
        assert!(a.tick(at, e), "ply {at} must advance");
    }
    assert_eq!(a.ply(), n);
    assert!(
        !a.tick(n, e),
        "there is nothing past the last ply to step to"
    );
}

/// An empty set is a broken build, not a state to render. Pinned because the UI
/// calls `current()` unconditionally, and that is only sound if this panics.
#[test]
#[should_panic(expected = "never empty")]
fn an_empty_database_is_a_broken_build() {
    session::Session::new(Vec::<puzzle::Puzzle>::new());
}

/// A filter nothing matches leaves the session where it was rather than emptying
/// it. It cannot arise from the UI — the chips are built from `depths()` — but
/// `current()` must stay valid regardless.
#[test]
fn a_tier_with_no_puzzles_is_ignored() {
    let mut s = session();
    s.show(session::Filter::Depth(2));
    let before = s.current().id.clone();
    s.show(session::Filter::Depth(99));
    assert_eq!(s.current().id, before);
    assert_eq!(
        s.filter(),
        session::Filter::Depth(2),
        "the filter is unchanged"
    );
    assert!(s.total() > 0);
}
