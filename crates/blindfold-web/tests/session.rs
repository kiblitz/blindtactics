//! Tests for puzzle selection, submission, and the reveal cursor.
//!
//! Built from the real embedded database rather than fixtures: `Session`'s job is
//! to pick from *that* set near the user's rating, and `choose_near`'s behaviour
//! depends on the real spread of puzzle ratings.

use blindfold_core::arrow;
use blindfold_core::mate;
use blindfold_core::puzzle;
use blindfold_web::constants;
use blindfold_web::database;
use blindfold_web::rating;
use blindfold_web::session;

fn database() -> Vec<puzzle::Puzzle> {
    database::load()
}

fn session() -> session::Session {
    session::Session::new(database())
}

/// A puzzle of a given depth, straight from the database — the tests that need a
/// solvable line reach for one this way now that the session no longer filters by
/// depth.
fn puzzle_of_depth(depth: usize) -> puzzle::Puzzle {
    database()
        .into_iter()
        .find(|p| p.depth == depth)
        .expect("the embedded database holds every depth 1..=4")
}

// --- selection ---------------------------------------------------------------

#[test]
fn starts_on_a_real_puzzle() {
    let s = session();
    // `new` seats index 0; `app` reseats to a random puzzle on load. Either way a
    // session always has a current puzzle.
    assert_eq!(s.current().id, database()[0].id);
    assert_eq!(s.total(), database().len());
}

/// `choose_near` is a pure function of its inputs: same rating, same exclusion,
/// same `r` — same puzzle. This is what lets the app drive it with `Math::random`
/// and still have the selection be testable.
#[test]
fn choose_near_is_deterministic() {
    let db = database();
    let a = session::choose_near(&db, 1500, Some(3), 0.42);
    let b = session::choose_near(&db, 1500, Some(3), 0.42);
    assert_eq!(a, b);
    assert_ne!(a, 3, "the excluded puzzle is never chosen");
}

/// The chosen puzzle is always one of the `SELECTION_POOL` nearest in rating, so
/// difficulty tracks the user rather than jumping across the whole spread.
#[test]
fn choose_near_stays_within_the_nearest_pool() {
    let db = database();
    let rating = 1500;

    let mut by_distance: Vec<usize> = (0..db.len()).collect();
    by_distance.sort_by_key(|&i| (db[i].rating.abs_diff(rating), i));
    let pool: std::collections::HashSet<usize> = by_distance
        .into_iter()
        .take(constants::SELECTION_POOL)
        .collect();

    for step in 0..=20 {
        let r = f64::from(step) / 20.0;
        let chosen = session::choose_near(&db, rating, None, r);
        assert!(
            pool.contains(&chosen),
            "r={r} chose {chosen}, outside the nearest {} puzzles",
            constants::SELECTION_POOL
        );
    }
}

/// Sweeping `r` reaches more than one puzzle — the selection really is spread
/// across the pool, not pinned to the single closest.
#[test]
fn choose_near_spreads_across_the_pool_as_r_varies() {
    let db = database();
    let seen: std::collections::HashSet<usize> = (0..20)
        .map(|step| session::choose_near(&db, 1500, None, f64::from(step) / 20.0))
        .collect();
    assert!(
        seen.len() > 1,
        "randomness must reach more than one puzzle, saw {}",
        seen.len()
    );
}

/// The rating steers the difficulty: seating near a low rating lands on an easier
/// (lower-rated) puzzle than seating near a high one.
#[test]
fn selection_follows_the_rating() {
    let mut low = session();
    low.reseat(600, 0.0);
    let mut high = session();
    high.reseat(2600, 0.0);
    assert!(
        low.current().rating < high.current().rating,
        "low {} should be easier than high {}",
        low.current().rating,
        high.current().rating
    );
}

/// `advance` never serves the same puzzle twice in a row.
#[test]
fn advancing_moves_to_a_new_puzzle() {
    let mut s = session();
    let before = s.current().id.clone();
    s.advance(1500, 0.5);
    assert_ne!(s.current().id, before);
}

