//! Chess situations the core solver fixtures do not reach.
//!
//! `tests/mate.rs` pins the *properties* of the prover — linearity, stalemate,
//! search/judge agreement — on a handful of deliberately plain positions. All of
//! them are White-to-move, and none of them involves a promotion, a castle, or an
//! en-passant capture. Those are exactly the moves where a mate solver's model of
//! "a move" tends to be subtly wrong, and a false positive here would ship a
//! broken puzzle to a user who cannot see the board and so cannot tell that they
//! were right.
//!
//! Every position here was found by brute-force enumeration rather than by hand,
//! after the warning in CLAUDE.md about hand-built FENs.

mod common;

use blindfold_core::arrow;
use blindfold_core::mate;

// ---------------------------------------------------------------------------
// Promotion
// ---------------------------------------------------------------------------

/// The mate exists only under an underpromotion, and the search must find it.
///
/// This is the position where "promote to a queen" as an unstated default is
/// wrong twice over: it misses the only mate, and the move it would pick instead
/// is a stalemate.
#[test]
fn finds_an_underpromotion_only_mate() {
    let pos = common::pos(common::UNDERPROMOTION);
    assert_eq!(
        mate::find_linear(&pos, 1),
        Some(common::line("b7b8n")),
        "b8=N# is the only mate in 1"
    );
    assert_eq!(mate::min_depth(&pos, 4), Some(1));
}

#[test]
fn judge_distinguishes_promotion_pieces() {
    let pos = common::pos(common::UNDERPROMOTION);
    assert_eq!(
        mate::judge(&pos, &common::line("b7b8n")),
        mate::Verdict::Mates { moves: 1 }
    );
    // Same arrow, same squares, different promotion piece — and a draw, not a win.
    assert!(
        matches!(
            mate::judge(&pos, &common::line("b7b8q")),
            mate::Verdict::Refuted {
                reason: mate::Reason::Stalemate,
                ..
            }
        ),
        "b8=Q stalemates and must never be judged a mate"
    );
}

/// A promotion arrow with no promotion piece is not a move. It must be rejected
/// rather than silently read as "queen": the user who drew it has not yet said
/// what they want, and guessing would hand them a stalemate in
/// [`common::UNDERPROMOTION`].
#[test]
fn a_promotion_arrow_without_a_piece_is_illegal() {
    let pos = common::pos(common::UNDERPROMOTION);
    assert_eq!(
        common::a("b7b8").resolve(&pos),
        Err(arrow::Error::Illegal),
        "b7b8 is under-specified, not a queen promotion"
    );
}

#[test]
fn of_move_round_trips_every_promotion_piece() {
    use shakmaty::Position as _;

    let pos = common::pos(common::UNDERPROMOTION);
    let mut seen = Vec::new();
    for m in pos.legal_moves().iter() {
        let Some(promotion) = m.promotion() else {
            continue;
        };
        let a = arrow::Arrow::of_move(m).expect("not a drop");
        assert_eq!(
            a.promotion,
            Some(promotion),
            "of_move dropped the promotion"
        );
        assert_eq!(a.resolve(&pos).as_ref(), Ok(m), "arrow must resolve back");
        seen.push(promotion);
    }
    seen.sort_by_key(|r| *r as usize);
    assert_eq!(
        seen,
        vec![
            shakmaty::Role::Knight,
            shakmaty::Role::Bishop,
            shakmaty::Role::Rook,
            shakmaty::Role::Queen,
        ],
        "all four promotions must survive the round trip"
    );
}

// ---------------------------------------------------------------------------
// Castling
// ---------------------------------------------------------------------------

/// `O-O-O#` is a real mate, and the arrow model has to be able to express it.
///
/// The trap this guards is the one CLAUDE.md records: a castle's raw `Move::to()`
/// is the *rook's* square, so an `of_move` that read it directly would return the
/// arrow `e1a1` here. The user drags their king to c1.
#[test]
fn finds_a_mate_delivered_by_castling() {
    let pos = common::pos(common::CASTLING_MATE);
    assert_eq!(
        mate::find_linear(&pos, 1),
        Some(common::line("e1c1")),
        "the search must return the king's travel, not the rook square"
    );
}

#[test]
fn judge_accepts_both_spellings_of_a_mating_castle() {
    let pos = common::pos(common::CASTLING_MATE);
    for uci in ["e1c1", "e1a1"] {
        assert_eq!(
            mate::judge(&pos, &common::line(uci)),
            mate::Verdict::Mates { moves: 1 },
            "{uci} is O-O-O# and must be accepted"
        );
    }
}

// ---------------------------------------------------------------------------
// En passant
// ---------------------------------------------------------------------------

#[test]
fn finds_a_mate_delivered_by_en_passant() {
    let pos = common::pos(common::EN_PASSANT_MATE);
    assert_eq!(mate::find_linear(&pos, 1), Some(common::line("a5b6")));
    assert_eq!(
        mate::judge(&pos, &common::line("a5b6")),
        mate::Verdict::Mates { moves: 1 }
    );
}

