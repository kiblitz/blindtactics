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

/// The drawn line, its promotion choices, and the submit controls.
#[component]
pub fn Line(
    arrows: RwSignal<Vec<arrow::Arrow>>,
    solver: shakmaty::Color,
    depth: usize,
    #[prop(into)] solve: Signal<Option<session::Solve>>,
    #[prop(into)] on_submit: Callback<()>,
    #[prop(into)] on_next: Callback<()>,
) -> impl IntoView {
    let solved = Signal::derive(move || matches!(solve.get(), Some(session::Solve::Solved(_))));

    view! {
        <section class="panel" aria-label="Your line">
            <h2 class="panel__title">"Your line"</h2>

            <ol class="line">
                {move || {
                    let drawn = arrows.get();
                    if drawn.is_empty() {
                        return view! {
                            <li class="line__empty">"Drag from one square to another."</li>
                        }
                            .into_any();
                    }
                    drawn
                        .into_iter()
                        .enumerate()
                        .map(|(i, a)| {
                            view! {
                                <Step index=i entry=a arrows=arrows solver=solver locked=solved />
                            }
                        })
                        .collect_view()
                        .into_any()
                }}
            </ol>

            <div class="controls">
                {move || {
                    if solved.get() {
                        view! {
                            <button class="button button--primary" on:click=move |_| on_next.run(())>
                                "Next puzzle"
                            </button>
                        }
                            .into_any()
                    } else {
                        view! {
                            <button
                                class="button button--primary"
                                disabled=move || arrows.get().is_empty()
                                on:click=move |_| on_submit.run(())
                            >
                                "Submit"
                            </button>
                            <button
                                class="button"
                                disabled=move || arrows.get().is_empty()
                                on:click=move |_| arrows.update(|l| { l.pop(); })
                            >
                                "Undo"
                            </button>
                            <button
                                class="button"
                                disabled=move || arrows.get().is_empty()
                                on:click=move |_| arrows.set(Vec::new())
                            >
                                "Clear"
                            </button>
                        }
                            .into_any()
                    }
                }}
            </div>

            <Verdict solve=solve depth=depth solver=solver />
        </section>
    }
}

/// One numbered arrow, with a promotion picker where the drag *could* be a pawn
/// promoting.
///
/// The picker's condition is [`arrow::Arrow::could_be_promotion`] — a pawn
/// stepping off the rank below onto the last one, straight or one file sideways —
/// not merely "lands on the last rank", which would put a picker beside both moves
/// of a rook mate-in-2. It is a necessary condition read off the drag alone.
/// Promotion is mandatory rather than optional, so there is no "promote, or don't"
/// to ask: only "to what". Whether the piece on `from` really is a pawn is
/// something the user knows from the roster and the app deliberately does not tell
/// them, so the choice is left unset rather than guessed at.
#[component]
fn Step(
    index: usize,
    entry: arrow::Arrow,
    arrows: RwSignal<Vec<arrow::Arrow>>,
    solver: shakmaty::Color,
    #[prop(into)] locked: Signal<bool>,
) -> impl IntoView {
    let promotes = entry.could_be_promotion(solver);

    view! {
        <li class="line__step">
            <span class="line__number">{index + 1}</span>
            <span class="line__move mono">
                {entry.from.to_string()} <span class="line__arrow">"→"</span> {entry.to.to_string()}
            </span>
            {promotes
                .then(|| {
                    view! {
                        <span class="promotion" role="group" aria-label="Promote to">
                            {blindfold_core::constants::PROMOTABLE
                                .into_iter()
                                .map(|role| {
                                    view! {
                                        <button
                                            class="promotion__choice"
                                            class:promotion__choice--on=move || {
                                                arrows
                                                    .get()
                                                    .get(index)
                                                    .and_then(|a| a.promotion)
                                                    == Some(role)
                                            }
                                            disabled=locked
                                            aria-label=roster::name(role, false)
                                            title=roster::name(role, false)
                                            on:click=move |_| {
                                                arrows
                                                    .update(|line| {
                                                        if let Some(a) = line.get_mut(index) {
                                                            // Tapping the chosen role again clears it, so a
                                                            // misread of the roster is one tap to undo rather
                                                            // than a cleared line.
                                                            a.promotion = (a.promotion != Some(role))
                                                                .then_some(role);
                                                        }
                                                    })
                                            }
                                        >
                                            <span inner_html=pieces::svg(solver, role) />
                                        </button>
                                    }
                                })
                                .collect_view()}
                        </span>
                    }
                })}
        </li>
    }
}

/// What came back from the judge.
#[component]
fn Verdict(
    #[prop(into)] solve: Signal<Option<session::Solve>>,
    depth: usize,
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
                                {session::explain(&reason, depth, solver)}
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

/// A hint about what to draw, under the board.
///
/// Just the count and the "your moves only" rule. The file/rank labels live in
/// [`crate::board::Coordinates`], rendered into the edge squares so they cannot
/// drift out of alignment with the grid — this is not where coordinates are.
#[component]
pub fn Hint(depth: usize) -> impl IntoView {
    view! {
        <p class="hint">
            {format!(
                "Draw {depth} arrow{} — your moves only, in order.",
                if depth == 1 { "" } else { "s" },
            )}
        </p>
    }
}