// --- judging -----------------------------------------------------------------

/// The line the user drew is judged, never compared to the stored one. This is
/// the difference between a trainer and a lookup table.
#[test]
fn the_stored_line_is_judged_a_mate() {
    let p = puzzle_of_depth(2);
    assert!(matches!(
        session::solve(&p, &p.solution),
        session::Solve::Solved(_)
    ));
}

/// A line that stops one move short must be refuted with `NoMate`, carrying the
/// defense that beat it — for a blindfold user that replay is the entire feedback.
#[test]
fn a_line_cut_short_is_refuted_with_a_named_defense() {
    let p = puzzle_of_depth(3);
    let short = &p.solution[..p.solution.len() - 1];
    match session::solve(&p, short) {
        session::Solve::Refuted { defense, reason } => {
            assert_eq!(reason, mate::Reason::NoMate);
            assert!(!defense.is_empty(), "a surviving line has a defending move");
            assert!(defense.iter().all(|a| a.from != a.to));
        }
        other => panic!("a two-move line must not solve a mate in 3: {other:?}"),
    }
}

/// A line that mates but then keeps going is not a solve. `judge` returns as soon
/// as the frontier empties, so on a mate-in-1 the line `[key, junk]` mated at move
/// 1 and the trailing arrow was never examined — the app happily accepted it. A
/// user who draws moves past the mate has drawn a wrong line, so this must be a miss.
#[test]
fn a_line_that_overshoots_the_mate_is_not_a_solve() {
    let p = puzzle_of_depth(1);
    let mut line = p.solution.clone();
    // An extra arrow that can never be played: after the mate the game is over, so
    // its content is irrelevant — `judge` never resolves it.
    line.push(arrow(shakmaty::Square::A1, shakmaty::Square::A2));
    match session::solve(&p, &line) {
        session::Solve::Overshot { mate_at } => assert_eq!(mate_at, 1),
        other => panic!("a line with arrows past the mate must not solve: {other:?}"),
    }
}

/// An overshoot scores a loss (the line was wrong) and does not reveal the board.
#[test]
fn an_overshoot_scores_a_loss_and_does_not_reveal() {
    let mut a = session::Attempt::new();
    assert_eq!(
        a.submit(session::Solve::Overshot { mate_at: 1 }),
        Some(rating::Outcome::Failed)
    );
    assert!(
        !a.is_solved(),
        "an overshoot is a miss, so the board stays blind"
    );
    assert_eq!(a.ply(), 0);
}

/// A pawn promotion left at the per-move control's "no promotion" default is an
/// incomplete entry, not a wrong answer. It must not score — and crucially must not
/// latch `scored` — so a user who forgot to pick a piece can fix it and still get
/// credit, rather than eating an unrecoverable rating loss. (Before this, the
/// unresolved move judged as `Refuted { Illegal }`, scored a `Failed`, and latched,
/// so the corrected mate then returned `None` and never rated.)
#[test]
fn an_unfinished_promotion_is_not_scored_and_does_not_latch() {
    let p = database()
        .into_iter()
        .find(|p| p.solution.iter().any(|a| a.promotion.is_some()))
        .expect("the embedded database holds a puzzle whose key is a promotion");

    // Exactly what a user who ignored the promotion control submits: the right line
    // with the promotion piece blanked off the promoting arrow.
    let unfinished: Vec<arrow::Arrow> = p
        .solution
        .iter()
        .map(|a| arrow::Arrow {
            promotion: None,
            ..*a
        })
        .collect();

    assert!(
        matches!(
            session::solve(&p, &unfinished),
            session::Solve::Incomplete(_)
        ),
        "an unfinished promotion is classified as incomplete, not refuted"
    );

    let mut a = session::Attempt::new();
    assert_eq!(
        a.submit(session::solve(&p, &unfinished)),
        None,
        "an unfinished promotion does not score"
    );
    assert!(!a.is_solved(), "and does not reveal the board");
    assert_eq!(
        a.submit(session::solve(&p, &p.solution)),
        Some(rating::Outcome::Solved),
        "picking the piece and resubmitting the real mate still scores the win"
    );
}

