//! Tests for the piece roster — the only information the blindfold user gets.

mod common;

use blindfold_core::roster;

#[test]
fn reads_a_position() {
    let r = roster::of(&common::pos(common::BACK_RANK));
    assert_eq!(r.to_move, shakmaty::Color::White);
    assert_eq!(
        r.text(),
        "white to play. white: king g1, rook a1. black: king g8, pawns f7 g7 h7."
    );
}

#[test]
fn announces_the_side_to_move_first() {
    let r = roster::of(&common::pos(common::BACK_RANK_IDLE));
    assert_eq!(r.to_move, shakmaty::Color::Black);
    assert_eq!(
        r.text(),
        "black to play. black: king g8, pawns f7 g7 h7. white: king g1, rook a1.",
        "the mover is read out first, as a human would"
    );
    // The struct fields stay colour-keyed regardless of who is to move.
    assert_eq!(r.white.color, shakmaty::Color::White);
    assert_eq!(r.black.color, shakmaty::Color::Black);
}

#[test]
fn orders_roles_king_first_then_descending_value() {
    let r = roster::of(&common::pos("4k3/8/8/8/8/8/PPP5/RNBQKB2 w - - 0 1"));
    let roles: Vec<shakmaty::Role> = r.white.entries.iter().map(|e| e.role).collect();
    assert_eq!(
        roles,
        vec![
            shakmaty::Role::King,
            shakmaty::Role::Queen,
            shakmaty::Role::Rook,
            shakmaty::Role::Bishop,
            shakmaty::Role::Knight,
            shakmaty::Role::Pawn,
        ],
        "shakmaty's own Role ordering runs pawn-first, which is not announcement order"
    );
}

#[test]
fn orders_squares_by_file_then_rank() {
    // Pawns on g5, a6, b7 — deliberately out of order, and deliberately spanning
    // ranks so a rank-major sort would give a different answer.
    let r = roster::of(&common::pos("4k3/1P6/P7/6P1/8/8/8/4K3 w - - 0 1"));
    let pawns = r
        .white
        .entries
        .iter()
        .find(|e| e.role == shakmaty::Role::Pawn)
        .expect("pawns");
    assert_eq!(pawns.text(), "pawns a6 b7 g5");
}

#[test]
fn pluralizes_by_count() {
    let r = roster::of(&common::pos("4k3/8/8/8/8/8/8/RN2K1N1 w - - 0 1"));
    let named: Vec<&str> = r.white.entries.iter().map(|e| e.name()).collect();
    assert_eq!(named, vec!["king", "rook", "knights"]);
}

#[test]
fn omits_roles_that_are_absent() {
    let r = roster::of(&common::pos(common::BACK_RANK));
    assert_eq!(r.white.entries.len(), 2, "only a king and a rook");
    assert!(
        r.white.entries.iter().all(|e| !e.squares.is_empty()),
        "no empty entries"
    );
}

/// Every position has exactly one king per side, so the roster always names both.
#[test]
fn both_kings_are_always_present() {
    for fen in [
        common::BACK_RANK,
        common::BRANCHING_LINEAR,
        common::BRANCHING_BLOCKED,
        common::LADDER,
        common::STALEMATE_TRAP,
    ] {
        let r = roster::of(&common::pos(fen));
        for side in [&r.white, &r.black] {
            let king = side.entries.first().expect("at least one entry");
            assert_eq!(
                king.role,
                shakmaty::Role::King,
                "king leads the roster: {fen}"
            );
            assert_eq!(king.squares.len(), 1, "exactly one king: {fen}");
        }
    }
}
