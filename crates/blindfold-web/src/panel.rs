//! The roster panel — everything the user is told, and nothing else.
//!
//! This is the whole puzzle, from the user's side. It renders
//! [`blindfold_core::roster::Roster`], which is structured data rather than a
//! string precisely so it can be drawn as pieces here, read as text by a screen
//! reader, and spoken aloud by the audio mode later — all from one source that
//! cannot disagree with itself.
//!
//! It carries castling rights and the en-passant square as well as placement,
//! and that is not decoration: both decide a mate, and a roster that omits them
//! makes a puzzle unsolvable and then marks a correct answer wrong. See
//! `roster.rs`'s "the roster must carry everything that decides the answer".

use crate::pieces;
use blindfold_core::roster;
use leptos::prelude::*;

/// The roster for a position, announced in the order a human would read it out:
/// the side to move first.
#[component]
pub fn Roster(#[prop(into)] roster: Signal<roster::Roster>) -> impl IntoView {
    view! {
        <section class="panel" aria-label="Piece locations">
            <h2 class="panel__title">"Roster"</h2>

            <p class="panel__tomove">
                {move || {
                    format!("{} to play. Find the forced mate.", roster::heading(roster.get().to_move))
                }}
            </p>

            {move || {
                let r = roster.get();
                r.sides_in_announce_order()
                    .into_iter()
                    .map(|side| view! { <Side side=side.clone() /> })
                    .collect_view()
            }}

            {move || {
                roster
                    .get()
                    .en_passant
                    .map(|sq| {
                        view! {
                            <p class="panel__extra">
                                "En passant on " <span class="mono">{sq.to_string()}</span> "."
                            </p>
                        }
                    })
            }}

            // The text the roster renders itself as. Visually hidden, but it is
            // what a screen reader announces and what the audio mode will speak,
            // so it is the same sentence the user would hear — not a second
            // rendering that could drift from the one above.
            <p class="visually-hidden">{move || roster.get().text()}</p>
        </section>
    }
}

/// One side's pieces, and its castling rights if it has any.
#[component]
fn Side(side: roster::Side) -> impl IntoView {
    let color = side.color;
    let name = roster::heading(color);
    let castling = side.castling.text();

    view! {
        <div class="side">
            <p class="side__name">{name}</p>
            {side
                .entries
                .into_iter()
                .map(|entry| {
                    let squares = entry
                        .squares
                        .iter()
                        .map(shakmaty::Square::to_string)
                        .collect::<Vec<_>>()
                        .join(" ");
                    view! {
                        <div class="entry">
                            // The picture the user asked for, not a letter. `alt`
                            // rather than aria-hidden: to a screen reader this
                            // line is "knight d5", which is the announcement.
                            <span
                                class="entry__piece"
                                class:entry__piece--black=color == shakmaty::Color::Black
                                role="img"
                                aria-label=entry.name()
                                inner_html=pieces::svg(color, entry.role)
                            />
                            <span class="entry__squares mono">{squares}</span>
                        </div>
                    }
                })
                .collect_view()}
            {castling.map(|text| view! { <p class="side__castling">{text}</p> })}
        </div>
    }
}
