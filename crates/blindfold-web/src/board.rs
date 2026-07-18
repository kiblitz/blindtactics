//! The board: eight by eight of nothing, until it isn't.
//!
//! Rendering only. Every question with a right answer — which square is under the
//! pointer, where an arrow's head goes, whether a line mates — is answered by
//! [`crate::square`] or by `blindfold_core`, so what is left here is markup and
//! event plumbing.
//!
//! The board is layers in one `aspect-ratio: 1` box: the squares, an SVG overlay
//! for the arrows, and the pieces. Only the squares take pointer events; the arrows
//! and pieces are `pointer-events: none` so a drag that crosses an arrow it has
//! already drawn does not get swallowed by it. Promotion is chosen in the line
//! panel, not here — a last-rank drag draws a plain move and the line offers a
//! per-move piece control (see [`crate::line`]), so the board stays pure geometry.

use crate::constants;
use crate::pieces;
use crate::square;
use blindfold_core::arrow;
use leptos::prelude::*;
use shakmaty::Position as _;

/// Where a drag started, if one is in progress.
///
/// An explicit `Option` rather than inferring a drag from the pointer being
/// down: the pointer can go down outside the board and arrive over it already
/// moving, and "no drag started here" and "a drag started on a1" are different
/// states that must not be told apart by a coordinate.
type Dragging = RwSignal<Option<shakmaty::Square>>;

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

/// The blank board, the arrows drawn on it, and — once revealed — the pieces.
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
    /// Called with each arrow the user completes by dragging. A last-rank drag is
    /// reported as a plain move like any other; whether it promotes is chosen later
    /// in the line panel, so the board never has to guess.
    #[prop(into)]
    on_draw: Callback<arrow::Arrow>,
    #[prop(into)] revealed: Signal<Option<shakmaty::Chess>>,
    /// The square a move just landed on, lit so the eye can follow the replay.
    #[prop(into)]
    highlight: Signal<Option<shakmaty::Square>>,
    /// Whether the user may still draw. False once the board is revealed — a solve or
    /// a give-up, both of which lock the board.
    #[prop(into)]
    locked: Signal<bool>,
) -> impl IntoView {
    let dragging: Dragging = RwSignal::new(None);
    // Where the pointer is now, so the in-flight arrow follows it rather than
    // appearing only once released — and, when no drag is in progress, so the
    // square under the pointer can be highlighted.
    let hovering: RwSignal<Option<shakmaty::Square>> = RwSignal::new(None);

    let on_down = move |ev: leptos::ev::PointerEvent| {
        if locked.get_untracked() {
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
        if locked.get_untracked() {
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
        // A plain move, even onto the last rank. Whether it promotes is the line
        // panel's question, not the board's — the board cannot know, since the same
        // drag is a pawn's against one defense and a rook's against another.
        on_draw.run(arrow::Arrow::new(from, to));
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
                    // `context-stroke`, not a fixed fill, so the one shared marker
                    // paints in whatever colour the arrow referencing it strokes with
                    // — otherwise every head would take the amber inherited at the
                    // `<defs>`, ignoring the per-arrow `style:color` below.
                    <path d=head_path fill="context-stroke" />
                </marker>
            </defs>

            {move || {
                let arrows = drawn.get();
                arrows
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        // Each move its own colour, cycled by position; and a move
                        // drawn more than once is fanned off its twins so both stay
                        // visible instead of the later one hiding the earlier.
                        let color = constants::ARROW_COLORS[i % constants::ARROW_COLORS.len()];
                        let twins = arrows[..i]
                            .iter()
                            .filter(|b| b.from == a.from && b.to == a.to)
                            .count();
                        view! {
                            <Shaft
                                from=a.from
                                to=a.to
                                orientation=orientation
                                number=Some(i + 1)
                                color=color
                                offset=twins as f64 * constants::ARROW_DUP_OFFSET
                            />
                        }
                    })
                    .collect_view()
            }}

            {move || {
                let (from, to) = (dragging.get()?, hovering.get()?);
                (from != to)
                    .then(|| {
                        view! {
                            <Shaft
                                from=from
                                to=to
                                orientation=orientation
                                number=None
                                color=constants::GHOST_ARROW_COLOR
                                offset=0.0
                            />
                        }
                    })
            }}
        </svg>
    }
}

/// One arrow. `number` is its place in the line, or `None` while it is still
/// being dragged and has no place yet. `color` is its shaft/head/badge colour and
/// `offset` fans it perpendicular off any twin sharing the same from/to.
#[component]
fn Shaft(
    from: shakmaty::Square,
    to: shakmaty::Square,
    orientation: square::Orientation,
    number: Option<usize>,
    color: &'static str,
    offset: f64,
) -> impl IntoView {
    // Fan the whole shaft perpendicular to itself so a move drawn twice does not
    // hide under its twin — the geometry lives in `square` where it is tested.
    let ((x1, y1), (x2, y2)) = square::fan(
        square::centre(from, orientation),
        square::centre(to, orientation),
        offset,
    );

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
        <g class="arrow" class:arrow--ghost=number.is_none() style:color=color>
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