/// The board flip is a per-puzzle view toggle: it flips and flips back, works even
/// after the board is revealed (unlike the drawing edits, which lock on a solve),
/// and resets to the POV preference when the next puzzle loads.
#[test]
fn flipping_toggles_is_unlocked_and_resets_with_the_puzzle() {
    let mut a = session::Attempt::new();
    assert!(!a.flipped(), "a fresh attempt opens at the POV preference");
    a.flip();
    assert!(a.flipped());
    a.flip();
    assert!(!a.flipped(), "flipping again flips back");

    // Flip is not locked by a solve — reading the revealed mate from the other side
    // is a legitimate thing to do, where drawing is not.
    a.submit(session::Solve::Solved(solved_steps()));
    assert!(a.is_solved());
    a.flip();
    assert!(a.flipped(), "flip still works once solved");

    a.reset();
    assert!(!a.flipped(), "a new puzzle opens unflipped");
}

fn solved_steps() -> Vec<mate::Step> {
    let p = puzzle_of_depth(3);
    match session::solve(&p, &p.solution) {
        session::Solve::Solved(steps) => steps,
        other => panic!("the stored mate-in-3 line must solve: {other:?}"),
    }
}

fn arrow(from: shakmaty::Square, to: shakmaty::Square) -> arrow::Arrow {
    arrow::Arrow {
        from,
        to,
        promotion: None,
    }
}

// --- step_at -----------------------------------------------------------------

/// `step_at(steps, ply)` shows the move *just played* — the `(ply - 1)`th step —
/// and `None` at ply 0, where nothing has been played.
#[test]
fn step_at_shows_the_ply_before_the_cursor() {
    let steps = solved_steps();
    assert!(steps.len() >= 5, "a mate in 3 replays 2*3 - 1 = 5 plies");

    assert!(session::step_at(&steps, 0).is_none(), "ply 0 is the start");
    assert_eq!(session::step_at(&steps, 1), Some(&steps[0]));

    let last = steps.len();
    assert_eq!(
        session::step_at(&steps, last),
        Some(&steps[last - 1]),
        "the final ply shows the mating move, not one past it"
    );
}

#[test]
fn step_at_is_total_past_the_end() {
    let steps = solved_steps();
    assert!(session::step_at(&steps, steps.len() + 1).is_none());
    assert!(session::step_at(&[], 5).is_none());
}

// --- explain -----------------------------------------------------------------

/// Stalemate must never be phrased as "no mate", and — since a puzzle never
/// advertises its depth — neither message may name the move count.
#[test]
fn explain_keeps_stalemate_distinct_and_hides_the_depth() {
    let stalemate = session::explain(&mate::Reason::Stalemate, shakmaty::Color::White);
    let no_mate = session::explain(&mate::Reason::NoMate, shakmaty::Color::White);

    assert_ne!(stalemate, no_mate);
    assert!(
        stalemate.to_lowercase().contains("draw") || stalemate.to_lowercase().contains("stalemate"),
        "stalemate must be named as a draw, got: {stalemate}"
    );
    assert!(
        !no_mate.chars().any(|c| c.is_ascii_digit()),
        "the refutation must not reveal the move count, got: {no_mate}"
    );
}

#[test]
fn explain_hints_at_promotion_for_an_unfinished_pawn_move() {
    let promoting = arrow(shakmaty::Square::G7, shakmaty::Square::G8);
    let hint = session::explain(&mate::Reason::Illegal(promoting), shakmaty::Color::White);
    assert!(
        hint.to_lowercase().contains("promote"),
        "an unfinished promotion must hint at promotion, got: {hint}"
    );
}

