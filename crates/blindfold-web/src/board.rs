//! The board: eight by eight of nothing, until it isn't.
//!
//! Rendering only. Every question with a right answer — which square is under the
//! pointer, where an arrow's head goes, whether a line mates — is answered by
//! [`crate::square`] or by `blindfold_core`, so what is left here is markup and
//! event plumbing.
//!
//! The board is layers in one `aspect-ratio: 1` box: the squares, an SVG overlay
//! for the arrows, the pieces, and — while the user is choosing a promotion — a
//! popup at the destination square. Only the squares (and the popup) take pointer
//! events; the arrows and pieces are `pointer-events: none` so a drag that crosses
//! an arrow it has already drawn does not get swallowed by it.

use crate::constants;
use crate::pieces;
use crate::square;
use blindfold_core::arrow;
use blindfold_core::roster;
use leptos::prelude::*;
use shakmaty::Position as _;

/// Where a drag started, if one is in progress.
///
/// An explicit `Option` rather than inferring a drag from the pointer being
/// down: the pointer can go down outside the board and arrive over it already
/// moving, and "no drag started here" and "a drag started on a1" are different
/// states that must not be told apart by a coordinate.
type Dragging = RwSignal<Option<shakmaty::Square>>;

/// An open promotion popup: the square it sits on, and the index of the
/// provisional arrow it is choosing a piece for.
///
/// Public so [`crate::app`], which owns the signal, constructs it through this
/// alias rather than re-spelling the tuple — the shape and the meaning of its two
/// fields live in one place.
pub type Promoting = RwSignal<Option<(shakmaty::Square, usize)>>;

/// The square under a pointer event, from the board element's own box.
///
/// Reads the element's rect per event rather than caching it, because the board
/// is fluid: it resizes with the window and the panel beside it, and a cached
/// rect would put arrows on the wrong squares after a resize — a bug that only
/// appears once someone drags the window edge.
///
/// # This depends on Leptos's `delegation` feature staying off
///
/// `current_target` is the board div only because the listener is on the board
/// div. Pointer events bubble, and with `tachys/delegation` enabled Leptos hangs
/// one listener on the *window* and dispatches from there — its own source
/// carries a `// TODO simulate currentTarget`. Under delegation this would return
/// the `Window`, the cast would fail, every event would resolve to `None`, and
/// **no arrow could be drawn at all** — with no panic and nothing in the console.
///
/// It is off today (`cargo tree -p blindfold-web -e features` shows `tachys` with
/// only `default`, `oco`, `reactive_graph`). Naming it because that is a
/// load-bearing dependency on a transitive crate's default, and the failure is
/// silent. `ev.target()` is not the alternative — pointer capture retargets it to
/// the board anyway; a `NodeRef` on the board div is.
fn square_of(
    ev: &leptos::ev::PointerEvent,
    orientation: square::Orientation,
) -> Option<shakmaty::Square> {
    let target = ev.current_target()?;
    let element: web_sys::Element = wasm_bindgen::JsCast::dyn_into(target).ok()?;
    let rect = element.get_bounding_client_rect();
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    let x = (f64::from(ev.client_x()) - rect.left()) / rect.width();
    let y = (f64::from(ev.client_y()) - rect.top()) / rect.height();
    square::of_fraction(x, y, orientation)
}

