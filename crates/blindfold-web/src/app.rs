//! The root component: holds the state, wires the panels to the board.

use crate::board;
use crate::database;
use crate::line;
use crate::panel;
use crate::rating;
use crate::session;
use crate::square;
use blindfold_core::arrow;
use blindfold_core::roster;
use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    let session = RwSignal::new(session::Session::new(database::load()));
    // The user's puzzle Elo, loaded from localStorage (or the starting value), and
    // the change from the last scored puzzle so it can be shown like chess.com's
    // "+8". The rating steers which puzzle comes next, so seat the opening one near
    // it rather than always landing on index 0.
    let elo = RwSignal::new(rating::load());
    let elo_delta = RwSignal::new(None::<i32>);
    session.update(|s| s.reseat(elo.get_untracked(), js_sys::Math::random()));

    // The whole attempt in one signal, so its reset invariant lives in one place a
    // native test can reach — see `session::Attempt`.
    let attempt = RwSignal::new(session::Attempt::new());

    // A promotion choice in progress: the popup's square and the provisional
    // arrow's index. Lifted out of the board so submission can be blocked while it
    // is open — the picker opens on geometry alone, so an unresolved move behind it
    // must not be judged.
    let promoting: board::Promoting = RwSignal::new(None);
    let choosing = Signal::derive(move || promoting.get().is_some());

    // `Memo`, not `Signal::derive`: each caches its value and clones only the
    // current puzzle (via `session.with`), not the whole 400-puzzle `Session`, and
    // only recomputes when the puzzle actually changes.
    //
    // The `position` memo dedups on `shakmaty::Chess`, whose `PartialEq` ignores the
    // clocks and a non-capturable ep square (see CLAUDE.md). Safe here: those are
    // exactly the fields the roster omits and the answer does not turn on, so a
    // dedup that treats two clock-only-different positions as equal shows an
    // identical board — and judging reads the `puzzle` memo, not this one.
    let puzzle = Memo::new(move |_| session.with(|s| s.current().clone()));
    let position = Memo::new(move |_| {
        puzzle.with(|p| p.position().expect("the embedded database is verified"))
    });
    let solver =
        Memo::new(move |_| puzzle.with(|p| p.solver().expect("the embedded database is verified")));

    // The attempt projected for the view. `Memo`, not a plain derive, so stepping
    // the reveal — which changes `attempt` but not the drawn line or the verdict —
    // does not re-render the line panel or, worse, re-announce the verdict under
    // its `aria-live`. The reveal signals below are memos for the same reason: the
    // pieces layer redraws only when the position it shows actually changes.
    let drawn = Memo::new(move |_| attempt.with(|a| a.arrows().to_vec()));
    let solve = Memo::new(move |_| attempt.with(|a| a.solve().cloned()));
    let solved = Memo::new(move |_| attempt.with(session::Attempt::is_solved));
    let can_back = Memo::new(move |_| attempt.with(session::Attempt::can_step_back));
    let can_forward = Memo::new(move |_| attempt.with(session::Attempt::can_step_forward));

    let next = move |_| {
        let r = js_sys::Math::random();
        let rating_now = elo.get_untracked();
        session.update(|s| s.advance(rating_now, r));
        attempt.update(session::Attempt::reset);
        elo_delta.set(None);
        // Provably already `None` here (a puzzle only advances once solved, and the
        // picker cannot be open on a solved board), but cleared anyway so the "no
        // stale picker across puzzles" invariant is not spread across three modules.
        promoting.set(None);
    };

    let submit = move |_| {
        let line = attempt.with_untracked(|a| a.arrows().to_vec());
        let verdict = puzzle.with_untracked(|p| session::solve(p, &line));
        let puzzle_rating = puzzle.with_untracked(|p| p.rating);
        // `submit` returns the outcome only for the first scoring submission on the
        // puzzle, so a miss-then-solve or a re-solve does not move the rating twice.
        let outcome = attempt.try_update(|a| a.submit(verdict)).flatten();
        if let Some(outcome) = outcome {
            let before = elo.get_untracked();
            let after = rating::update(before, puzzle_rating, outcome);
            elo.set(after);
            // No badge when the rating did not move — reachable only at the floor or
            // ceiling, where a win/loss clamps back to where it was, and a "+0" up-badge
            // there would read as a gain that did not happen. Both are <= ELO_CEILING
            // (3000), so the i32 cast is exact.
            elo_delta.set((after != before).then(|| after as i32 - before as i32));
            rating::save(after);
        }
    };

    let draw = move |arrow: arrow::Arrow| attempt.update(|a| a.draw(arrow));
    let undo = move |()| attempt.update(session::Attempt::undo);
    let clear = move |()| attempt.update(session::Attempt::clear);
    let promote = move |(index, role): (usize, shakmaty::Role)| {
        attempt.update(|a| a.set_promotion(index, role))
    };
    let step_back = move |()| attempt.update(session::Attempt::step_back);
    let step_forward = move |()| attempt.update(session::Attempt::step_forward);

    // The position the board draws: `None` while the user is still blind, then the
    // start position, then each ply the reveal has been stepped to. Stepped by hand
    // through the line panel's controls — there is no timer.
    let revealed = Memo::new(move |_| {
        attempt.with(|a| {
            a.steps().map(|steps| {
                session::step_at(steps, a.ply())
                    .map_or_else(|| position.get(), |step| step.after.clone())
            })
        })
    });

    // The square the move just landed on.
    //
    // Via `Arrow::of_move` rather than `Move::to()`, which for a castle returns
    // the **rook's** square — so the reveal would light h1 while the king walked
    // to g1. CLAUDE.md lists this among the shakmaty gotchas that cost real time,
    // and `of_move` exists precisely to spell a move the way a drag would.
    let highlight = Memo::new(move |_| {
        attempt.with(|a| {
            a.steps()
                .and_then(|steps| session::step_at(steps, a.ply()))
                .and_then(|step| arrow::Arrow::of_move(&step.played))
                .map(|drag| drag.to)
        })
    });

    let locked = solved;

    view! {
        <main class="app">
            <header class="masthead">
                <p class="masthead__eyebrow">"Blindfold chess trainer"</p>
                <h1 class="masthead__title">"The board stays empty."</h1>
                <p class="masthead__lede">
                    "You get a roster of squares and nothing else. Draw your line — one arrow per
                     move of your own side, in order — then submit. Solve it and the pieces appear."
                </p>
            </header>

            <RatingBar
                rating=elo
                delta=elo_delta
                total=Signal::derive(move || session.with(session::Session::total))
            />

            <div class="layout">
                <div class="layout__board">
                    {move || {
                        // Keyed on the puzzle so a new one gets a fresh board
                        // rather than one carrying the last puzzle's drag state.
                        // `track`, not `get`: subscribe without cloning the puzzle.
                        puzzle.track();
                        let orientation = square::Orientation(solver.get());
                        view! {
                            <board::Board
                                orientation=orientation
                                drawn=drawn
                                on_draw=Callback::new(draw)
                                on_promote=Callback::new(promote)
                                on_cancel=Callback::new(undo)
                                promoting=promoting
                                revealed=revealed
                                highlight=highlight
                                locked=locked
                            />
                        }
                    }}
                    <Facts session=session />
                </div>

                <aside class="layout__panels">
                    {move || {
                        // Keyed on the puzzle, like the board above, so the roster
                        // and line rebuild fresh when a new one is served.
                        puzzle.track();
                        let r = Signal::derive(move || roster::of(&position.get()));
                        view! {
                            <panel::Roster roster=r />
                            <line::Line
                                drawn=drawn
                                solver=solver.get()
                                solve=solve
                                can_back=can_back
                                can_forward=can_forward
                                choosing=choosing
                                on_undo=Callback::new(undo)
                                on_clear=Callback::new(clear)
                                on_submit=Callback::new(submit)
                                on_next=Callback::new(next)
                                on_step_back=Callback::new(step_back)
                                on_step_forward=Callback::new(step_forward)
                            />
                        }
                    }}
                </aside>
            </div>

            <footer class="colophon">
                <p>
                    "Every puzzle here is re-proved by the same solver the browser runs: a forced
                     mate against "<em>"every"</em>" defense, with a roster small enough to hold —
                     four to ten squares. Your line is played out, not compared to a stored answer,
                     so any mate you find counts."
                </p>
                <p>
                    "Puzzles from the "
                    <a href="https://database.lichess.org/#puzzles">"Lichess puzzle database"</a>
                    " (CC0). Pieces by Colin M.L. Burnett, via Lichess (GPLv2-or-later). This app is "
                    <a href="https://www.gnu.org/licenses/gpl-3.0.html">"GPL-3.0-or-later"</a> "."
                </p>
            </footer>
        </main>
    }
}

