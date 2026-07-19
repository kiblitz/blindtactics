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
        "white to play. white: king g1. rook a1. black: king g8. pawns f7, g7, h7."
    );
}

// --- speech (the text-to-speech rendering) -----------------------------------

/// The read-aloud rendering upper-cases each square, so a speech engine reads the file
/// as its letter *name* ("gee one"), not the "ah two" a bare lower-case "a2" produces —
/// the whole reason `speech` is separate from `text`. Same wording and order; only the
/// squares change.
#[test]
fn speech_spells_files_as_letter_names() {
    let r = roster::of(&common::pos(common::BACK_RANK));
    assert_eq!(
        r.speech(),
        "white to play. white: king G1. rook A1. \
         black: king G8. pawns F7, G7, H7."
    );
}

/// Every square is upper-cased and kept glued — the file is the letter name, not the
/// article "a", and the rank stays fused to it so there is no pause between them (the
/// separation an earlier full-stop form introduced). The rank digit reads correctly on
/// its own.
#[test]
fn every_file_is_spelled_out() {
    for (square, spoken) in [
        (shakmaty::Square::A1, "A1"),
        (shakmaty::Square::B2, "B2"),
        (shakmaty::Square::C3, "C3"),
        (shakmaty::Square::D4, "D4"),
        (shakmaty::Square::E5, "E5"),
        (shakmaty::Square::F6, "F6"),
        (shakmaty::Square::G7, "G7"),
        (shakmaty::Square::H8, "H8"),
    ] {
        assert_eq!(roster::square_spoken(square), spoken);
    }
}

/// `speech` and `text` are the same announcement: they differ only in how a square is
/// spelled (upper-cased vs. bare), never in the words around it.
#[test]
fn speech_differs_from_text_only_in_the_squares() {
    let r = roster::of(&common::pos(common::BACK_RANK));
    assert!(r.text().contains("g1"), "plain text keeps bare coordinates");
    assert!(!r.speech().contains("g1"), "speech upper-cases them");
    assert!(r.speech().contains("G1"));
    assert!(r.text().starts_with("white to play."));
    assert!(r.speech().starts_with("white to play."));
}

#[test]
fn announces_the_side_to_move_first() {
    let r = roster::of(&common::pos(common::BACK_RANK_IDLE));
    assert_eq!(r.to_move, shakmaty::Color::Black);
    assert_eq!(
        r.text(),
        "black to play. black: king g8. pawns f7, g7, h7. white: king g1. rook a1.",
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
        "white to play. white: king e1. queen f4. rooks a1, c2. may castle queenside. \
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
        "white to play. white: king e1. rooks a1, h1. may castle either side. \
         black: king e8. rooks a8, h8. may castle either side."
    );
}

