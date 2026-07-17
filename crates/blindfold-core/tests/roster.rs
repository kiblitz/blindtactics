//! Tests for the piece roster — the only information the blindfold user gets.

mod common;

use blindfold_core::mate;
use blindfold_core::roster;
// Trait import, so `as _`: shakmaty's board queries are trait methods and Rust has
// no way to reach them via a module path. Nothing here refers to the name.
use shakmaty::Position as _;

/// The roster is the user's *only* channel. Anything that changes the answer has
/// to be visible in it.
///
/// This is the property, not a detail: a blindfold user cannot look at the board.
/// If two positions render the same roster but have different solutions, then one
/// of those puzzles is unsolvable from what the user was told — and they would be
/// marked wrong for a correct answer with no way to see why.
///
/// Castling rights and the en-passant square are the two pieces of chess state
/// that are not placement, and both decide a mate in our own fixtures. Note that
/// `tests/mate_edge_cases.rs` builds those fixtures precisely to prove the
/// *solver* handles them, which is what made this easy to miss: the solver's reach
/// and the roster's reach were tested separately and never against each other.
#[test]
fn roster_distinguishes_positions_whose_answers_differ() {
    for (with, without, key, what) in [
        (
            common::EN_PASSANT_MATE,
            "8/8/Q7/Pp6/8/8/8/k1K5 w - - 0 1",
            "a5b6",
            "en passant",
        ),
        (
            common::CASTLING_MATE,
            "8/8/8/8/5Q2/3k4/2R5/R3K3 w - - 0 1",
            "e1c1",
            "castling",
        ),
    ] {
        let with = common::pos(with);
        let without = common::pos(without);

        // Identical pieces on identical squares...
        assert_eq!(
            with.board(),
            without.board(),
            "{what}: the fixtures must differ only in non-placement state"
        );
        // ...and yet only one of them is a mate.
        assert!(
            mate::judge(&with, &common::line(key)).mates(),
            "{what}: {key} must mate here"
        );
        assert!(
            !mate::judge(&without, &common::line(key)).mates(),
            "{what}: {key} must not mate once the right is gone"
        );

        // So the roster cannot be allowed to render them the same.
        assert_ne!(
            roster::of(&with),
            roster::of(&without),
            "{what}: same roster, different answer — the puzzle is unsolvable blind"
        );
        assert_ne!(
            roster::of(&with).text(),
            roster::of(&without).text(),
            "{what}: the *rendered* roster must differ too, not just the struct"
        );
    }
}

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
fn announces_castling_rights() {
    let r = roster::of(&common::pos(common::CASTLING_MATE));
    assert_eq!(
        r.text(),
        "white to play. white: king e1, queen f4, rooks a1 c2, may castle queenside. \
         black: king d3."
    );
    assert_eq!(
        r.white.castling,
        roster::Castling {
            kingside: false,
            queenside: true
        }
    );
    assert_eq!(r.black.castling, roster::Castling::default());
}

#[test]
fn announces_both_castling_rights_together() {
    let r = roster::of(&common::pos("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1"));
    assert_eq!(
        r.text(),
        "white to play. white: king e1, rooks a1 h1, may castle either side. \
         black: king e8, rooks a8 h8, may castle either side."
    );
}

#[test]
fn announces_an_en_passant_square() {
    let r = roster::of(&common::pos(common::EN_PASSANT_MATE));
    assert_eq!(
        r.text(),
        "white to play. white: king c1, queen a6, pawn a5. black: king a1, pawn b5. \
         en passant on b6."
    );
    assert_eq!(r.en_passant, Some(shakmaty::Square::B6));
}

/// A FEN may name an en-passant square that no pawn can actually capture on. The
/// roster stays silent, because `EnPassantMode::Legal` reports the square only when
/// the capture is playable.
///
/// This is safe rather than lossy: en-passant rights expire after one ply, so a
/// square with no legal capture now can never matter later, and cannot be hiding an
/// answer. The raw Lichess FEN carries squares like this routinely, which is why it
/// is worth pinning — the alternative (`Always`) would have the roster announce a
/// right nobody can use on a large fraction of the database.
#[test]
fn stays_silent_about_an_en_passant_square_nobody_can_use() {
    let r = roster::of(&common::pos("8/8/8/1p6/8/8/8/k1K5 w - b6 0 1"));
    assert_eq!(r.en_passant, None);
    assert_eq!(
        r.text(),
        "white to play. white: king c1. black: king a1, pawn b5.",
        "no legal capture, so nothing to say"
    );
}

/// Rights are announced even when the side is in check and cannot use them this
/// ply. The check may be parried and the right used later in the line.
#[test]
fn announces_rights_that_cannot_be_used_yet() {
    let r = roster::of(&common::pos("4k3/8/8/8/7q/8/8/R3K2R w KQ - 0 1"));
    assert!(
        r.text().contains("may castle either side"),
        "rights survive a check: {}",
        r.text()
    );
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