#[test]
fn explain_does_not_cry_promotion_for_an_ordinary_illegal_move() {
    let ordinary = arrow(shakmaty::Square::E2, shakmaty::Square::E4);
    let message = session::explain(&mate::Reason::Illegal(ordinary), shakmaty::Color::White);
    assert!(!message.to_lowercase().contains("promote"));
    assert!(message.to_lowercase().contains("not legal"));
}

// --- spoken (read-aloud verdict) ---------------------------------------------

/// A solve is announced as a mate, and — like the panel — the spoken verdict never
/// names a move count, which would leak the puzzle's depth.
#[test]
fn spoken_announces_a_mate_without_leaking_the_depth() {
    let solved = session::spoken(
        &session::Solve::Solved(solved_steps()),
        shakmaty::Color::White,
    );
    assert!(
        solved.to_lowercase().contains("mate"),
        "a solve must be announced as a mate, got: {solved}"
    );
    for verdict in [
        session::Solve::Solved(solved_steps()),
        session::Solve::GaveUp(solved_steps()),
        session::Solve::Overshot { mate_at: 1 },
    ] {
        let spoken = session::spoken(&verdict, shakmaty::Color::White);
        assert!(
            !spoken.chars().any(|c| c.is_ascii_digit()),
            "the spoken verdict must not reveal a move count, got: {spoken}"
        );
    }
}

/// The spoken verdict and the panel's must say the same thing about a refutation:
/// `spoken` reuses `explain` rather than phrasing it a second way, so the voice mode
/// and the screen cannot drift.
#[test]
fn spoken_reuses_explain_for_a_refutation() {
    let reason = mate::Reason::Stalemate;
    let refuted = session::Solve::Refuted {
        defense: Vec::new(),
        reason: reason.clone(),
    };
    assert_eq!(
        session::spoken(&refuted, shakmaty::Color::White),
        session::explain(&reason, shakmaty::Color::White),
    );
}

/// An unfinished promotion is a fixable entry, not a wrong answer, so its spoken form
/// is the same promotion hint the panel gives — never a bare "illegal".
#[test]
fn spoken_hints_at_promotion_for_an_unfinished_pawn_move() {
    let promoting = arrow(shakmaty::Square::G7, shakmaty::Square::G8);
    let spoken = session::spoken(
        &session::Solve::Incomplete(promoting),
        shakmaty::Color::White,
    );
    assert!(
        spoken.to_lowercase().contains("promote"),
        "an unfinished promotion must hint at promotion aloud, got: {spoken}"
    );
}

// --- Attempt -----------------------------------------------------------------

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

/// Once solved the board is locked; a stray edit must not touch a judged line.
#[test]
fn a_solved_line_is_locked_against_edits() {
    let mut a = session::Attempt::new();
    a.draw(arrow(shakmaty::Square::G7, shakmaty::Square::G8));
    a.submit(session::Solve::Solved(solved_steps()));
    assert!(a.is_solved());
    a.draw(arrow(shakmaty::Square::D2, shakmaty::Square::D4));
    a.undo();
    a.clear();
    a.set_promotion(0, Some(shakmaty::Role::Queen));
    assert_eq!(a.arrows().len(), 1, "edits are ignored once solved");
}

#[test]
fn set_promotion_sets_replaces_and_clears() {
    let mut a = session::Attempt::new();
    a.draw(arrow(shakmaty::Square::G7, shakmaty::Square::G8));
    a.set_promotion(0, Some(shakmaty::Role::Queen));
    assert_eq!(a.arrows()[0].promotion, Some(shakmaty::Role::Queen));
    a.set_promotion(0, Some(shakmaty::Role::Rook));
    assert_eq!(
        a.arrows()[0].promotion,
        Some(shakmaty::Role::Rook),
        "a different role replaces the choice"
    );
    a.set_promotion(0, None);
    assert_eq!(
        a.arrows()[0].promotion,
        None,
        "`None` clears back to no promotion — the control's default"
    );
    a.set_promotion(5, Some(shakmaty::Role::Queen)); // out of range: must not panic
}