/// The en-passant version of `arrow.rs`'s
/// `one_arrow_resolves_to_different_moves_in_different_positions`, and the
/// sharpest case of it in the game.
///
/// After `b7b5` the arrow `a5b6` is an *en-passant* capture; after `b7b6` the very
/// same arrow is an ordinary pawn capture of the pawn now standing on b6. Two
/// different move kinds, two different captured squares, one drag. If linearity
/// were defined over `Move` equality this pair would be indistinguishable from a
/// bug.
#[test]
fn one_arrow_is_an_en_passant_capture_or_an_ordinary_one() {
    use shakmaty::Position as _;

    let start = common::pos("8/1p6/Q7/P7/8/8/8/k1K5 b - - 0 1");
    let a = common::a("a5b6");

    let mut double_push = start.clone();
    double_push.play_unchecked(common::a("b7b5").resolve(&start).expect("legal"));
    let ep = a.resolve(&double_push).expect("e.p. capture is legal");

    let mut single_push = start.clone();
    single_push.play_unchecked(common::a("b7b6").resolve(&start).expect("legal"));
    let normal = a.resolve(&single_push).expect("ordinary capture is legal");

    assert!(ep.is_en_passant(), "b7b5 makes a5b6 an en-passant capture");
    assert!(
        !normal.is_en_passant(),
        "b7b6 makes a5b6 an ordinary capture"
    );
    assert_ne!(ep, normal, "different moves...");
    assert_eq!(
        arrow::Arrow::of_move(&ep),
        arrow::Arrow::of_move(&normal),
        "...but the very same arrow"
    );
}

/// En-passant rights expire after one move, so an arrow that depends on them is
/// only legal against the single defense that granted them. That makes it the
/// opposite of linear, and the prover has to say so.
#[test]
fn an_en_passant_arrow_is_not_linear_when_only_one_defense_allows_it() {
    let pos = common::pos("8/1p6/Q7/P7/8/8/8/k1K5 w - - 0 1");
    assert!(
        matches!(
            mate::judge(&pos, &common::line("c1d1 a5b6")),
            mate::Verdict::Refuted {
                reason: mate::Reason::Illegal(a),
                ..
            } if a == common::a("a5b6")
        ),
        "a5b6 needs Black to have just played b7b5; Black need not oblige"
    );
}

// ---------------------------------------------------------------------------
// Coverage gaps in the existing fixtures
// ---------------------------------------------------------------------------

/// Every other fixture is White-to-move. Nothing in the suite would catch a
/// solver that assumed the solver is White.
#[test]
fn solves_a_position_where_black_is_the_solver() {
    use shakmaty::Position as _;

    let pos = common::pos(common::BACK_RANK_BLACK);
    assert_eq!(pos.turn(), shakmaty::Color::Black, "fixture sanity");
    assert_eq!(mate::find_linear(&pos, 1), Some(common::line("a8a1")));
    assert_eq!(
        mate::judge(&pos, &common::line("a8a1")),
        mate::Verdict::Mates { moves: 1 }
    );
    assert_eq!(mate::min_depth(&pos, 4), Some(1));
}

#[test]
fn finds_a_smothered_mate() {
    let pos = common::pos(common::SMOTHERED);
    assert_eq!(mate::find_linear(&pos, 1), Some(common::line("g5f7")));
}

/// King and a minor piece cannot mate a lone king, at any depth. The search must
/// bottom out at `None` rather than mistake the resulting draws for a win.
#[test]
fn no_mate_with_insufficient_material() {
    for fen in [
        "8/8/8/4k3/8/8/8/4K2B w - - 0 1", // K+B vs K
        "8/8/8/4k3/8/8/8/4K2N w - - 0 1", // K+N vs K
    ] {
        let pos = common::pos(fen);
        assert_eq!(mate::find_linear(&pos, 3), None, "{fen}");
        assert_eq!(mate::min_depth(&pos, 3), None, "{fen}");
    }
}

/// [`mate::find_linear`] and [`mate::judge`] must agree on the awkward positions
/// too, not just the plain ones.
///
/// `tests/mate.rs::search_and_judge_agree` makes this check over the original
/// fixtures; the promotion, castling and en-passant positions are precisely where
/// the two could drift apart, because they are where an arrow and a move are not
/// the same thing.
#[test]
fn search_and_judge_agree_on_special_moves() {
    for fen in [
        common::UNDERPROMOTION,
        common::CASTLING_MATE,
        common::EN_PASSANT_MATE,
        common::SMOTHERED,
        common::BACK_RANK_BLACK,
    ] {
        let pos = common::pos(fen);
        for depth in 1..=3 {
            if let Some(line) = mate::find_linear(&pos, depth) {
                assert_eq!(
                    mate::judge(&pos, &line),
                    mate::Verdict::Mates { moves: line.len() },
                    "search found {line:?} at depth {depth} but judge rejects it: {fen}"
                );
            }
        }
    }
}