/// The blank board, the arrows drawn on it, and — once solved — the pieces.
///
/// `revealed` is the position to show, `None` while the user is still blind. It
/// is a position rather than a bool because the reveal steps through a line: each
/// ply hands this a new position, and the board just draws whatever it is given.
#[component]
pub fn Board(
    orientation: square::Orientation,
    /// The committed arrows to draw. Read-only: the board reports a new one
    /// through `on_draw` rather than pushing to a shared vector, so all attempt
    /// state lives behind [`crate::session::Attempt`].
    #[prop(into)]
    drawn: Signal<Vec<arrow::Arrow>>,
    /// Called with each arrow the user completes by dragging.
    #[prop(into)]
    on_draw: Callback<arrow::Arrow>,
    /// Called with `(arrow index, role)` when the user picks a promotion piece in
    /// the board popup.
    #[prop(into)]
    on_promote: Callback<(usize, shakmaty::Role)>,
    /// Called when the user dismisses the promotion popup by clicking away. The
    /// provisional arrow is removed — a click outside the picker cancels the move,
    /// the way it does in Lichess. Choosing "no promotion" *keeps* the plain move
    /// instead; that is a separate exit (see the picker's own button).
    #[prop(into)]
    on_cancel: Callback<()>,
    /// The open promotion popup, if any: the square it sits on and the index of the
    /// provisional arrow it is choosing for. Owned by the app so it can disable
    /// submission while a choice is pending — an unresolved move must not be judged.
    promoting: Promoting,
    #[prop(into)] revealed: Signal<Option<shakmaty::Chess>>,
    /// The square a move just landed on, lit so the eye can follow the replay.
    #[prop(into)]
    highlight: Signal<Option<shakmaty::Square>>,
    /// Whether the user may still draw. False once they have solved it.
    #[prop(into)]
    locked: Signal<bool>,
) -> impl IntoView {
    let dragging: Dragging = RwSignal::new(None);
    // Where the pointer is now, so the in-flight arrow follows it rather than
    // appearing only once released — and, when no drag is in progress, so the
    // square under the pointer can be highlighted.
    let hovering: RwSignal<Option<shakmaty::Square>> = RwSignal::new(None);

    let on_down = move |ev: leptos::ev::PointerEvent| {
        if locked.get_untracked() || promoting.get_untracked().is_some() {
            return;
        }
        // Capture, so a drag that leaves the board still delivers its `pointerup`
        // here. Without it, releasing outside leaves `dragging` set and the next
        // click anywhere draws an arrow from wherever the user last pressed.
        if let Some(target) = ev.current_target() {
            if let Ok(element) = wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(target) {
                let _ = element.set_pointer_capture(ev.pointer_id());
            }
        }
        let at = square_of(&ev, orientation);
        dragging.set(at);
        hovering.set(at);
    };

    let on_move = move |ev: leptos::ev::PointerEvent| {
        // Modal while a promotion is open, like `on_down`: the backdrop swallows
        // `pointerdown` but not `pointermove`, so without this the hover highlight
        // would still track the pointer under the covering backdrop.
        if locked.get_untracked() || promoting.get_untracked().is_some() {
            return;
        }
        // Tracked whether or not a drag is in progress: mid-drag it aims the arrow,
        // and otherwise it drives the hover highlight. Only write on a boundary
        // crossing — a pointermove within the same square would notify every
        // square's `class:` closure for no visible change.
        let now = square_of(&ev, orientation);
        if hovering.get_untracked() != now {
            hovering.set(now);
        }
    };

    let on_up = move |ev: leptos::ev::PointerEvent| {
        let from = dragging.get_untracked();
        dragging.set(None);
        hovering.set(None);
        let (Some(from), Some(to)) = (from, square_of(&ev, orientation)) else {
            return;
        };
        // A press and release on one square is a click, not a drag. Dropping it
        // silently is deliberate: it is how a user cancels a drag they have
        // thought better of, by returning to where they started.
        if from == to || locked.get_untracked() {
            return;
        }
        let candidate = arrow::Arrow::new(from, to);
        // Whether this drag could be a pawn promoting, read off the drag alone —
        // see `arrow::Arrow::could_be_promotion`. Computed before `on_draw` moves
        // the arrow away.
        let promotes = candidate.could_be_promotion(orientation.0);
        // `on_draw` is about to append this arrow, so it lands at the current
        // length. The board is modal while a promotion is open (`on_down` bails and
        // the backdrop swallows events), so nothing can slip in after it — the
        // provisional arrow stays at this index until it is picked or cancelled.
        let index = drawn.with_untracked(Vec::len);
        on_draw.run(candidate);
        if promotes {
            promoting.set(Some((to, index)));
        }
    };

    let squares = square::in_layout_order(orientation);

    view! {
        <div
            class="board"
            class:board--revealed=move || revealed.get().is_some()
            on:pointerdown=on_down
            on:pointermove=on_move
            on:pointerup=on_up
            on:pointercancel=move |_| { dragging.set(None); hovering.set(None); }
            on:pointerleave=move |_| { hovering.set(None); }
        >
            <div class="board__squares">
                {squares
                    .into_iter()
                    .map(|sq| {
                        let dark = sq.is_dark();
                        view! {
                            <div
                                class="square"
                                // Named so a test can drive the board the way a
                                // user does — by dragging between squares — instead
                                // of reimplementing the layout to find them, which
                                // would make the mapping agree with itself and
                                // prove nothing.
                                data-square=sq.to_string()
                                class:square--dark=dark
                                class:square--light=!dark
                                class:square--from=move || dragging.get() == Some(sq)
                                class:square--to=move || {
                                    dragging.get().is_some() && hovering.get() == Some(sq)
                                }
                                class:square--hover=move || {
                                    !locked.get()
                                        && dragging.get().is_none()
                                        && hovering.get() == Some(sq)
                                }
                                class:square--played=move || highlight.get() == Some(sq)
                            >
                                <Coordinates square=sq orientation=orientation />
                            </div>
                        }
                    })
                    .collect_view()}
            </div>

            <Arrows drawn=drawn dragging=dragging hovering=hovering orientation=orientation />

            <div class="board__pieces">
                {move || {
                    revealed
                        .get()
                        .map(|position| {
                            position
                                .board()
                                .clone()
                                .into_iter()
                                .map(|(sq, piece)| {
                                    let (col, row) = square::cell(sq, orientation);
                                    let per = constants::PERCENT_PER_SQUARE;
                                    view! {
                                        <div
                                            class="piece"
                                            style:left=format!("{}%", f64::from(col) * per)
                                            style:top=format!("{}%", f64::from(row) * per)
                                            inner_html=pieces::svg(piece.color, piece.role)
                                        />
                                    }
                                })
                                .collect_view()
                        })
                }}
            </div>

            {move || {
                promoting
                    .get()
                    .map(|(sq, index)| {
                        view! {
                            <Promotion
                                square=sq
                                index=index
                                orientation=orientation
                                on_promote=on_promote
                                on_cancel=Callback::new(move |()| {
                                    on_cancel.run(());
                                    promoting.set(None);
                                })
                                on_pick=Callback::new(move |()| promoting.set(None))
                            />
                        }
                    })
            }}
        </div>
    }
}

