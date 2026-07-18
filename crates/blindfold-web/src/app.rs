//! The root component: holds the state, wires the panels to the board.

use crate::board;
use crate::constants;
use crate::database;
use crate::line;
use crate::panel;
use crate::session;
use crate::square;
use blindfold_core::arrow;
use blindfold_core::roster;
use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    let session = RwSignal::new(session::Session::new(database::load()));
    // The whole attempt in one signal, so its reset invariant lives in one place a
    // native test can reach rather than in a hand-rolled closure here — see
    // `session::Attempt`.
    let attempt = RwSignal::new(session::Attempt::new());

    let puzzle = Signal::derive(move || session.get().current().clone());
    let position = Signal::derive(move || {
        puzzle.with(|p| p.position().expect("the embedded database is verified"))
    });
    let solver = Signal::derive(move || {
        puzzle.with(|p| p.solver().expect("the embedded database is verified"))
    });

    // The attempt projected for the view. `Memo`, not a plain derive, so a ply tick
    // — which changes `attempt` but not the drawn line or the verdict — does not
    // re-render the line panel or, worse, re-announce the verdict under its
    // `aria-live`. The reveal signals below are memos for the same reason: the
    // pieces layer redraws only when the position it shows actually changes.
    let drawn = Memo::new(move |_| attempt.with(|a| a.arrows().to_vec()));
    let solve = Memo::new(move |_| attempt.with(|a| a.solve().cloned()));
    let solved = Memo::new(move |_| attempt.with(session::Attempt::is_solved));

    let next = move |_| {
        session.update(session::Session::advance);
        attempt.update(session::Attempt::reset);
    };

    let submit = move |_| {
        let line = attempt.with_untracked(|a| a.arrows().to_vec());
        let verdict = puzzle.with_untracked(|p| session::solve(p, &line));
        attempt.update(|a| a.submit(verdict));
    };

    let draw = move |arrow: arrow::Arrow| attempt.update(|a| a.draw(arrow));
    let undo = move |()| attempt.update(session::Attempt::undo);
    let clear = move |()| attempt.update(session::Attempt::clear);
    let promote = move |(index, role): (usize, shakmaty::Role)| {
        attempt.update(|a| a.toggle_promotion(index, role))
    };

    // The reveal. Watches the attempt and walks the replay forward, one timer per
    // ply.
    //
    // Reading `a.ply()` through `attempt.with` is deliberately **tracked**:
    // advancing it re-runs this effect, which arms the next ply's timer. That
    // self-retriggering is the whole clock — read untracked, the effect fires once,
    // the board takes a single step and freezes there, still captioned "mate".
    // Which is exactly what it did.
    //
    // The first ply waits longer than the rest: the board has just gone from void
    // to pieces, and moving something immediately steps on the moment the user
    // solved the puzzle for. The two-guard check (identity + idempotence) lives in
    // `Attempt::tick`, where a native test can reach it.
    Effect::new(move |_| {
        let armed = attempt.with(|a| a.steps().map(|steps| (steps.len(), a.ply(), a.epoch())));
        let Some((len, at, epoch)) = armed else {
            return;
        };
        if at >= len {
            return;
        }
        let delay = if at == 0 {
            constants::REVEAL_MS
        } else {
            constants::PLAYBACK_MS
        };
        set_timeout(
            move || {
                attempt.update(|a| {
                    a.tick(at, epoch);
                });
            },
            std::time::Duration::from_millis(delay),
        );
    });

    // The position the board draws: `None` while the user is still blind, then the
    // start position, then each ply of the replay.
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
    //
    // No committed puzzle can reach it today: none of the 400 has castling rights
    // at all. That is exactly why it is worth fixing now — the database is meant
    // to be regenerated larger, the roster carries castling rights *because they
    // decide mates*, and this would come back as a wrong square with nothing
    // failing.
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

            <Tiers
                session=session
                reset=Callback::new(move |()| attempt.update(session::Attempt::reset))
            />

            <div class="layout">
                <div class="layout__board">
                    {move || {
                        // Keyed on the puzzle so a new one gets a fresh board
                        // rather than one carrying the last puzzle's drag state.
                        let p = puzzle.get();
                        let orientation = square::Orientation(solver.get());
                        view! {
                            <board::Board
                                orientation=orientation
                                drawn=drawn
                                on_draw=Callback::new(draw)
                                revealed=revealed
                                highlight=highlight
                                locked=locked
                            />
                            <line::Hint depth=p.depth />
                        }
                    }}
                    <Facts session=session />
                </div>

                <aside class="layout__panels">
                    {move || {
                        let p = puzzle.get();
                        let r = Signal::derive(move || roster::of(&position.get()));
                        view! {
                            <panel::Roster roster=r depth=p.depth />
                            <line::Line
                                drawn=drawn
                                solver=solver.get()
                                depth=p.depth
                                solve=solve
                                on_undo=Callback::new(undo)
                                on_clear=Callback::new(clear)
                                on_promote=Callback::new(promote)
                                on_submit=Callback::new(submit)
                                on_next=Callback::new(next)
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

/// The depth filter — which tier the user is drilling.
#[component]
fn Tiers(session: RwSignal<session::Session>, reset: Callback<()>) -> impl IntoView {
    let choose = move |filter: session::Filter| {
        session.update(|s| s.show(filter));
        reset.run(());
    };

    view! {
        <div class="tiers" role="group" aria-label="Choose a mate depth">
            {move || {
                let current = session.get().filter();
                let mut tiers = vec![(session::Filter::All, "All".to_string())];
                tiers
                    .extend(
                        session
                            .get()
                            .depths()
                            .into_iter()
                            .map(|d| (session::Filter::Depth(d), format!("Mate in {d}"))),
                    );
                tiers
                    .into_iter()
                    .map(|(filter, label)| {
                        view! {
                            <button
                                class="tier"
                                class:tier--on=current == filter
                                aria-pressed=move || (current == filter).to_string()
                                on:click=move |_| choose(filter)
                            >
                                {label}
                            </button>
                        }
                    })
                    .collect_view()
            }}
        </div>
    }
}

/// The puzzle's provenance and the user's place in the tier, under the board.
///
/// The id is here so a user who hits something odd can report *which* puzzle, and
/// so it can be looked up in the committed database. Rating is Lichess's, and
/// measures how hard the mate is to find with the board in front of you — which
/// is not the same as how hard this is, hence the square count beside it: that is
/// the blindfold cost, and it is what curation actually gates on.
#[component]
fn Facts(session: RwSignal<session::Session>) -> impl IntoView {
    view! {
        <p class="facts mono">
            {move || {
                let s = session.get();
                let p = s.current();
                let squares = roster::of(&p.position().expect("verified")).squares();
                view! {
                    <span>{format!("{} of {}", s.ordinal(), s.total())}</span>
                    <span>{format!("id {}", p.id)}</span>
                    <span>{format!("rating {}", p.rating)}</span>
                    <span>{format!("{squares} squares to hold")}</span>
                }
            }}
        </p>
    }
}