/// A solve scores a win, and only the first submission on the puzzle counts.
#[test]
fn a_solve_scores_a_win_once() {
    let mut a = session::Attempt::new();
    assert_eq!(
        a.submit(session::Solve::Solved(solved_steps())),
        Some(rating::Outcome::Solved)
    );
    assert_eq!(
        a.submit(session::Solve::Solved(solved_steps())),
        None,
        "re-solving the same puzzle must not score again"
    );
}

/// A miss scores a loss, and solving it afterward does not overwrite that with a
/// win — the first attempt is the one that rates.
#[test]
fn a_miss_then_solve_scores_only_the_miss() {
    let mut a = session::Attempt::new();
    let miss = session::Solve::Refuted {
        defense: vec![],
        reason: mate::Reason::NoMate,
    };
    assert_eq!(a.submit(miss), Some(rating::Outcome::Failed));
    assert_eq!(
        a.submit(session::Solve::Solved(solved_steps())),
        None,
        "the miss already counted; the later solve does not re-score"
    );
    assert!(a.is_solved(), "but the board still reveals on the solve");
}

/// An `Unjudged` verdict is not the user's fault, so it does not score — and a
/// definitive submission afterward still can.
#[test]
fn an_unjudged_submission_does_not_score() {
    let mut a = session::Attempt::new();
    assert_eq!(
        a.submit(session::Solve::Unjudged(mate::Limit::Length { moves: 99 })),
        None
    );
    assert_eq!(
        a.submit(session::Solve::Solved(solved_steps())),
        Some(rating::Outcome::Solved),
        "a real result after an unjudged one still scores"
    );
}

#[test]
fn reset_lets_the_next_puzzle_score_again() {
    let mut a = session::Attempt::new();
    assert!(a.submit(session::Solve::Solved(solved_steps())).is_some());
    a.reset();
    assert!(a.arrows().is_empty());
    assert!(a.solve().is_none());
    assert_eq!(a.ply(), 0);
    assert_eq!(
        a.submit(session::Solve::Solved(solved_steps())),
        Some(rating::Outcome::Solved),
        "a fresh puzzle scores on its own first submission"
    );
}

/// Submitting a solve lands the reveal on the final position, so the board shows
/// the mate first and the user steps *back* into it.
#[test]
fn submit_lands_the_reveal_on_the_mate() {
    let mut a = session::Attempt::new();
    let steps = solved_steps();
    let n = steps.len();
    a.submit(session::Solve::Solved(steps));
    assert_eq!(a.ply(), n, "the reveal opens on the mating position");
    assert!(!a.can_step_forward(), "nothing past the mate");
    assert!(a.can_step_back(), "but the line can be rewound");
}

/// Stepping walks the reveal and bounds cleanly at both ends.
#[test]
fn stepping_walks_the_reveal_and_bounds_at_the_ends() {
    let mut a = session::Attempt::new();
    let n = solved_steps().len();
    a.submit(session::Solve::Solved(solved_steps()));
    assert_eq!(a.ply(), n);

    for _ in 0..n {
        a.step_back();
    }
    assert_eq!(a.ply(), 0);
    assert!(!a.can_step_back(), "the start is the floor");
    a.step_back();
    assert_eq!(a.ply(), 0, "stepping back past the start is a no-op");

    for _ in 0..n {
        a.step_forward();
    }
    assert_eq!(a.ply(), n);
    a.step_forward();
    assert_eq!(a.ply(), n, "stepping forward past the mate is a no-op");
}

/// Stepping is inert on an unsolved attempt — there is no reveal to walk.
#[test]
fn stepping_an_unsolved_attempt_does_nothing() {
    let mut a = session::Attempt::new();
    assert!(!a.can_step_back());
    assert!(!a.can_step_forward());
    a.step_forward();
    a.step_back();
    assert_eq!(a.ply(), 0);
}

/// An empty set is a broken build, not a state to render. Pinned because the UI
/// calls `current()` unconditionally, and that is only sound if this panics.
#[test]
#[should_panic(expected = "never empty")]
fn an_empty_database_is_a_broken_build() {
    session::Session::new(Vec::<puzzle::Puzzle>::new());
}