/// The promotion popup: a column of piece choices at the destination square, over
/// a full-board backdrop that cancels on any click outside it.
///
/// Three exits, because the picker opens on geometry alone and the move under it
/// may not be a pawn's ([`arrow::Arrow::could_be_promotion`] is necessary, not
/// sufficient): pick a piece to promote, choose **no promotion** to keep the plain
/// move (a rook lift to the last rank is a real, legal move), or click away to take
/// the whole move back. Without the middle exit the ~14% of puzzles whose key is a
/// non-pawn move to the last rank could not be entered at all.
#[component]
fn Promotion(
    square: shakmaty::Square,
    index: usize,
    orientation: square::Orientation,
    #[prop(into)] on_promote: Callback<(usize, shakmaty::Role)>,
    #[prop(into)] on_cancel: Callback<()>,
    /// Called once the choice is resolved — a piece picked or "no promotion" — so
    /// the board can close the popup. The plain arrow is already drawn, so closing
    /// after "no promotion" simply leaves it in place.
    #[prop(into)]
    on_pick: Callback<()>,
) -> impl IntoView {
    // The promoting pawn is always the solver's, and the board is drawn from the
    // solver's side, so the piece colour is exactly the orientation's.
    let color = orientation.0;
    let (col, row) = square::cell(square, orientation);
    let per = constants::PERCENT_PER_SQUARE;

    view! {
        <div class="board__promotion">
            // The backdrop swallows the pointer so the modal board draws no arrow
            // behind it, and any press cancels the promotion.
            <div
                class="promotion-backdrop"
                on:pointerdown=move |ev| {
                    ev.stop_propagation();
                    on_cancel.run(());
                }
            ></div>
            <div
                class="promotion-picker"
                style:left=format!("{}%", f64::from(col) * per)
                style:top=format!("{}%", f64::from(row) * per)
                style:width=format!("{per}%")
                // Keep the picker's own pointer events off the board's drag logic.
                on:pointerdown=|ev| ev.stop_propagation()
                on:pointerup=|ev| ev.stop_propagation()
            >
                {blindfold_core::constants::PROMOTABLE
                    .into_iter()
                    .map(|role| {
                        view! {
                            <button
                                class="promotion-picker__choice"
                                aria-label=roster::name(role, false)
                                title=roster::name(role, false)
                                on:click=move |ev| {
                                    ev.stop_propagation();
                                    on_promote.run((index, role));
                                    on_pick.run(());
                                }
                            >
                                <span inner_html=pieces::svg(color, role) />
                            </button>
                        }
                    })
                    .collect_view()}
                // The escape hatch for a move that only *looked* like a promotion:
                // keep the arrow as the plain move it already is.
                <button
                    class="promotion-picker__plain"
                    aria-label="Move without promoting"
                    title="Move without promoting"
                    on:click=move |ev| {
                        ev.stop_propagation();
                        on_pick.run(());
                    }
                >
                    "no promotion"
                </button>
            </div>
        </div>
    }
}

