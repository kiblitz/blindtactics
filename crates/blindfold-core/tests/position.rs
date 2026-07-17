//! Tests for FEN parsing and writing.
//!
//! `position` had no test file of its own until `to_fen` arrived — it was covered
//! incidentally, through `puzzle`. That was thin for a module the curation tool is
//! about to lean on from both directions.

mod common;

use blindfold_core::position;
use blindfold_core::roster;

#[test]
fn round_trips_every_fixture() {
    for fen in [
        common::BACK_RANK,
        common::BRANCHING_LINEAR,
        common::LADDER,
        common::STALEMATE_TRAP,
        common::UNDERPROMOTION,
        common::EN_PASSANT_MATE,
        common::CASTLING_MATE,
        common::SMOTHERED,
        common::BACK_RANK_BLACK,
    ] {
        let pos = common::pos(fen);
        let written = position::to_fen(&pos);
        assert_eq!(
            position::of_fen(&written).expect("our own output must reparse"),
            pos,
            "round trip changed the position: {fen} -> {written}"
        );
    }
}

/// The two bits of state that are not placement have to survive a round trip, or
/// the curation tool would quietly strip the very thing that makes these mate.
#[test]
fn round_trip_preserves_castling_and_en_passant() {
    for (fen, needle) in [
        (common::CASTLING_MATE, "w Q -"),
        (common::EN_PASSANT_MATE, "w - b6"),
    ] {
        let written = position::to_fen(&common::pos(fen));
        assert!(
            written.contains(needle),
            "{fen} must still say `{needle}`, got {written}"
        );
    }
}

/// `to_fen` uses `EnPassantMode::Legal`, so a square nobody can capture on is not
/// written. This is the canonicalisation the module doc commits to, and it is what
/// keeps the stored FEN carrying exactly the state [`roster`] announces.
#[test]
fn drops_an_en_passant_square_nobody_can_use() {
    let pos = common::pos("8/8/8/1p6/8/8/8/k1K5 w - b6 0 1");
    let written = position::to_fen(&pos);
    assert!(
        written.ends_with("w - - 0 1"),
        "a dead ep square is not canonical state, got {written}"
    );
    assert_eq!(
        position::of_fen(&written).expect("reparses"),
        pos,
        "dropping it must not change the position"
    );
}

/// The invariant that ties `to_fen` to the roster: what we store and what we say
/// agree about en passant. If these ever diverge, the database holds state the
/// user is never told about.
#[test]
fn what_is_stored_matches_what_the_user_is_told() {
    for fen in [
        common::EN_PASSANT_MATE,
        "8/8/8/1p6/8/8/8/k1K5 w - b6 0 1",
        common::BACK_RANK,
    ] {
        let pos = common::pos(fen);
        let stored = position::of_fen(&position::to_fen(&pos)).expect("reparses");
        assert_eq!(
            roster::of(&stored),
            roster::of(&pos),
            "the roster must survive storage: {fen}"
        );
    }
}

#[test]
fn rejects_nonsense() {
    assert!(matches!(
        position::of_fen("not a fen"),
        Err(position::Error::Parse { .. })
    ));
}

/// Parseable but not a legal chess position — three white kings.
#[test]
fn rejects_a_parseable_but_illegal_position() {
    assert!(matches!(
        position::of_fen("4k3/8/8/8/8/8/8/KKK5 w - - 0 1"),
        Err(position::Error::Illegal { .. })
    ));
}
