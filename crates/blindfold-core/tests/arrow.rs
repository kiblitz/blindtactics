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

/// The length error must report the same unit the parser gates on.
///
/// This once counted characters, on the theory that a person counts characters.
/// The theory is fine and the conclusion was still wrong: the arity gate matches
/// on *bytes*, so a string can be rejected for its byte length and then be
/// described by its character length — and the two disagree exactly when the
/// input is not ASCII.
///
/// `é2é4` is the case that makes it absurd: six bytes, so it never reaches the
/// four-byte arm, but four characters, so the error read "expected 4 or 5
/// characters of UCI, got 4". The complaint refuted itself. Note that `中中`,
/// which is what this test used to check, could never have caught it — two
/// characters is outside the accepted range, so the message still read sensibly.
/// The bug needs a char count that lands *inside* the range the gate rejected.
#[test]
fn length_error_is_reported_in_the_unit_the_parser_gates_on() {
    for (s, expected) in [("é2é4", 6), ("中中", 6), ("abc", 3), ("e2e4qq", 6)] {
        match s.parse::<arrow::Arrow>() {
            Err(arrow::ParseError::Length(n)) => {
                assert_eq!(n, expected, "{s:?} is {expected} bytes");
                assert!(
                    !(4..=5).contains(&n),
                    "{s:?} was rejected for its length, so the length it reports \
                     must not be one the parser accepts — got {n}"
                );
            }
            other => panic!("expected a Length error for {s:?}, got {other:?}"),
        }
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

/// The promotion rank is the far one, and it differs by colour. Getting this
/// backwards would offer a promotion picker on a user's *own* back rank — the one
/// square a pawn of theirs can never reach.
#[test]
fn the_promotion_rank_is_the_far_side() {
    let to_eighth = arrow::Arrow::new(shakmaty::Square::G7, shakmaty::Square::G8);
    assert!(to_eighth.lands_on_promotion_rank(shakmaty::Color::White));
    assert!(!to_eighth.lands_on_promotion_rank(shakmaty::Color::Black));

    let to_first = arrow::Arrow::new(shakmaty::Square::G2, shakmaty::Square::G1);
    assert!(to_first.lands_on_promotion_rank(shakmaty::Color::Black));
    assert!(!to_first.lands_on_promotion_rank(shakmaty::Color::White));

    // Nowhere near either.
    let middle = arrow::Arrow::new(shakmaty::Square::E4, shakmaty::Square::E5);
    for color in shakmaty::Color::ALL {
        assert!(!middle.lands_on_promotion_rank(color));
    }
}

/// `could_be_promotion` is what decides whether the app offers a promotion
/// picker. It reads the drag alone — a pawn steps from the rank below onto the
/// last one, straight or one file sideways to capture.
#[test]
fn a_promotion_steps_off_the_seventh_rank() {
    for (from, to, want, what) in [
        (shakmaty::Square::G7, shakmaty::Square::G8, true, "a push"),
        (
            shakmaty::Square::G7,
            shakmaty::Square::H8,
            true,
            "a capture",
        ),
        (
            shakmaty::Square::G7,
            shakmaty::Square::F8,
            true,
            "the other capture",
        ),
        (
            shakmaty::Square::E4,
            shakmaty::Square::E8,
            false,
            "a rook lift lands on the rank but no pawn made it",
        ),
        (
            shakmaty::Square::E8,
            shakmaty::Square::F8,
            false,
            "a slide along the back rank",
        ),
        (
            shakmaty::Square::G7,
            shakmaty::Square::E8,
            false,
            "two files is not a capture — the boundary the condition draws",
        ),
        (
            shakmaty::Square::A7,
            shakmaty::Square::H8,
            false,
            "no pawn moves seven files",
        ),
        (
            shakmaty::Square::G7,
            shakmaty::Square::G6,
            false,
            "the wrong way",
        ),
    ] {
        assert_eq!(
            arrow::Arrow::new(from, to).could_be_promotion(shakmaty::Color::White),
            want,
            "{from}{to}: {what}"
        );
    }
}

/// Black promotes on the first rank, off the second.
#[test]
fn black_promotes_the_other_way() {
    let a = arrow::Arrow::new(shakmaty::Square::B2, shakmaty::Square::B1);
    assert!(a.could_be_promotion(shakmaty::Color::Black));
    assert!(!a.could_be_promotion(shakmaty::Color::White));
}

/// It is a *necessary* condition, not a sufficient one, and the doc says so —
/// this pins the gap so nobody later reads the name as a promise. A rook on the
/// seventh dragged one square forward is indistinguishable, from the drag alone,
/// from a pawn doing the same.
#[test]
fn could_be_promotion_cannot_tell_a_rook_from_a_pawn() {
    let drag = arrow::Arrow::new(shakmaty::Square::G7, shakmaty::Square::G8);
    assert!(drag.could_be_promotion(shakmaty::Color::White));

    // The same drag, made by a rook. Resolving it against a real position is what
    // settles the question — and it settles it without the promotion suffix.
    // Black's king is on h8, not g8: a king beside the rook on g8 would be
    // capturable, which is "opposite check" and not a position at all. The
    // first draft of this fixture had exactly that.
    let with_a_rook = common::pos("7k/6R1/8/8/8/8/8/4K3 w - - 0 1");
    assert!(
        drag.resolve(&with_a_rook).is_ok(),
        "a rook makes this drag legally, with no promotion"
    );
    assert!(
        arrow::Arrow::promoting(
            shakmaty::Square::G7,
            shakmaty::Square::G8,
            shakmaty::Role::Queen
        )
        .resolve(&with_a_rook)
        .is_err(),
        "and a rook cannot promote"
    );
}

/// Black's promotion rank is a back rank too.
///
/// `lands_on_back_rank` gates `resolve`'s promotion-suffix check, and it is
/// derived from `lands_on_promotion_rank` over both colours. Reducing it to
/// White's half alone — i.e. "Black can never promote" — left all of
/// `blindfold-core` green: the only thing that caught it was a *curate* test
/// noticing some committed puzzle happens to promote onto the first rank. The
/// database is deliberately small and regenerable, so that coverage could vanish
/// in a regeneration with nothing turning red. Core proves its own rules.
#[test]
fn black_may_promote_onto_the_first_rank() {
    // Black pawn b2, white king h1 tucked away, black to move.
    let pos = common::pos("4k3/8/8/8/8/8/1p6/6K1 b - - 0 1");
    assert!(
        arrow::Arrow::promoting(
            shakmaty::Square::B2,
            shakmaty::Square::B1,
            shakmaty::Role::Queen
        )
        .resolve(&pos)
        .is_ok(),
        "b2b1q is a legal promotion and must not be rejected as off-back-rank"
    );
}
