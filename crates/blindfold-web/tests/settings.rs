//! Tests for the point-of-view preference and how it resolves to an orientation.
//!
//! `load`/`save` touch `localStorage` and are covered by the browser e2e, like
//! `rating`'s; the side-resolution that actually decides the orientation is pure,
//! and this is where it is pinned — the sign of the flip especially, the same care
//! `square` takes with the board geometry.

use blindfold_web::settings;

#[test]
fn to_move_follows_the_solver() {
    assert_eq!(
        settings::Pov::ToMove.side(shakmaty::Color::White),
        shakmaty::Color::White
    );
    assert_eq!(
        settings::Pov::ToMove.side(shakmaty::Color::Black),
        shakmaty::Color::Black
    );
}

#[test]
fn white_and_black_ignore_the_solver() {
    for solver in [shakmaty::Color::White, shakmaty::Color::Black] {
        assert_eq!(settings::Pov::White.side(solver), shakmaty::Color::White);
        assert_eq!(settings::Pov::Black.side(solver), shakmaty::Color::Black);
    }
}

/// The flip inverts whatever the POV resolved to, for every combination — the
/// transient half of orientation, layered on the persisted POV.
#[test]
fn flipping_inverts_the_resolved_side() {
    for pov in settings::Pov::ALL {
        for solver in [shakmaty::Color::White, shakmaty::Color::Black] {
            let base = settings::facing(pov, solver, false);
            let flipped = settings::facing(pov, solver, true);
            assert_eq!(base, pov.side(solver), "unflipped is the POV's side");
            assert_eq!(
                flipped,
                base.other(),
                "flipped is its opposite ({pov:?}, {solver:?})"
            );
        }
    }
}

/// Every POV has a distinct, non-empty menu label, and `ALL` lists each once — the
/// menu renders straight off `ALL`, so a duplicate or a gap would be a broken menu.
#[test]
fn every_pov_has_a_distinct_label() {
    let labels: std::collections::HashSet<&str> =
        settings::Pov::ALL.into_iter().map(|p| p.label()).collect();
    assert_eq!(
        labels.len(),
        settings::Pov::ALL.len(),
        "labels are distinct"
    );
    assert!(settings::Pov::ALL
        .into_iter()
        .all(|p| !p.label().is_empty()));
}