// --- give up -----------------------------------------------------------------

/// Giving up reveals the stored solution and scores a loss — once. It reveals the
/// board like a solve (so the line can be stepped through) but is not a solve, and a
/// second give-up neither re-scores nor disturbs the reveal.
#[test]
fn giving_up_reveals_the_solution_and_scores_a_loss_once() {
    let mut a = session::Attempt::new();
    let steps = solved_steps();
    let n = steps.len();
    assert_eq!(
        a.give_up(steps),
        Some(rating::Outcome::Failed),
        "giving up counts as a loss"
    );
    assert!(a.is_revealed(), "and reveals the board");
    assert!(
        !a.is_solved(),
        "but is not a solve — the user did not find it"
    );
    assert_eq!(a.ply(), n, "the reveal opens on the mate, like a solve");
    assert_eq!(
        a.give_up(solved_steps()),
        None,
        "giving up again neither re-scores nor changes the reveal"
    );
}

/// A user who already missed the puzzle (and was scored for it) is not docked a
/// second time for then giving up — but the solution is still revealed.
#[test]
fn giving_up_after_a_miss_reveals_without_scoring_again() {
    let mut a = session::Attempt::new();
    let miss = session::Solve::Refuted {
        defense: vec![],
        reason: mate::Reason::NoMate,
    };
    assert_eq!(a.submit(miss), Some(rating::Outcome::Failed));
    assert_eq!(
        a.give_up(solved_steps()),
        None,
        "the miss already counted; giving up does not dock twice"
    );
    assert!(a.is_revealed(), "but the solution is still revealed");
}

/// You cannot give up a puzzle you have already solved: it is a no-op, and the
/// attempt stays a solve rather than being overwritten as a concession.
#[test]
fn giving_up_after_a_solve_does_nothing() {
    let mut a = session::Attempt::new();
    a.submit(session::Solve::Solved(solved_steps()));
    assert!(a.is_solved());
    assert_eq!(a.give_up(solved_steps()), None);
    assert!(a.is_solved(), "still a solve, not a give-up");
}

/// Giving up clears whatever line the user had drawn. On a solve the arrows are the
/// solution and stay on the board, but a give-up's arrows are the user's own (often a
/// wrong stab) and must not be left painted over the revealed answer.
#[test]
fn giving_up_clears_the_drawn_line() {
    let mut a = session::Attempt::new();
    a.draw(arrow(shakmaty::Square::E2, shakmaty::Square::E4));
    a.draw(arrow(shakmaty::Square::D2, shakmaty::Square::D4));
    assert_eq!(a.arrows().len(), 2, "the user drew a wrong line first");
    a.give_up(solved_steps());
    assert!(
        a.arrows().is_empty(),
        "the drawn line is cleared so it cannot overlay the reveal"
    );
}

/// `reveal` plays out a puzzle's own stored solution to the mate — the plies a give-up
/// walks. A mate in N is `2N-1` plies and ends in checkmate.
#[test]
fn reveal_plays_out_the_stored_solution_to_mate() {
    for depth in 1..=4 {
        let steps = session::reveal(&puzzle_of_depth(depth));
        assert_eq!(
            steps.len(),
            2 * depth - 1,
            "a mate in {depth} is 2N-1 plies"
        );
        assert!(
            shakmaty::Position::is_checkmate(&steps.last().expect("non-empty").after),
            "the last ply delivers mate",
        );
    }
}

// --- step_to -----------------------------------------------------------------

/// `step_to` jumps the reveal straight to a named ply and clamps a past-the-end
/// index to the mate, so a move-list click can never land out of range.
#[test]
fn step_to_jumps_within_the_reveal_and_clamps() {
    let mut a = session::Attempt::new();
    let n = solved_steps().len();
    a.submit(session::Solve::Solved(solved_steps()));
    a.step_to(1);
    assert_eq!(a.ply(), 1);
    a.step_to(0);
    assert_eq!(a.ply(), 0);
    a.step_to(n + 5);
    assert_eq!(a.ply(), n, "a past-the-end index clamps to the mate");
}

