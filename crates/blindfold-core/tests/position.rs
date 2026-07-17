//! Tests for FEN parsing and writing.
//!
//! `position` had no test file of its own until `to_fen` arrived — it was covered
//! incidentally, through `puzzle`. That was thin for a module the curation tool is
//! about to lean on from both directions.

mod common;

use blindfold_core::position;
use blindfold_core::roster;

/// Note this asserts on the *text*, not on `Chess == Chess`.
///
/// `Chess`'s `PartialEq` ignores the en-passant square unless a capture is legal,
/// and ignores the halfmove and fullmove clocks entirely — `"… w - - 0 1"` and
/// `"… w - - 47 90"` compare equal. So a `to_fen` that mangled the clocks would
/// sail through a `==` round trip. Comparing strings is what makes this test able
/// to fail, since the string is what actually lands in the database.
///
/// Every fixture here is already canonical, which is what lets the comparison be
/// this strict.
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
        common::DIVERGENT_DEFENSE,
    ] {
        let pos = common::pos(fen);
        let written = position::to_fen(&pos);
        assert_eq!(written, fen, "to_fen must reproduce the fixture verbatim");
        assert_eq!(
            position::of_fen(&written).expect("our own output must reparse"),
            pos,
            "round trip changed the position: {fen} -> {written}"
        );
    }
}

/// The clocks survive `to_fen`, which `round_trips_every_fixture` could not check
/// on its own — every fixture there happens to sit at `0 1`.
///
/// Nothing reads the clocks today and no mate can turn on them (shakmaty has no
/// 50-move rule), but they are written into the database, and a writer that quietly
/// reset them would be invisible to `Chess == Chess`.
#[test]
fn round_trip_preserves_the_clocks() {
    let fen = "8/8/8/8/8/8/8/k1K5 w - - 47 90";
    assert_eq!(position::to_fen(&common::pos(fen)), fen);
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
/// keeps the FEN's ep square and the roster's announcement in step.
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

/// The invariant that ties `to_fen` to the roster: the FEN records an en-passant
/// square exactly when the user is told about one.
///
/// Read the FEN's fourth field as *text*, which looks clumsy and is the whole
/// point. An earlier version of this test compared `roster::of` before and after a
/// `to_fen` -> `of_fen` round trip, and was **vacuous**: it stayed green even with
/// `to_fen` switched to `EnPassantMode::Always`. Both sides read ep through
/// `Legal`, and `Chess`'s own `PartialEq` compares `legal_ep_square()` — so
/// equality is blind to exactly the field `to_fen` is choosing about, and no test
/// phrased in terms of `Chess == Chess` can ever check that choice. The string is
/// the only place the decision is observable.
#[test]
fn what_is_stored_matches_what_the_user_is_told() {
    for fen in [
        common::EN_PASSANT_MATE,            // live ep square: must be written
        "8/8/8/1p6/8/8/8/k1K5 w - b6 0 1",  // dead ep square: must not be
        "7k/8/8/r1pPK3/8/8/8/8 w - c6 0 1", // ep illegal by pin: must not be
        common::BACK_RANK,                  // no ep square at all
    ] {
        let pos = common::pos(fen);
        let stored = position::to_fen(&pos);
        let ep_field = stored.split(' ').nth(3).expect("a FEN has six fields");
        assert_eq!(
            ep_field == "-",
            roster::of(&pos).en_passant.is_none(),
            "stored `{ep_field}` disagrees with what the roster announces, for {fen}"
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
