//! The root component: holds the state, wires the panels to the board.

use crate::board;
use crate::database;
use crate::line;
use crate::panel;
use crate::rating;
use crate::session;
use crate::settings;
use crate::speech;
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

    // Which side faces the user, persisted across reloads. The per-puzzle flip that
    // layers on top of it lives in the `attempt` below, transient by design.
    let pov = RwSignal::new(settings::load_pov());

    // Whether the puzzle and verdict are read aloud, persisted. Off by default: audio
    // needs a user gesture to start (the enabling click), and a page that talks on
    // load would be a surprise. The announcement effects below read this.
    let sound = RwSignal::new(settings::load_sound());
    // Start the browser loading its voice list now, so a good voice is chosen from the
    // very first announcement rather than after it (some browsers load voices async).
    speech::warm();

    // The whole attempt in one signal, so its reset invariant lives in one place a
    // native test can reach — see `session::Attempt`.
    let attempt = RwSignal::new(session::Attempt::new());

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
    // Revealed (solve or give-up) locks the board and swaps the panel to the
    // solution. `ply` is the reveal cursor the move list highlights.
    let locked = Memo::new(move |_| attempt.with(session::Attempt::is_revealed));
    let ply = Memo::new(move |_| attempt.with(session::Attempt::ply));
    // The reveal as a move list. Depends on `solve` and the puzzle, not on `ply`, so
    // it is built once per reveal and not rebuilt on every step. `None` until the
    // board is revealed.
    let movelist = Memo::new(move |_| {
        solve.with(|s| {
            s.as_ref()
                .and_then(session::Solve::steps)
                .map(|steps| position.with(|p| session::movelist(p, steps)))
        })
    });
    let can_back = Memo::new(move |_| attempt.with(session::Attempt::can_step_back));
    let can_forward = Memo::new(move |_| attempt.with(session::Attempt::can_step_forward));
    // A `Memo` so the board rebuilds when the flip toggles but *not* on every arrow
    // draw — reading `attempt` directly in the board's render would resubscribe it to
    // the whole attempt and rebuild the board (losing an in-progress drag) each edit.
    let flipped = Memo::new(move |_| attempt.with(session::Attempt::flipped));

    let next = move |_| {
        let r = js_sys::Math::random();
        let rating_now = elo.get_untracked();
        session.update(|s| s.advance(rating_now, r));
        attempt.update(session::Attempt::reset);
        elo_delta.set(None);
    };

    // Apply a scoring outcome to the rating — shared by submit and give-up, since
    // both move the rating exactly the same way and `Attempt` already decides *whether*
    // a given event scores (returning `Some` only for the first one on the puzzle). A
    // `Callback` so both closures can hold it. `None` is the common no-score case.
    let score = Callback::new(move |outcome: Option<rating::Outcome>| {
        let Some(outcome) = outcome else {
            return;
        };
        let before = elo.get_untracked();
        let puzzle_rating = puzzle.with_untracked(|p| p.rating);
        let after = rating::update(before, puzzle_rating, outcome);
        elo.set(after);
        // No badge when the rating did not move — reachable only at the floor or
        // ceiling, where a win/loss clamps back to where it was, and a "+0" up-badge
        // there would read as a gain that did not happen. Both are <= ELO_CEILING
        // (3000), so the i32 cast is exact.
        elo_delta.set((after != before).then(|| after as i32 - before as i32));
        rating::save(after);
    });

    let submit = move |_| {
        let line = attempt.with_untracked(|a| a.arrows().to_vec());
        let verdict = puzzle.with_untracked(|p| session::solve(p, &line));
        // `submit` returns the outcome only for the first scoring submission on the
        // puzzle, so a miss-then-solve or a re-solve does not move the rating twice.
        let outcome = attempt.try_update(|a| a.submit(verdict)).flatten();
        score.run(outcome);
    };

    // Give up: reveal the puzzle's *own* stored solution (there is no winning line of
    // the user's to play out) and score it as a loss, once. `give_up` returns the
    // outcome under the same first-event-only rule `submit` uses.
    let give_up = move |()| {
        let steps = puzzle.with_untracked(session::reveal);
        let outcome = attempt.try_update(|a| a.give_up(steps)).flatten();
        score.run(outcome);
    };

    let draw = move |arrow: arrow::Arrow| attempt.update(|a| a.draw(arrow));
    let flip = move |()| attempt.update(session::Attempt::flip);
    // Set and persist together, lifted here rather than in the menu so the settings
    // component stays markup-and-plumbing — the same lifted-callback shape as `submit`.
    // Guarded against a no-op re-click of the active POV: `pov` feeds the board's
    // render directly (not through a diff-suppressing memo like `flipped`), so an
    // unconditional `set` would rebuild the board and drop an in-progress drag even
    // when nothing changed.
    let choose_pov = move |chosen: settings::Pov| {
        if pov.get_untracked() != chosen {
            pov.set(chosen);
            settings::save_pov(chosen);
        }
    };
    // Toggle read-aloud, persist it, and act at once: on enabling, read the current
    // puzzle (the enabling click is the user gesture browsers require before speech);
    // on muting, stop any speech mid-sentence rather than letting it finish.
    let toggle_sound = move |()| {
        let on = !sound.get_untracked();
        sound.set(on);
        settings::save_sound(on);
        if on {
            speech::say(&position.with_untracked(|p| roster::of(p).speech()));
        } else {
            speech::silence();
        }
    };
    let undo = move |()| attempt.update(session::Attempt::undo);
    let clear = move |()| attempt.update(session::Attempt::clear);
    let promote = move |(index, role): (usize, Option<shakmaty::Role>)| {
        attempt.update(|a| a.set_promotion(index, role))
    };
    let step_back = move |()| attempt.update(session::Attempt::step_back);
    let step_forward = move |()| attempt.update(session::Attempt::step_forward);
    let step_to = move |ply: usize| attempt.update(|a| a.step_to(ply));

    // Arrow keys walk the reveal, like a Lichess analysis board — only while revealed,
    // so they never fight anything during solving. `prevent_default` stops the page
    // scrolling out from under the board on the same key. The listener is on the
    // window (there is nothing focusable to hang it on) and removed on cleanup.
    let keydown = window_event_listener(leptos::ev::keydown, move |ev| {
        if !attempt.with_untracked(session::Attempt::is_revealed) {
            return;
        }
        match ev.key().as_str() {
            "ArrowLeft" => {
                ev.prevent_default();
                attempt.update(session::Attempt::step_back);
            }
            "ArrowRight" => {
                ev.prevent_default();
                attempt.update(session::Attempt::step_forward);
            }
            _ => {}
        }
    });
    on_cleanup(move || keydown.remove());

    // Read the puzzle aloud when a new one is served, if sound is on. Subscribes to
    // `position` (so it fires on load and on every `next`) but reads `sound`
    // untracked — toggling sound must not re-read the roster through here; that is the
    // toggle's own job, which also owns the enabling gesture. On the very first run
    // with sound persisted-on, the browser may refuse for lack of a gesture; that
    // degrades to silence until the first interaction, which is acceptable.
    Effect::new(move |_| {
        let spoken = position.with(|p| roster::of(p).speech());
        if sound.get_untracked() {
            speech::say(&spoken);
        }
    });

    // Speak the verdict the moment a submission or give-up produces one. Subscribes to
    // `solve` only: it fires when the verdict appears, not when the reveal is stepped
    // (that changes `ply`, not `solve`) and not when sound is toggled (read untracked,
    // so enabling sound after solving does not replay the verdict).
    Effect::new(move |_| {
        if let Some(verdict) = solve.get() {
            if sound.get_untracked() {
                speech::say(&session::spoken(&verdict, solver.get_untracked()));
            }
        }
    });

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

            // One header row: the rating and pool size on the left, the board's view
            // controls (flip, settings) on the right — the toolbar for everything that
            // is not the board itself. Putting the view controls here rather than in a
            // bar above the board frees that whole row for the board on a phone.
            <div class="topbar">
                <RatingBar
                    rating=elo
                    delta=elo_delta
                    total=Signal::derive(move || session.with(session::Session::total))
                />
                <BoardBar
                    pov=pov
                    flipped=flipped
                    sound=sound
                    on_flip=Callback::new(flip)
                    on_choose_pov=Callback::new(choose_pov)
                    on_toggle_sound=Callback::new(toggle_sound)
                />
            </div>

            <div class="layout">
                <div class="layout__board">
                    // A frame around the board so it can be a size container on a
                    // phone: the board then fills the smaller of the frame's width and
                    // height, shrinking to fit the space the fixed-height mobile shell
                    // leaves it rather than forcing a scroll. On desktop the frame is
                    // `display: contents` — a passthrough that changes nothing.
                    <div class="board-frame">
                        {move || {
                            // Rebuilt when the puzzle, the POV, or the flip changes — a
                            // fresh board without the last render's in-progress drag. The
                            // `puzzle.track()` is load-bearing beyond the orientation read:
                            // two puzzles can resolve to the *same* orientation, and
                            // without it the board would carry the previous drag into the
                            // next puzzle. `track`, not `get`: subscribe without cloning.
                            puzzle.track();
                            let side = settings::facing(pov.get(), solver.get(), flipped.get());
                            view! {
                                <board::Board
                                    orientation=square::Orientation(side)
                                    drawn=drawn
                                    on_draw=Callback::new(draw)
                                    revealed=revealed
                                    highlight=highlight
                                    locked=locked
                                />
                            }
                        }}
                    </div>
                    <Facts session=session />
                </div>

                <aside class="layout__panels">
                    {move || {
                        // Keyed on the puzzle, like the board above, so the roster
                        // and line rebuild fresh when a new one is served.
                        puzzle.track();
                        let r = Signal::derive(move || roster::of(&position.get()));
                        // Wrapped so each is an independent layout region: on a phone
                        // the roster is hoisted above the board and pinned there (see
                        // `.layout__roster` in styles.css), the line stays below.
                        view! {
                            <div class="layout__roster">
                                <panel::Roster roster=r />
                            </div>
                            <div class="layout__line">
                            <line::Line
                                drawn=drawn
                                solver=solver.get()
                                solve=solve
                                movelist=movelist
                                ply=ply
                                can_back=can_back
                                can_forward=can_forward
                                on_undo=Callback::new(undo)
                                on_clear=Callback::new(clear)
                                on_submit=Callback::new(submit)
                                on_give_up=Callback::new(give_up)
                                on_promote=Callback::new(promote)
                                on_next=Callback::new(next)
                                on_step_back=Callback::new(step_back)
                                on_step_forward=Callback::new(step_forward)
                                on_step_to=Callback::new(step_to)
                            />
                            </div>
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

/// The puzzle's id, small and muted under the board.
///
/// Kept only so a user who hits something odd can report *which* puzzle, and look
/// it up in the committed database. Everything else that used to sit here is gone
/// on purpose: the rating is a difficulty hint, the square count read as clutter,
/// and the mate depth is the very thing the trainer withholds.
#[component]
fn Facts(session: RwSignal<session::Session>) -> impl IntoView {
    view! {
        <p class="facts mono">{move || format!("#{}", session.with(|s| s.current().id.clone()))}</p>
    }
}

/// The board's view controls: flip, and the settings menu.
///
/// Both change *how* the board is drawn, not what the puzzle is, so they sit in the
/// header row (`.topbar`) with the rating — the toolbar for everything that is not the
/// board — rather than in a bar above the board, which on a phone is a row of height
/// the board would rather have. The flip is a transient per-puzzle toggle (its
/// `pressed` state tracks the attempt); the settings menu holds the persisted point of
/// view.
#[component]
fn BoardBar(
    #[prop(into)] pov: Signal<settings::Pov>,
    #[prop(into)] flipped: Signal<bool>,
    #[prop(into)] sound: Signal<bool>,
    #[prop(into)] on_flip: Callback<()>,
    #[prop(into)] on_choose_pov: Callback<settings::Pov>,
    #[prop(into)] on_toggle_sound: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="boardbar">
            // Read-aloud toggle, first because it is the entry to the voice mode — one
            // tap to have the puzzle (and later the verdict) spoken.
            <button
                class="button button--icon"
                class:button--on=move || sound.get()
                aria-pressed=move || sound.get().to_string()
                aria-label="Read aloud"
                title=move || if sound.get() { "Reading aloud — tap to mute" } else { "Read the puzzle aloud" }
                on:click=move |_| on_toggle_sound.run(())
            >
                {move || if sound.get() { "🔊" } else { "🔈" }}
            </button>
            <button
                class="button button--icon"
                class:button--on=move || flipped.get()
                aria-pressed=move || flipped.get().to_string()
                aria-label="Flip board"
                title="Flip board"
                on:click=move |_| on_flip.run(())
            >
                "⇅"
            </button>
            <Settings pov=pov on_choose=on_choose_pov />
        </div>
    }
}

/// The settings menu: a gear that opens a small panel of preferences.
///
/// One setting for now — the point of view — but it is its own component and backed
/// by its own [`settings`] module because it is the seam more settings hang off, not
/// a lone control. Toggled open by the gear; the board updates live behind it as the
/// choice changes, so there is nothing to confirm and no need to close on select.
#[component]
fn Settings(
    #[prop(into)] pov: Signal<settings::Pov>,
    #[prop(into)] on_choose: Callback<settings::Pov>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <div class="settings">
            <button
                class="button button--icon"
                aria-haspopup="menu"
                aria-expanded=move || open.get().to_string()
                aria-label="Settings"
                title="Settings"
                on:click=move |_| open.update(|o| *o = !*o)
            >
                "⚙"
            </button>
            {move || {
                open.get()
                    .then(|| {
                        view! {
                            <div class="settings__menu" role="menu" aria-label="Settings">
                                <p class="settings__heading">"Point of view"</p>
                                {settings::Pov::ALL
                                    .into_iter()
                                    .map(|option| {
                                        let on = Signal::derive(move || pov.get() == option);
                                        view! {
                                            <button
                                                class="settings__option"
                                                class:settings__option--on=move || on.get()
                                                role="menuitemradio"
                                                aria-checked=move || on.get().to_string()
                                                on:click=move |_| on_choose.run(option)
                                            >
                                                {option.label()}
                                            </button>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }
                    })
            }}
        </div>
    }
}
