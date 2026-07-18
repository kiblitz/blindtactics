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

/// The drawn line, the submit controls, and — once the board is revealed — the
/// controls that step through the reveal.
///
/// Reads the line through `drawn` and reports edits through callbacks rather than
/// mutating a shared vector, so the whole attempt lives behind one
/// [`crate::session::Attempt`] value instead of a signal each component can poke.
#[component]
pub fn Line(
    #[prop(into)] drawn: Signal<Vec<arrow::Arrow>>,
    solver: shakmaty::Color,
    #[prop(into)] solve: Signal<Option<session::Solve>>,
    /// The reveal as a move list, once the board is revealed — the plies to walk,
    /// grouped into numbered rows. `None` until the board is revealed.
    #[prop(into)]
    movelist: Signal<Option<Vec<session::Row>>>,
    /// Which ply the reveal is showing, for highlighting the move list's current
    /// move. Meaningful only once revealed.
    #[prop(into)]
    ply: Signal<usize>,
    /// Whether the reveal can step toward the start / toward the mate — for
    /// disabling the two navigation controls at the ends of the line.
    #[prop(into)]
    can_back: Signal<bool>,
    #[prop(into)] can_forward: Signal<bool>,
    #[prop(into)] on_undo: Callback<()>,
    #[prop(into)] on_clear: Callback<()>,
    #[prop(into)] on_submit: Callback<()>,
    #[prop(into)] on_give_up: Callback<()>,
    /// Set (or clear) the promotion piece on the arrow at the given index — driven
    /// by the per-move control shown on a last-rank move.
    #[prop(into)]
    on_promote: Callback<(usize, Option<shakmaty::Role>)>,
    #[prop(into)] on_next: Callback<()>,
    #[prop(into)] on_step_back: Callback<()>,
    #[prop(into)] on_step_forward: Callback<()>,
    /// Jump the reveal straight to a ply — a move-list entry was clicked.
    #[prop(into)]
    on_step_to: Callback<usize>,
) -> impl IntoView {
    // Revealed once the board is shown — a solve *or* a give-up. Both replace the
    // "Your line" editing view with the solution's move list and the stepper. Read
    // from `solve`, the panel's one source of state, rather than from the presence of
    // the `movelist` data prop: the two are kept in lockstep by `app`, but the truth
    // is "the attempt is revealed", not "a list happens to exist".
    let revealed = Signal::derive(move || {
        solve.with(|s| s.as_ref().and_then(session::Solve::steps).is_some())
    });

    // The line-editing controls (Submit/Undo/Clear) are inert together when there is
    // no line to act on. One predicate so the three buttons cannot drift apart. Give
    // up is *not* among them: being stuck with nothing drawn is exactly when it is
    // wanted, so it stays enabled on an empty line.
    let editing_disabled = Signal::derive(move || drawn.get().is_empty());

    view! {
        <section class="panel" aria-label="Your line">
            <h2 class="panel__title">
                {move || if revealed.get() { "Solution" } else { "Your line" }}
            </h2>

            {move || {
                if revealed.get() {
                    // The solution as a Lichess-style move list, clickable to jump.
                    let rows = Signal::derive(move || movelist.get().unwrap_or_default());
                    view! { <Movelist rows=rows current=ply on_step_to=on_step_to /> }.into_any()
                } else {
                    view! {
                        <ol class="line">
                            {move || {
                                let line = drawn.get();
                                if line.is_empty() {
                                    return view! {
                                        <li class="line__empty">
                                            "Drag from one square to another."
                                        </li>
                                    }
                                        .into_any();
                                }
                                line.into_iter()
                                    .enumerate()
                                    .map(|(i, a)| {
                                        view! {
                                            <Step
                                                index=i
                                                entry=a
                                                solver=solver
                                                on_promote=on_promote
                                            />
                                        }
                                    })
                                    .collect_view()
                                    .into_any()
                            }}
                        </ol>
                    }
                        .into_any()
                }
            }}

            <div class="controls">
                {move || {
                    if revealed.get() {
                        // Revealed: walk the mating line back and forth, Lichess
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
                            <button
                                class="button button--give-up"
                                on:click=move |_| on_give_up.run(())
                            >
                                "Give up"
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

/// The reveal's move list: SAN for every ply, in numbered rows, each clickable to
/// jump the board to that move. The move the board is currently showing is
/// highlighted.
///
/// Only shown once revealed, so it says nothing the board does not already: by the
/// time it renders, the mate is on screen. The `<ol>` rebuilds only when `rows`
/// changes (a new reveal); stepping the cursor changes only which cell is
/// highlighted, via each cell's own read of `current`.
#[component]
fn Movelist(
    #[prop(into)] rows: Signal<Vec<session::Row>>,
    #[prop(into)] current: Signal<usize>,
    #[prop(into)] on_step_to: Callback<usize>,
) -> impl IntoView {
    view! {
        <ol class="movelist" aria-label="Solution moves">
            {move || {
                rows.get()
                    .into_iter()
                    .map(|row| {
                        view! {
                            <li class="movelist__row">
                                <span class="movelist__number">{row.number} "."</span>
                                <MoveCell ply=row.white current=current on_step_to=on_step_to />
                                <MoveCell ply=row.black current=current on_step_to=on_step_to />
                            </li>
                        }
                    })
                    .collect_view()
            }}
        </ol>
    }
}

/// One side's move in a [`Movelist`] row: a button that jumps the reveal to it, or an
/// empty placeholder when that side did not move in this row (a line that opens on
/// Black, or ends on White).
#[component]
fn MoveCell(
    ply: Option<session::Ply>,
    #[prop(into)] current: Signal<usize>,
    #[prop(into)] on_step_to: Callback<usize>,
) -> impl IntoView {
    match ply {
        None => view! { <span class="movelist__cell movelist__cell--empty"></span> }.into_any(),
        Some(ply) => {
            let at = ply.at;
            view! {
                <button
                    class="movelist__cell movelist__ply mono"
                    class:movelist__ply--on=move || current.get() == at
                    aria-current=move || (current.get() == at).then_some("true")
                    on:click=move |_| on_step_to.run(at)
                >
                    {ply.san}
                </button>
            }
            .into_any()
        }
    }
}

/// One numbered arrow, with a promotion control when the move could be a pawn
/// promoting.
///
/// The control is shown only for a last-rank drag ([`arrow::Arrow::could_be_promotion`]
/// is a necessary precondition, read off the drag alone), and it defaults to "no
/// promotion" so a rook lift to the last rank is just a plain move — no modal
/// interrupts the line. A real pawn promotion is set here; if left as "no promotion"
/// the illegal move is caught on submit with a hint to pick a piece.
#[component]
fn Step(
    index: usize,
    entry: arrow::Arrow,
    solver: shakmaty::Color,
    #[prop(into)] on_promote: Callback<(usize, Option<shakmaty::Role>)>,
) -> impl IntoView {
    view! {
        <li class="line__step">
            <span class="line__number">{index + 1}</span>
            <span class="line__move mono">
                {entry.from.to_string()} <span class="line__arrow">"→"</span> {entry.to.to_string()}
            </span>
            {entry
                .could_be_promotion(solver)
                .then(|| {
                    view! {
                        <Promote
                            index=index
                            chosen=entry.promotion
                            solver=solver
                            on_promote=on_promote
                        />
                    }
                })}
        </li>
    }
}

/// The per-move promotion control: a "no promotion" option and the four promotable
/// pieces, the current choice pressed.
///
/// `chosen` is a plain value, not a signal: changing it edits the drawn line, which
/// re-renders the whole [`Step`] with the new value, so a static read per render is
/// exactly right.
#[component]
fn Promote(
    index: usize,
    chosen: Option<shakmaty::Role>,
    solver: shakmaty::Color,
    #[prop(into)] on_promote: Callback<(usize, Option<shakmaty::Role>)>,
) -> impl IntoView {
    view! {
        <span class="line__promote" role="group" aria-label="Promotion piece">
            <button
                class="line__promote-choice"
                class:line__promote-choice--on=chosen.is_none()
                aria-label="No promotion"
                aria-pressed=chosen.is_none().to_string()
                title="No promotion"
                on:click=move |_| on_promote.run((index, None))
            >
                "—"
            </button>
            // Spelled in full rather than imported: `constants::` is bound to the
            // *web* crate's constants across the sibling component modules, so a bare
            // `constants::PROMOTABLE` would read as web policy. The full path names
            // the core module it actually comes from.
            {blindfold_core::constants::PROMOTABLE
                .into_iter()
                .map(|role| {
                    let on = chosen == Some(role);
                    view! {
                        <button
                            class="line__promote-choice"
                            class:line__promote-choice--on=on
                            aria-label=roster::name(role, false)
                            aria-pressed=on.to_string()
                            title=roster::name(role, false)
                            on:click=move |_| on_promote.run((index, Some(role)))
                        >
                            <span inner_html=pieces::svg(solver, role) />
                        </button>
                    }
                })
                .collect_view()}
        </span>
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
                    // Gave up: the solution is revealed and was scored as a loss (the
                    // delta badge shows it), so this is phrased as the concession it
                    // is — informational, not a "wrong answer" the user can act on.
                    Some(session::Solve::GaveUp(_)) => {
                        view! {
                            <span class="verdict--no">
                                "You gave up — here is the forced mate. Step through it below."
                            </span>
                        }
                            .into_any()
                    }
                    // A line that mates before its last arrow. Phrased as a miss
                    // (it is scored as one), but its own message — the mistake is
                    // "too many moves", not "the mate fails". The move count is
                    // withheld on purpose: naming it would leak the puzzle's depth.
                    Some(session::Solve::Overshot { .. }) => {
                        view! {
                            <span class="verdict--no">
                                "That line mates before its last arrow — some moves you drew are \
                                 never played. Trim the arrows after the mate."
                            </span>
                        }
                            .into_any()
                    }
                    // A last-rank move left with no promotion piece. Not a wrong
                    // answer — a fixable entry — so it is phrased as a neutral hint
                    // (`verdict--hmm`), not a refutation, and reuses `explain`'s
                    // promotion wording rather than restating it.
                    Some(session::Solve::Incomplete(a)) => {
                        view! {
                            <span class="verdict--hmm">
                                {session::explain(&mate::Reason::Illegal(a), solver)}
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