#[test]
fn announces_an_en_passant_square() {
    let r = roster::of(&common::pos(common::EN_PASSANT_MATE));
    assert_eq!(
        r.text(),
        "white to play. white: king c1. queen a6. pawn a5. black: king a1. pawn b5. \
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
        "white to play. white: king c1. black: king a1. pawn b5.",
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
    assert_eq!(pawns.text(), "pawns a6, b7, g5");
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

/// `squares` is the blindfold cost of a puzzle, and curation gates on it, so it has
/// to be the count of things the user is actually told — not of pieces, and not of
/// roster *entries*.
#[test]
fn squares_counts_every_occupied_square() {
    // Two kings, a rook, and three pawns.
    assert_eq!(
        roster::of(&common::pos(common::BACK_RANK_BLACK)).squares(),
        2 + 1 + 3
    );
    // The starting position: every piece still home.
    assert_eq!(
        roster::of(&common::pos(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        ))
        .squares(),
        32
    );
    // Two bare kings — the floor.
    assert_eq!(
        roster::of(&common::pos("4k3/8/8/8/8/8/8/4K3 w - - 0 1")).squares(),
        2
    );
}

/// The trap a naive implementation falls into: one `Entry` can hold many squares, so
/// counting entries counts *roles*. `UNBOUNDED_FRONTIER` has four black bishops in a
/// single entry — 4 entries but 7 squares.
#[test]
fn squares_counts_squares_not_roster_entries() {
    let r = roster::of(&common::pos(common::UNBOUNDED_FRONTIER));
    let entries: usize = [&r.white, &r.black].iter().map(|s| s.entries.len()).sum();
    assert_eq!(
        entries, 4,
        "K, B for white; k, and one bishops entry for black"
    );
    assert_eq!(
        r.squares(),
        7,
        "two kings, one white bishop, four black bishops"
    );
    assert_ne!(
        r.squares(),
        entries,
        "counting entries would undercount duplicates"
    );
}

/// Count algebraic squares in rendered roster text: a letter a-h followed by 1-8.
fn squares_named_in(text: &str) -> usize {
    text.as_bytes()
        .windows(2)
        .filter(|w| (b'a'..=b'h').contains(&w[0]) && (b'1'..=b'8').contains(&w[1]))
        .count()
}

/// `squares` must agree with what the roster actually reads out per side, since that
/// text is the only thing the user gets. A square announced but uncounted would let
/// curation ship a puzzle heavier than its gate allows.
#[test]
fn squares_matches_the_squares_named_per_side() {
    for fen in [
        common::BACK_RANK,
        common::BRANCHING_LINEAR,
        common::UNBOUNDED_FRONTIER,
        common::SMOTHERED,
        common::EN_PASSANT_MATE,
        common::CASTLING_MATE,
    ] {
        let r = roster::of(&common::pos(fen));
        let named: usize = [&r.white, &r.black]
            .iter()
            .map(|side| squares_named_in(&side.text()))
            .sum();
        assert_eq!(r.squares(), named, "roster for {fen}");
    }
}

/// The en-passant square is announced but deliberately not counted — it is not a
/// piece the user has to place. Pinning it so the exclusion is a decision rather than
/// an oversight: the full text names exactly one more square than `squares()` when
/// there is an ep square, and exactly as many when there is not.
#[test]
fn the_en_passant_square_is_announced_but_not_counted() {
    let with_ep = roster::of(&common::pos(common::EN_PASSANT_MATE));
    assert!(with_ep.en_passant.is_some(), "fixture has an ep square");
    assert_eq!(with_ep.squares(), 5, "two kings, queen, two pawns");
    assert_eq!(
        squares_named_in(&with_ep.text()),
        with_ep.squares() + 1,
        "the ep square is named on top of the pieces: {}",
        with_ep.text()
    );

    let without = roster::of(&common::pos(common::CASTLING_MATE));
    assert!(without.en_passant.is_none());
    assert_eq!(squares_named_in(&without.text()), without.squares());
}

/// Every word the roster can say about a role, pinned exhaustively.
///
/// `pluralizes_by_count` looks like it covers this and does not: its fixture has
/// a king, a rook and two knights, so ten of the twelve arms below are invisible
/// to it. Mutation testing found `name(Knight, false) -> "horse"` and
/// `name(Bishop, true) -> "XX"` both surviving the whole workspace — and the
/// singular arms are exactly the ones the promotion picker reads, since it calls
/// `name(role, false)` for each of `PROMOTABLE`. Half the picker's labels and
/// every one of its `aria-label`s had no coverage at all.
///
/// A table rather than a fixture, because this is a vocabulary: the point is that
/// all twelve are spelled out, which a position can never guarantee.
#[test]
fn every_role_has_a_singular_and_a_plural_name() {
    for (role, singular, plural) in [
        (shakmaty::Role::King, "king", "kings"),
        (shakmaty::Role::Queen, "queen", "queens"),
        (shakmaty::Role::Rook, "rook", "rooks"),
        (shakmaty::Role::Bishop, "bishop", "bishops"),
        (shakmaty::Role::Knight, "knight", "knights"),
        (shakmaty::Role::Pawn, "pawn", "pawns"),
    ] {
        assert_eq!(roster::name(role, false), singular);
        assert_eq!(roster::name(role, true), plural);
    }
}

/// The promotion picker labels its choices with `name(role, false)`, and the
/// roster announces the same word. This is the claim that justified extracting
/// `name` out of `Entry::name` in the first place, so it gets a test rather than
/// a comment.
#[test]
fn the_roster_and_the_promotion_picker_agree_on_what_a_piece_is_called() {
    // A position with one of every promotable role, so the roster must announce
    // each of them in the singular — the same arm the picker reads.
    let r = roster::of(&common::pos("4k3/8/8/8/8/8/8/RNBQK3 w - - 0 1"));
    for role in blindfold_core::constants::PROMOTABLE {
        let announced = r
            .white
            .entries
            .iter()
            .find(|e| e.role == role)
            .expect("one of each")
            .name();
        assert_eq!(
            announced,
            roster::name(role, false),
            "the roster and the picker must call a {role:?} the same thing"
        );
    }
}

/// `color_name` is the lower-case word in a sentence ("white to play"); `heading`
/// is the capitalized word at the head of a list ("White"). They must stay one
/// word apart — the same letters, capitalized — because the panel renders both
/// into the same DOM and a mismatch would read as two different colours.
#[test]
fn a_side_is_named_the_same_in_a_sentence_and_a_heading() {
    for color in shakmaty::Color::ALL {
        let sentence = roster::color_name(color);
        let heading = roster::heading(color);
        assert_eq!(
            heading.to_lowercase(),
            sentence,
            "the heading is the sentence word capitalized, nothing else"
        );
        let mut chars = heading.chars();
        assert!(
            chars.next().is_some_and(|c| c.is_uppercase()),
            "a heading leads with a capital, got: {heading}"
        );
        assert!(
            chars.all(|c| !c.is_uppercase()),
            "only the first letter is capitalized, got: {heading}"
        );
    }
    assert_eq!(roster::heading(shakmaty::Color::White), "White");
    assert_eq!(roster::heading(shakmaty::Color::Black), "Black");
}