#[test]
fn step_to_is_inert_on_an_unrevealed_attempt() {
    let mut a = session::Attempt::new();
    a.step_to(3);
    assert_eq!(a.ply(), 0);
}

// --- movelist ----------------------------------------------------------------

/// The (start, plies) of a puzzle's own solved line — the reveal a solve or give-up
/// walks, and what the move list is built from. Built through [`session::reveal`], the
/// same call the give-up path uses.
fn reveal_of(p: &puzzle::Puzzle) -> (shakmaty::Chess, Vec<mate::Step>) {
    let pos = p.position().expect("the database is verified");
    (pos, session::reveal(p))
}

/// All the plies in the list, in board order.
fn plies(rows: &[session::Row]) -> Vec<&session::Ply> {
    rows.iter()
        .flat_map(|r| [r.white.as_ref(), r.black.as_ref()])
        .flatten()
        .collect()
}

/// The move list names every ply once, in order, with cursor indices that run
/// `1..=len` (so a click maps straight to [`Attempt::step_to`]) and the mate written
/// as SAN ending in `#`.
#[test]
fn the_movelist_names_every_ply_in_order() {
    let (pos, steps) = reveal_of(&puzzle_of_depth(3));
    let rows = session::movelist(&pos, &steps);
    let plies = plies(&rows);

    assert_eq!(plies.len(), steps.len(), "every ply appears exactly once");
    for (i, ply) in plies.iter().enumerate() {
        assert_eq!(ply.at, i + 1, "cursor indices run 1..=len in board order");
        assert!(!ply.san.is_empty(), "every ply has a SAN");
    }
    assert!(
        plies
            .last()
            .expect("a mate has at least one ply")
            .san
            .ends_with('#'),
        "the final ply is the mate, written in SAN"
    );
}

/// A line the solver plays as Black opens with an empty White cell, so the move
/// numbering stays honest (a `1...` in Lichess terms) rather than mislabelling
/// Black's move as White's.
#[test]
fn the_movelist_opens_on_black_when_black_moves_first() {
    let p = database()
        .into_iter()
        .find(|p| p.solver().expect("the database is verified") == shakmaty::Color::Black)
        .expect("the database holds a puzzle the solver plays as Black");
    let (pos, steps) = reveal_of(&p);
    let rows = session::movelist(&pos, &steps);

    let first = &rows[0];
    assert!(
        first.white.is_none(),
        "a line the solver opens as Black has no White ply in its first row"
    );
    assert_eq!(
        first.black.as_ref().map(|ply| ply.at),
        Some(1),
        "the solver's first move is Black's, at cursor 1"
    );
    assert_eq!(
        first.number,
        shakmaty::Position::fullmoves(&pos).get(),
        "the row keeps the position's own move number"
    );
}

// --- voice input (`interpret`) -----------------------------------------------
//
// Built from inline puzzles, not the database, because these pin the exact spoken
// wording against known positions — the database's puzzles are picked for their
// mates, not for legible test assertions.

/// A puzzle from a FEN and a space-separated UCI solution. Depth is the solution
/// length; the rating is irrelevant to `interpret`.
fn voice_puzzle(fen: &str, solution: &str) -> puzzle::Puzzle {
    let solution: Vec<arrow::Arrow> = solution
        .split_whitespace()
        .map(|u| u.parse().expect("test arrow parses"))
        .collect();
    puzzle::Puzzle {
        id: "test".to_owned(),
        fen: fen.to_owned(),
        depth: solution.len(),
        solution,
        rating: 1500,
    }
}