/// File letters along the bottom edge and rank numbers up the left.
///
/// Rendered into the squares that sit on those edges rather than as rulers
/// outside the board, so the labels cannot drift out of alignment with the grid
/// they describe. They matter more here than on a normal board: they are the only
/// thing tying the roster's "d5" to a place the user can drag to.
#[component]
fn Coordinates(square: shakmaty::Square, orientation: square::Orientation) -> impl IntoView {
    let (col, row) = square::cell(square, orientation);
    let last = constants::BOARD_SIDE as u32 - 1;
    view! {
        {(row == last)
            .then(|| view! { <span class="coord coord--file">{square.file().char().to_string()}</span> })}
        {(col == 0)
            .then(|| view! { <span class="coord coord--rank">{square.rank().char().to_string()}</span> })}
    }
}

/// The numbered arrows, plus the one being dragged right now.
#[component]
fn Arrows(
    #[prop(into)] drawn: Signal<Vec<arrow::Arrow>>,
    dragging: Dragging,
    hovering: RwSignal<Option<shakmaty::Square>>,
    orientation: square::Orientation,
) -> impl IntoView {
    let side = constants::VIEWBOX_SIDE;

    // The arrowhead triangle, built from the marker's constants rather than spelled
    // out, so its coordinates cannot drift from the viewBox and anchor beside it: a
    // tip at (VIEWBOX - MARGIN, ANCHOR_Y) and a back edge down the x=0 side, inset
    // from top and bottom by MARGIN.
    let margin = constants::ARROW_HEAD_MARGIN;
    let far = constants::ARROW_HEAD_VIEWBOX - margin;
    let head_path = format!(
        "M0,{margin} L{far},{} L0,{far} z",
        constants::ARROW_HEAD_ANCHOR_Y
    );

    view! {
        <svg class="board__arrows" viewBox=format!("0 0 {side} {side}") aria-hidden="true">
            <defs>
                <marker
                    id=constants::ARROW_HEAD_ID
                    viewBox=format!(
                        "0 0 {} {}",
                        constants::ARROW_HEAD_VIEWBOX,
                        constants::ARROW_HEAD_VIEWBOX,
                    )
                    refX=constants::ARROW_HEAD_ANCHOR_X
                    refY=constants::ARROW_HEAD_ANCHOR_Y
                    markerWidth=constants::ARROW_HEAD_SCALE
                    markerHeight=constants::ARROW_HEAD_SCALE
                    orient="auto-start-reverse"
                >
                    <path d=head_path fill="currentColor" />
                </marker>
            </defs>

            {move || {
                drawn
                    .get()
                    .into_iter()
                    .enumerate()
                    .map(|(i, a)| {
                        view! { <Shaft from=a.from to=a.to orientation=orientation number=Some(i + 1) /> }
                    })
                    .collect_view()
            }}

            {move || {
                let (from, to) = (dragging.get()?, hovering.get()?);
                (from != to).then(|| view! { <Shaft from=from to=to orientation=orientation number=None /> })
            }}
        </svg>
    }
}

/// One arrow. `number` is its place in the line, or `None` while it is still
/// being dragged and has no place yet.
#[component]
fn Shaft(
    from: shakmaty::Square,
    to: shakmaty::Square,
    orientation: square::Orientation,
    number: Option<usize>,
) -> impl IntoView {
    let (x1, y1) = square::centre(from, orientation);
    let (x2, y2) = square::centre(to, orientation);

    // Stop short of the target's centre so the head points at the square rather
    // than covering it, and so two arrows converging on one square stay legible.
    let (dx, dy) = (x2 - x1, y2 - y1);
    let length = dx.hypot(dy);
    let scale = if length > constants::ARROW_HEAD_INSET {
        (length - constants::ARROW_HEAD_INSET) / length
    } else {
        0.0
    };
    let (hx, hy) = (x1 + dx * scale, y1 + dy * scale);

    view! {
        <g class="arrow" class:arrow--ghost=number.is_none()>
            <line
                x1=x1
                y1=y1
                x2=hx
                y2=hy
                stroke-width=constants::ARROW_WIDTH
                stroke-linecap="butt"
                marker-end=format!("url(#{})", constants::ARROW_HEAD_ID)
            />
            {number
                .map(|n| {
                    view! {
                        <circle cx=x1 cy=y1 r=constants::ARROW_BADGE_RADIUS class="arrow__badge" />
                        <text
                            x=x1
                            y=y1
                            class="arrow__number"
                            font-size=constants::ARROW_NUMBER_SIZE
                            dy=constants::ARROW_NUMBER_BASELINE_EM
                        >
                            {n.to_string()}
                        </text>
                    }
                })}
        </g>
    }
}
