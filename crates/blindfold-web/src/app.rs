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
    let arrows: RwSignal<Vec<arrow::Arrow>> = RwSignal::new(Vec::new());
    let solve: RwSignal<Option<session::Solve>> = RwSignal::new(None);
    // How many plies of a solved line have been played out. Only meaningful while
    // `solve` is `Solved`; reset with it.
    let ply = RwSignal::new(0usize);
    // Bumped whenever the attempt changes identity, so a timer can tell whether
    // it still belongs to the reveal that armed it. A counter rather than the
    // puzzle's id because "same puzzle, resubmitted" is also a different attempt.
    let epoch = RwSignal::new(0u64);

    let puzzle = Signal::derive(move || session.get().current().clone());
    let position = Signal::derive(move || {
        puzzle.with(|p| p.position().expect("the embedded database is verified"))
    });
    let solver = Signal::derive(move || {
        puzzle.with(|p| p.solver().expect("the embedded database is verified"))
    });

    // Everything about the attempt, cleared together. Separate `set` calls in
    // three handlers is how a board ends up revealed on a fresh puzzle.
    let reset = move || {
        arrows.set(Vec::new());
        solve.set(None);
        ply.set(0);
        epoch.update(|e| *e += 1);
    };

    let next = move |_| {
        session.update(session::Session::advance);
        reset();
    };

    let submit = move |_| {
        let verdict = puzzle.with(|p| session::solve(p, &arrows.get_untracked()));
        ply.set(0);
        epoch.update(|e| *e += 1);
        solve.set(Some(verdict));
    };

    // The reveal. Watches `solve` and walks the replay forward, one timer per ply.
    //
    // `ply.get()` is deliberately **tracked**: advancing it re-runs this effect,
    // which arms the next ply's timer. That self-retriggering is the whole clock —
    // read it untracked and the effect fires once, the board takes a single step
    // and freezes there, still captioned "mate". Which is exactly what it did.
    //
    // The first ply waits longer than the rest: the board has just gone from void
    // to pieces, and moving something immediately steps on the moment the user
    // solved the puzzle for.
    Effect::new(move |_| {
        let Some(session::Solve::Solved(steps)) = solve.get() else {
            return;
        };
        let at = ply.get();
        if at >= steps.len() {
            return;
        }
        let delay = if at == 0 {
            constants::REVEAL_MS
        } else {
            constants::PLAYBACK_MS
        };
        let mine = epoch.get_untracked();
        set_timeout(
            move || {
                // Two different guards, and both are load-bearing.
                //
                // `epoch` is identity: a timer outlives the attempt that armed
                // it, so one still in flight when the user hits "Next" must not
                // step the next reveal forward. An earlier version compared plies
                // for this and its comment claimed it was checking ownership — it
                // was not, and a timer armed at ply 0 sailed through the check
                // because `reset` puts ply back to 0 too.
                //
                // `ply` is idempotence: the effect can re-run for a ply it has
                // already armed a timer for, and two timers each incrementing
                // would skip a move of the replay.
                if epoch.get_untracked() == mine && ply.get_untracked() == at {
                    ply.update(|n| *n += 1);
                }
            },
            std::time::Duration::from_millis(delay),
        );
    });

    // The position the board draws: `None` while the user is still blind, then
    // the start position, then each ply of the replay.
    let revealed = Signal::derive(move || match solve.get() {
        Some(session::Solve::Solved(steps)) => Some(
            session::step_at(&steps, ply.get())
                .map_or_else(|| position.get(), |step| step.after.clone()),
        ),
        _ => None,
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
    let highlight = Signal::derive(move || match solve.get() {
        Some(session::Solve::Solved(steps)) => session::step_at(&steps, ply.get())
            .and_then(|step| arrow::Arrow::of_move(&step.played))
            .map(|drag| drag.to),
        _ => None,
    });

    let locked = Signal::derive(move || matches!(solve.get(), Some(session::Solve::Solved(_))));

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

            <Tiers session=session reset=Callback::new(move |_| reset()) />

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
                                arrows=arrows
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
                                arrows=arrows
                                solver=solver.get()
                                depth=p.depth
                                solve=solve
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
                    " (CC0). Pieces by Colin M.L. Burnett (GPL). "
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
