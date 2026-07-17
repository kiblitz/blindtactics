//! Tests for arrow parsing and resolution.

mod common;

use blindfold_core::arrow;

/// Both rooks and both kings home, all castling rights available.
const CASTLING: &str = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";

#[test]
fn round_trips_through_uci() {
    for uci in ["e2e4", "a1a8", "h7h8q", "e7e8n", "b1c3"] {
        let a: arrow::Arrow = uci.parse().expect("parses");
        assert_eq!(a.to_string(), uci);
    }
}

#[test]
fn round_trips_through_json() {
    let a = common::a("h7h8q");
    let json = serde_json::to_string(&a).expect("serializes");
    assert_eq!(
        json, r#""h7h8q""#,
        "arrows must serialize as bare UCI strings"
    );
    assert_eq!(
        serde_json::from_str::<arrow::Arrow>(&json).expect("deserializes"),
        a
    );
}

#[test]
fn parses_promotion() {
    let a = common::a("e7e8q");
    assert_eq!(a.from, shakmaty::Square::E7);
    assert_eq!(a.to, shakmaty::Square::E8);
    assert_eq!(a.promotion, Some(shakmaty::Role::Queen));
}

#[test]
fn rejects_malformed_uci() {
    assert!("e2e".parse::<arrow::Arrow>().is_err(), "too short");
    assert!("e2e4e5".parse::<arrow::Arrow>().is_err(), "too long");
    assert!("z2e4".parse::<arrow::Arrow>().is_err(), "not a file");
    assert!("e9e4".parse::<arrow::Arrow>().is_err(), "not a rank");
    assert!(
        "e7e8k".parse::<arrow::Arrow>().is_err(),
        "cannot promote to a king"
    );
    assert!(
        "e7e8p".parse::<arrow::Arrow>().is_err(),
        "cannot promote to a pawn"
    );
}

/// Multi-byte input must be *rejected*, not panic.
///
/// The bug this pins: the length gate counted bytes while the parser sliced the
/// `&str` at fixed byte offsets, so a 4-byte single char passed the gate and then
/// split a codepoint. Every input below panicked before the fix.
///
/// This is not a curiosity. `Arrow` is `#[serde(try_from = "String")]`, so the
/// panic reached `puzzle::of_jsonl` — a function that returns `Result` — and
/// one bad byte in a committed database file would abort the whole wasm app
/// instead of surfacing a parse error.
#[test]
fn rejects_multibyte_input_without_panicking() {
    for s in [
        "🙂", "中a", "é2é", "e€4", "€e4", "e2€", "e2eé", "😀4", "aé4", "中中",
    ] {
        assert!(
            s.parse::<arrow::Arrow>().is_err(),
            "{s:?} must parse to an error, not panic"
        );
    }
}

/// The length error should report what a human sees, not what the allocator does.
#[test]
fn length_error_counts_characters_not_bytes() {
    match "中中".parse::<arrow::Arrow>() {
        Err(arrow::ParseError::Length(n)) => assert_eq!(n, 2, "two characters, six bytes"),
        other => panic!("expected a Length error, got {other:?}"),
    }
}

/// UCI promotion suffixes are lowercase. Accepting uppercase would make parsing
/// non-round-trip-stable, since `Display` always emits lowercase.
#[test]
fn rejects_uppercase_promotion() {
    assert!(
        "e7e8Q".parse::<arrow::Arrow>().is_err(),
        "UCI promotions are lowercase"
    );
}