/// White Ra1, Kg1; Black Kg8, pawns f7 g7 h7. Mate in 1: Ra8#.
const MATE_IN_1: &str = "6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1";
/// The BRANCHING_LINEAR fixture: Kf6 Rb1 vs Kh8 a7 c7. Mate in 2: Kg6 then Rb8#.
const MATE_IN_2: &str = "7k/p1p5/5K2/8/8/8/8/1R6 w - - 0 1";
/// Two white knights (d2, f2) both reaching e4 — for the ambiguity path.
const TWO_KNIGHTS: &str = "4k3/8/8/8/8/8/3N1N2/4K3 w - - 0 1";
/// White may castle either side — for the castle paths.
const BOTH_CASTLES: &str = "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1";

#[test]
fn a_spoken_move_resolves_to_an_arrow_and_is_read_back() {
    let puzzle = voice_puzzle(MATE_IN_1, "a1a8");
    assert_eq!(
        session::interpret("rook a8", &puzzle, &[]),
        session::Heard::Draw {
            arrow: "a1a8".parse().unwrap(),
            say: "rook to A. 8.".to_owned(),
        }
    );
}

#[test]
fn a_spoken_command_is_passed_through() {
    let puzzle = voice_puzzle(MATE_IN_1, "a1a8");
    assert_eq!(
        session::interpret("submit", &puzzle, &[]),
        session::Heard::Command(blindfold_core::diction::Command::Submit)
    );
    assert_eq!(
        session::interpret("next", &puzzle, &[]),
        session::Heard::Command(blindfold_core::diction::Command::Next)
    );
}

#[test]
fn an_unrecognised_phrase_asks_to_repeat() {
    let puzzle = voice_puzzle(MATE_IN_1, "a1a8");
    assert!(matches!(
        session::interpret("mumble mumble", &puzzle, &[]),
        session::Heard::Say(_)
    ));
}

#[test]
fn a_later_move_resolves_against_the_line_played_forward() {
    // The second move is resolved at the position after the first solver move *and*
    // the opponent's reply — proving `interpret` plays the stored line forward rather
    // than resolving every move against the start.
    let puzzle = voice_puzzle(MATE_IN_2, "f6g6 b1b8");
    let first = "f6g6".parse::<arrow::Arrow>().unwrap();
    assert_eq!(
        session::interpret("king g6", &puzzle, &[]),
        session::Heard::Draw {
            arrow: first,
            say: "king to G. 6.".to_owned(),
        }
    );
    assert_eq!(
        session::interpret("rook b8", &puzzle, &[first]),
        session::Heard::Draw {
            arrow: "b1b8".parse().unwrap(),
            say: "rook to B. 8.".to_owned(),
        }
    );
}

#[test]
fn a_move_past_the_end_of_the_line_says_so() {
    // The whole mate-in-1 line is already entered, so there is no next move to make.
    let puzzle = voice_puzzle(MATE_IN_1, "a1a8");
    let entered = ["a1a8".parse::<arrow::Arrow>().unwrap()];
    assert!(matches!(
        session::interpret("rook a7", &puzzle, &entered),
        session::Heard::Say(_)
    ));
}

#[test]
fn an_ambiguous_move_is_read_back_as_a_question_not_guessed() {
    // Two knights reach e4; `interpret` must ask which by from-square, spelled aloud,
    // never silently pick one.
    let puzzle = voice_puzzle(TWO_KNIGHTS, "d2e4");
    assert_eq!(
        session::interpret("knight e4", &puzzle, &[]),
        session::Heard::Say("Which one? D. 2 or F. 2.".to_owned())
    );
}

#[test]
fn a_named_castle_resolves_and_is_read_back_as_a_castle() {
    let puzzle = voice_puzzle(BOTH_CASTLES, "e1g1");
    assert_eq!(
        session::interpret("castle kingside", &puzzle, &[]),
        session::Heard::Draw {
            arrow: "e1g1".parse().unwrap(),
            say: "Castle kingside.".to_owned(),
        }
    );
}

#[test]
fn a_bare_castle_with_two_options_asks_which_side() {
    let puzzle = voice_puzzle(BOTH_CASTLES, "e1g1");
    assert!(matches!(
        session::interpret("castle", &puzzle, &[]),
        session::Heard::Say(_)
    ));
}