/// The user's rating, the change from the last scored puzzle, and the pool size.
///
/// Chess.com-style: the rating with a "+8" / "-6" badge beside it that appears only
/// after a scored submission and clears on the next puzzle.
#[component]
fn RatingBar(
    #[prop(into)] rating: Signal<u32>,
    /// The signed change from the last scored puzzle, or `None` when nothing has
    /// been scored since the current puzzle was served.
    #[prop(into)]
    delta: Signal<Option<i32>>,
    #[prop(into)] total: Signal<usize>,
) -> impl IntoView {
    view! {
        <div class="statusbar">
            <span class="elo">
                "Rating " <strong>{move || rating.get().to_string()}</strong>
                {move || {
                    delta
                        .get()
                        .map(|d| {
                            let up = d >= 0;
                            let sign = if up { "+" } else { "" };
                            view! {
                                <span
                                    class="elo__delta"
                                    class:elo__delta--up=up
                                    class:elo__delta--down=!up
                                >
                                    {format!("{sign}{d}")}
                                </span>
                            }
                        })
                }}
            </span>
            <span class="statusbar__count">{move || format!("{} puzzles", total.get())}</span>
        </div>
    }
}

/// The puzzle's provenance, under the board.
///
/// The id is here so a user who hits something odd can report *which* puzzle, and
/// so it can be looked up in the committed database. Rating is Lichess's, and
/// measures how hard the mate is to find with the board in front of you — which is
/// not the same as how hard this is, hence the square count beside it: that is the
/// blindfold cost, and it is what curation actually gates on. The mate depth is
/// deliberately absent — a puzzle never says how many moves it takes.
#[component]
fn Facts(session: RwSignal<session::Session>) -> impl IntoView {
    view! {
        <p class="facts mono">
            {move || {
                session.with(|s| {
                    let p = s.current();
                    let squares = roster::of(&p.position().expect("verified")).squares();
                    view! {
                        <span>{format!("id {}", p.id)}</span>
                        <span>{format!("rating {}", p.rating)}</span>
                        <span>{format!("{squares} squares to hold")}</span>
                    }
                })
            }}
        </p>
    }
}