/// An en-passant capture can never promote — it always lands on rank 3 or 6.
///
/// shakmaty's UCI resolution builds `Move::EnPassant` without consulting the
/// promotion suffix, so `e5d6q` and `e5d6` resolved to the *same* move while
/// comparing unequal as `Arrow`s. That directly contradicts this module's claim
/// that an arrow is identified by `(from, to, promotion)` and nothing else, so
/// the impossible suffix is rejected outright.
#[test]
fn rejects_a_promotion_suffix_on_an_en_passant_capture() {
    let pos = common::pos("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
    assert!(
        common::a("e5d6").resolve(&pos).is_ok(),
        "the plain e.p. capture is legal"
    );
    assert_eq!(
        common::a("e5d6q").resolve(&pos),
        Err(arrow::Error::Illegal),
        "a promoting en-passant capture is not a thing"
    );
}

/// The invariant the whole design leans on: resolving an arrow and reading the
/// arrow back off the move is the identity.
#[test]
fn resolve_and_of_move_round_trip() {
    for (fen, uci) in [
        (common::BACK_RANK, "a1a8"),
        (CASTLING, "e1g1"),
        (CASTLING, "e1c1"),
        ("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6"),
        ("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1", "b7b8q"),
        ("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1", "b7b8n"),
    ] {
        let pos = common::pos(fen);
        let a = common::a(uci);
        let m = a
            .resolve(&pos)
            .unwrap_or_else(|e| panic!("{uci} in {fen}: {e}"));
        assert_eq!(
            arrow::Arrow::of_move(&m),
            Some(a),
            "round trip broke for {uci} in {fen}"
        );
    }
}

/// A pawn reaching the back rank without a stated promotion is not a move. The
/// UI must prompt for the piece rather than silently queening.
#[test]
fn promotion_requires_an_explicit_piece() {
    let pos = common::pos("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1");
    assert_eq!(common::a("b7b8").resolve(&pos), Err(arrow::Error::Illegal));
    assert!(common::a("b7b8q").resolve(&pos).is_ok());
}

// ---------------------------------------------------------------------------
// Castling
// ---------------------------------------------------------------------------

#[test]
fn castling_resolves_from_the_kings_travel() {
    let pos = common::pos(CASTLING);
    let m = common::a("e1g1").resolve(&pos).expect("O-O is legal");
    assert!(m.is_castle());
}

/// A user dragging their king onto their own rook can only mean "castle", and
/// the Lichess database emits this encoding too (lila carries an `altCastles`
/// table to undo it). Both spellings must reach the same move.
#[test]
fn castling_also_accepts_king_takes_rook() {
    let pos = common::pos(CASTLING);
    let standard = common::a("e1g1")
        .resolve(&pos)
        .expect("O-O via king travel");
    let takes_rook = common::a("e1h1")
        .resolve(&pos)
        .expect("O-O via king-takes-rook");
    assert_eq!(standard, takes_rook);
}

#[test]
fn castling_queenside_both_spellings() {
    let pos = common::pos(CASTLING);
    let standard = common::a("e1c1")
        .resolve(&pos)
        .expect("O-O-O via king travel");
    let takes_rook = common::a("e1a1")
        .resolve(&pos)
        .expect("O-O-O via king-takes-rook");
    assert_eq!(standard, takes_rook);
    assert!(standard.is_castle());
}

#[test]
fn king_takes_rook_is_not_accepted_without_castling_rights() {
    // Same piece placement, no rights.
    let pos = common::pos("r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1");
    assert!(
        common::a("e1h1").resolve(&pos).is_err(),
        "no rights, and Rh1 is our own piece"
    );
    assert!(common::a("e1g1").resolve(&pos).is_err());
}

#[test]
fn of_move_recovers_the_arrow_a_user_would_draw() {
    let pos = common::pos(CASTLING);
    let m = common::a("e1g1").resolve(&pos).expect("legal");
    // Not `e1h1`: the arrow is the king's travel, which is what was dragged.
    assert_eq!(arrow::Arrow::of_move(&m), Some(common::a("e1g1")));
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

#[test]
fn resolving_an_empty_square_fails() {
    let pos = common::pos(common::BACK_RANK);
    assert_eq!(common::a("d4d5").resolve(&pos), Err(arrow::Error::Illegal));
}

#[test]
fn resolving_an_opponent_piece_fails() {
    let pos = common::pos(common::BACK_RANK); // White to move.
    assert_eq!(
        common::a("f7f6").resolve(&pos),
        Err(arrow::Error::Illegal),
        "f7 is Black's pawn"
    );
}

/// The same arrow means different `shakmaty::Move`s in different positions — a
/// quiet slide in one, a capture in another. This is why arrows, not moves, are
/// the unit of identity.
#[test]
fn one_arrow_resolves_to_different_moves_in_different_positions() {
    let quiet = common::pos("6k1/8/8/8/8/8/8/R5K1 w - - 0 1");
    let capture = common::pos("r5k1/8/8/8/8/8/8/R5K1 w - - 0 1");

    let a = common::a("a1a8");
    let quiet_move = a.resolve(&quiet).expect("legal slide");
    let capture_move = a.resolve(&capture).expect("legal capture");

    assert_eq!(quiet_move.capture(), None);
    assert_eq!(capture_move.capture(), Some(shakmaty::Role::Rook));
    assert_ne!(quiet_move, capture_move, "different moves...");
    assert_eq!(
        arrow::Arrow::of_move(&quiet_move),
        arrow::Arrow::of_move(&capture_move),
        "...but the very same arrow"
    );
}
