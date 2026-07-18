//! The line the user has drawn, and the verdict on it.
//!
//! One arrow per move of the user's own side, in order — the interface's central
//! constraint. It cannot express a branch, which is why the database is filtered
//! to puzzles whose answer works against every defense.

use crate::pieces;
use crate::session;
use blindfold_core::arrow;
use blindfold_core::mate;
use blindfold_core::roster;
use leptos::prelude::*;

/// The drawn line, the submit controls, and — once solved — the controls that step
/// through the reveal.
///
/// Reads the line through `drawn` and reports edits through callbacks rather than
/// mutating a shared vector, so the whole attempt lives behind one
/// [`crate::session::Attempt`] value instead of a signal each component can poke.
#[component]
pub fn Line(
    #[prop(into)] drawn: Signal<Vec<arrow::Arrow>>,
    solver: shakmaty::Color,
    #[prop(into)] solve: Signal<Option<session::Solve>>,
    /// Whether the reveal can step toward the start / toward the mate — for
    /// disabling the two navigation controls at the ends of the line.
    #[prop(into)]
    can_back: Signal<bool>,
    #[prop(into)] can_forward: Signal<bool>,
    /// Whether a promotion choice is open on the board. The submit controls are
    /// inert while it is, so an unresolved move cannot be judged.
    #[prop(into)]
    choosing: Signal<bool>,
    #[prop(into)] on_undo: Callback<()>,
    #[prop(into)] on_clear: Callback<()>,
    #[prop(into)] on_submit: Callback<()>,
    #[prop(into)] on_next: Callback<()>,
    #[prop(into)] on_step_back: Callback<()>,
    #[prop(into)] on_step_forward: Callback<()>,
) -> impl IntoView {
    let solved =
        Signal::derive(move || solve.with(|s| matches!(s, Some(session::Solve::Solved(_)))));

    // The line-editing controls (Submit/Undo/Clear) are inert together: with no
    // line there is nothing to act on, and while a promotion choice is open the
    // move behind it is unresolved and must not be judged. One predicate so the
    // three buttons cannot drift apart when that rule changes.
    let editing_disabled = Signal::derive(move || drawn.get().is_empty() || choosing.get());

    view! {
        <section class="panel" aria-label="Your line">
            <h2 class="panel__title">"Your line"</h2>

            <ol class="line">
                {move || {
                    let line = drawn.get();
                    if line.is_empty() {
                        return view! {
                            <li class="line__empty">"Drag from one square to another."</li>
                        }
                            .into_any();
                    }
                    line.into_iter()
                        .enumerate()
                        .map(|(i, a)| {
                            view! { <Step index=i entry=a solver=solver /> }
                        })
                        .collect_view()
                        .into_any()
                }}
            </ol>

            <div class="controls">
                {move || {
                    if solved.get() {
                        // Solved: walk the mating line back and forth, Lichess
                        // analysis style, or move on.
                        view! {
                            <div class="stepper" role="group" aria-label="Step through the mate">
                                <button
                                    class="button button--step"
                                    aria-label="Step back"
                                    disabled=move || !can_back.get()
                                    on:click=move |_| on_step_back.run(())
                                >
                                    "◀"
                                </button>
                                <button
                                    class="button button--step"
                                    aria-label="Step forward"
                                    disabled=move || !can_forward.get()
                                    on:click=move |_| on_step_forward.run(())
                                >
                                    "▶"
                                </button>
                            </div>
                            <button
                                class="button button--primary"
                                on:click=move |_| on_next.run(())
                            >
                                "Next puzzle"
                            </button>
                        }
                            .into_any()
                    } else {
                        view! {
                            <button
                                class="button button--primary"
                                disabled=move || editing_disabled.get()
                                on:click=move |_| on_submit.run(())
                            >
                                "Submit"
                            </button>
                            <button
                                class="button"
                                disabled=move || editing_disabled.get()
                                on:click=move |_| on_undo.run(())
                            >
                                "Undo"
                            </button>
                            <button
                                class="button"
                                disabled=move || editing_disabled.get()
                                on:click=move |_| on_clear.run(())
                            >
                                "Clear"
                            </button>
                        }
                            .into_any()
                    }
                }}
            </div>

            <Verdict solve=solve solver=solver />
        </section>
    }
}

/// One numbered arrow, showing its promotion piece if it has one.
///
/// The picker itself lives on the board now, popped up at the destination the
/// moment a promotion-candidate drag lands ([`crate::board::Board`]). Here the
/// chosen piece is only shown, not chosen — so the list reads back the whole line
/// including "…and it promotes to a queen".
#[component]
fn Step(index: usize, entry: arrow::Arrow, solver: shakmaty::Color) -> impl IntoView {
    view! {
        <li class="line__step">
            <span class="line__number">{index + 1}</span>
            <span class="line__move mono">
                {entry.from.to_string()} <span class="line__arrow">"→"</span> {entry.to.to_string()}
            </span>
            {entry
                .promotion
                .map(|role| {
                    view! {
                        <span
                            class="line__promotion"
                            role="img"
                            aria-label=format!("promotes to {}", roster::name(role, false))
                            title=roster::name(role, false)
                            inner_html=pieces::svg(solver, role)
                        />
                    }
                })}
        </li>
    }
}

/// What came back from the judge.
#[component]
fn Verdict(
    #[prop(into)] solve: Signal<Option<session::Solve>>,
    solver: shakmaty::Color,
) -> impl IntoView {
    view! {
        <p class="verdict" aria-live="polite">
            {move || {
                match solve.get() {
                    None => ().into_any(),
                    Some(session::Solve::Solved(_)) => {
                        view! {
                            <span class="verdict--ok">
                                "Mate. Here is the position you were holding."
                            </span>
                        }
                            .into_any()
                    }
                    Some(session::Solve::Refuted { defense, reason }) => {
                        view! {
                            <span class="verdict--no">
                                {session::explain(&reason, solver)}
                                {(!defense.is_empty())
                                    .then(|| {
                                        let moves = defense
                                            .iter()
                                            .map(arrow::Arrow::to_string)
                                            .collect::<Vec<_>>()
                                            .join(" ");
                                        view! {
                                            <span class="verdict__defense">
                                                "The defense that holds: "
                                                <span class="mono">{moves}</span>
                                            </span>
                                        }
                                    })}
                            </span>
                        }
                            .into_any()
                    }
                    // Never phrased as a wrong answer, because it is not one — we
                    // declined to judge. No database puzzle can reach this; it
                    // exists so a pathological line fails honestly.
                    Some(session::Solve::Unjudged(limit)) => {
                        view! {
                            <span class="verdict--hmm">
                                {match limit {
                                    mate::Limit::Length { moves } => {
                                        format!(
                                            "That is {moves} moves; this trainer judges up to {}.",
                                            blindfold_core::constants::MAX_LINE,
                                        )
                                    }
                                    mate::Limit::Frontier { branches } => {
                                        format!(
                                            "That line branches {branches} ways — too far to check. \
                                             Not a verdict on whether it works.",
                                        )
                                    }
                                }}
                            </span>
                        }
                            .into_any()
                    }
                }
            }}
        </p>
    }
}
