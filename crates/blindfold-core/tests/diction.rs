//! Tests for spoken input — the most bug-prone feature in the project, so tested the
//! hardest. Two halves: `parse` (transcript → intent, position-free) and `resolve`
//! (intent + position → arrow / a question / illegal).
//!
//! The two grammar rules under test throughout: **extra information is never penalised**
//! (a full from-square, a spoken "takes"/"check"/"mate" — all accepted) and **missing
//! information is never penalised but never auto-resolved** (an ambiguous move asks
//! which piece rather than guessing or rejecting).

mod common;

use blindfold_core::diction;

fn parse(s: &str) -> diction::Intent {
    diction::parse(s)
}

/// Parse `phrase` and resolve it against `fen`, panicking if it was a command.
fn resolve(fen: &str, phrase: &str) -> diction::Resolution {
    diction::resolve(&diction::parse(phrase), &common::pos(fen))
        .expect("phrase should be a move, not a command")
}

// --- commands ----------------------------------------------------------------

#[test]
fn command_words_map_to_commands() {
    use diction::Command;
    for (phrase, command) in [
        ("submit", Command::Submit),
        ("go", Command::Submit),
        ("done", Command::Submit),
        ("undo", Command::Undo),
        ("back", Command::Undo),
        ("take back", Command::Undo),
        ("clear", Command::Clear),
        ("reset", Command::Clear),
        ("next", Command::Next),
        ("skip", Command::Next),
        ("next puzzle", Command::Next),
        ("repeat", Command::Repeat),
        ("again", Command::Repeat),
        ("give up", Command::GiveUp),
        ("resign", Command::GiveUp),
        ("show me the solution", Command::GiveUp),
    ] {
        assert_eq!(
            parse(phrase),
            diction::Intent::Command(command),
            "`{phrase}` should be {command:?}"
        );
    }
}

#[test]
fn a_command_does_not_resolve_to_a_move() {
    let pos = common::pos("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    assert_eq!(diction::resolve(&parse("submit"), &pos), None);
    assert_eq!(diction::resolve(&parse("gibberish nonsense"), &pos), None);
}

// --- parsing a move ----------------------------------------------------------

fn a_move(
    role: Option<shakmaty::Role>,
    from_file: Option<shakmaty::File>,
    from_rank: Option<shakmaty::Rank>,
    to: shakmaty::Square,
    promotion: Option<shakmaty::Role>,
) -> diction::Intent {
    diction::Intent::Move(diction::Move {
        role,
        from_file,
        from_rank,
        to,
        promotion,
    })
}

#[test]
fn a_plain_piece_move() {
    assert_eq!(
        parse("knight f6"),
        a_move(
            Some(shakmaty::Role::Knight),
            None,
            None,
            shakmaty::Square::F6,
            None
        )
    );
}

#[test]
fn a_bare_square_is_a_pawn_move() {
    // Algebraic notation omits the piece letter for a pawn.
    assert_eq!(
        parse("e4"),
        a_move(None, None, None, shakmaty::Square::E4, None)
    );
}

#[test]
fn a_glued_and_a_spaced_coordinate_parse_the_same() {
    let glued = parse("knight f6");
    assert_eq!(glued, parse("knight f 6"));
    assert_eq!(glued, parse("knight f six"));
    // ...and the file spoken as its letter name.
    assert_eq!(glued, parse("knight eff six"));
}

#[test]
fn recogniser_homophones_are_understood() {
    // "night" for knight, "rock" for rook — what a speech recogniser actually returns.
    assert_eq!(
        parse("night f6"),
        a_move(
            Some(shakmaty::Role::Knight),
            None,
            None,
            shakmaty::Square::F6,
            None
        )
    );
    assert_eq!(
        parse("rock a1"),
        a_move(
            Some(shakmaty::Role::Rook),
            None,
            None,
            shakmaty::Square::A1,
            None
        )
    );
}

#[test]
fn a_from_file_disambiguates() {
    // "rook g f8" — the earlier file is the disambiguator, the last file+rank the target.
    assert_eq!(
        parse("rook g f8"),
        a_move(
            Some(shakmaty::Role::Rook),
            Some(shakmaty::File::G),
            None,
            shakmaty::Square::F8,
            None
        )
    );
}

#[test]
fn a_from_rank_disambiguates() {
    // "rook 1 a3" — the earlier rank disambiguates.
    assert_eq!(
        parse("rook 1 a3"),
        a_move(
            Some(shakmaty::Role::Rook),
            None,
            Some(shakmaty::Rank::First),
            shakmaty::Square::A3,
            None
        )
    );
}

#[test]
fn a_full_from_square_is_accepted() {
    // Extra information: naming the whole from-square when it was not needed.
    assert_eq!(
        parse("knight g1 f3"),
        a_move(
            Some(shakmaty::Role::Knight),
            Some(shakmaty::File::G),
            Some(shakmaty::Rank::First),
            shakmaty::Square::F3,
            None
        )
    );
}

#[test]
fn a_trailing_piece_is_a_promotion() {
    // "e8 queen" — the role *after* the destination promotes; the mover is a pawn.
    assert_eq!(
        parse("e8 queen"),
        a_move(
            None,
            None,
            None,
            shakmaty::Square::E8,
            Some(shakmaty::Role::Queen)
        )
    );
    // "promotes to" is filler around the same thing.
    assert_eq!(parse("e8 promotes to queen"), parse("e8 queen"));
    // ...and an explicit pawn with an explicit promotion piece.
    assert_eq!(
        parse("pawn e8 knight"),
        a_move(
            Some(shakmaty::Role::Pawn),
            None,
            None,
            shakmaty::Square::E8,
            Some(shakmaty::Role::Knight)
        )
    );
}

#[test]
fn a_leading_piece_moves_it_is_not_a_promotion() {
    // "queen e8" is the queen moving to e8, not a pawn promoting — decided purely by the
    // role coming before the destination rather than after.
    assert_eq!(
        parse("queen e8"),
        a_move(
            Some(shakmaty::Role::Queen),
            None,
            None,
            shakmaty::Square::E8,
            None
        )
    );
}

#[test]
fn extra_words_are_never_penalised() {
    // "takes", "check", "mate" are all accepted and dropped — the move is the same.
    let plain = parse("queen h5");
    assert_eq!(plain, parse("queen h5 mate"));
    assert_eq!(plain, parse("queen takes h5"));
    assert_eq!(parse("rook f8"), parse("rook takes f8 check"));
}

#[test]
fn nothing_chess_shaped_is_unclear() {
    // No destination, no command word — refused rather than guessed into a move.
    assert_eq!(parse("hello there friend"), diction::Intent::Unclear);
    assert_eq!(parse(""), diction::Intent::Unclear);
}

// --- parsing a castle --------------------------------------------------------

#[test]
fn castles_parse_with_and_without_a_side() {
    let kingside = diction::Intent::Castle(Some(shakmaty::CastlingSide::KingSide));
    let queenside = diction::Intent::Castle(Some(shakmaty::CastlingSide::QueenSide));
    assert_eq!(parse("castle"), diction::Intent::Castle(None));
    assert_eq!(parse("castle kingside"), kingside);
    assert_eq!(parse("castles queenside"), queenside);
    assert_eq!(parse("short castle"), kingside);
    assert_eq!(parse("long castle"), queenside);
}

// --- parsing a whole line (streaming multi-move) -----------------------------

fn line(s: &str) -> diction::LineParse {
    diction::parse_line(s)
}

fn parsed(intents: Vec<diction::Intent>, trailing: bool) -> diction::LineParse {
    diction::LineParse { intents, trailing }
}

#[test]
fn a_single_move_is_a_one_element_line() {
    assert_eq!(line("knight f6"), parsed(vec![parse("knight f6")], false));
}

#[test]
fn several_moves_in_one_breath_each_become_a_move() {
    // The whole point: "queen f5 queen g6" is two moves, split at the second role because
    // it has a coordinate after it (a new mover, not a promotion).
    assert_eq!(
        line("queen f5 queen g6"),
        parsed(vec![parse("queen f5"), parse("queen g6")], false)
    );
    // The originally-reported case: two knight moves said back to back.
    assert_eq!(
        line("knight f7 knight f6"),
        parsed(vec![parse("knight f7"), parse("knight f6")], false)
    );
}

#[test]
fn an_incomplete_trailing_move_is_flagged_not_dropped() {
    // "knight f7 knight f" — the first move is complete and settled; the second is still
    // being spoken (a role and a file, no rank), so it is not an intent yet but `trailing`
    // says to keep listening.
    assert_eq!(
        line("knight f7 knight f"),
        parsed(vec![parse("knight f7")], true)
    );
}

#[test]
fn a_command_after_moves_is_its_own_segment() {
    // "queen f5 submit" streams the move, then the command — spoken hands-free in one go.
    assert_eq!(
        line("queen f5 submit"),
        parsed(
            vec![
                parse("queen f5"),
                diction::Intent::Command(diction::Command::Submit)
            ],
            false
        )
    );
}

#[test]
fn a_promotion_stays_one_move_in_a_line() {
    // "e8 queen" is a pawn promoting, not a pawn move then a stray "queen": the trailing
    // role has no coordinate after it, so it is a promotion and stays in the move.
    assert_eq!(line("e8 queen"), parsed(vec![parse("e8 queen")], false));
}

#[test]
fn a_promotion_mid_line_is_not_read_as_the_next_mover() {
    // "pawn g2 pawn g1 queen queen g3" — a pawn promotes on g1, then the new queen moves to
    // g3. The first "queen" is the promotion (no coordinate of its own before the next role);
    // the second "queen" is the mover of the next move. Read greedily, the first queen would
    // be taken as a mover to g3 and the promotion lost.
    assert_eq!(
        line("pawn g2 pawn g1 queen queen g3"),
        parsed(
            vec![parse("pawn g2"), parse("pawn g1 queen"), parse("queen g3")],
            false
        )
    );
}

#[test]
fn a_castle_in_a_line_is_split_from_the_next_move() {
    assert_eq!(
        line("castle kingside queen g6"),
        parsed(
            vec![
                diction::Intent::Castle(Some(shakmaty::CastlingSide::KingSide)),
                parse("queen g6"),
            ],
            false
        )
    );
}

#[test]
fn a_disambiguated_move_stays_a_single_move_in_a_line() {
    // "rook g f8" is one move with a from-file, not a move to g-something then f8 —
    // coordinates stick to the current move unless a new role intervenes.
    assert_eq!(line("rook g f8"), parsed(vec![parse("rook g f8")], false));
}

#[test]
fn nothing_chess_shaped_is_an_empty_line() {
    assert_eq!(line("hello there"), parsed(vec![], false));
    assert_eq!(line(""), parsed(vec![], false));
}

// --- resolving a move against a position -------------------------------------

const ONE_KNIGHT: &str = "4k3/8/8/8/8/5N2/8/4K3 w - - 0 1";
const TWO_KNIGHTS: &str = "4k3/8/8/8/8/8/3N1N2/4K3 w - - 0 1";
const PAWN_TO_PROMOTE: &str = "k4n2/4P3/8/8/8/8/8/4K3 w - - 0 1";
const BOTH_CASTLES: &str = "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1";
const ONE_CASTLE: &str = "4k3/8/8/8/8/8/8/4K2R w K - 0 1";

#[test]
fn an_unambiguous_move_resolves_to_one_arrow() {
    assert_eq!(
        resolve(ONE_KNIGHT, "knight e5"),
        diction::Resolution::Move(common::a("f3e5"))
    );
}

#[test]
fn a_move_no_piece_can_make_is_illegal() {
    assert_eq!(
        resolve(ONE_KNIGHT, "knight a1"),
        diction::Resolution::Illegal
    );
    assert_eq!(
        resolve(ONE_KNIGHT, "queen d4"),
        diction::Resolution::Illegal
    );
}

#[test]
fn extra_information_still_resolves() {
    // "takes"/"check" around a real move do not change the resolution.
    assert_eq!(
        resolve(ONE_KNIGHT, "knight takes e5 check"),
        diction::Resolution::Move(common::a("f3e5"))
    );
}

#[test]
fn an_ambiguous_move_asks_which_it_never_guesses() {
    // Both knights (d2 and f2) reach e4. The resolver must *ask*, listing the candidate
    // from-squares in board order — not silently pick one, which would hand over the
    // answer, and not reject, which would punish a legal intent.
    assert_eq!(
        resolve(TWO_KNIGHTS, "knight e4"),
        diction::Resolution::Ambiguous(vec![shakmaty::Square::D2, shakmaty::Square::F2])
    );
}

#[test]
fn a_disambiguator_resolves_the_ambiguity() {
    // The same position, now told which knight — by file, or by full from-square.
    assert_eq!(
        resolve(TWO_KNIGHTS, "knight d e4"),
        diction::Resolution::Move(common::a("d2e4"))
    );
    assert_eq!(
        resolve(TWO_KNIGHTS, "knight f2 e4"),
        diction::Resolution::Move(common::a("f2e4"))
    );
}

#[test]
fn a_promotion_without_a_piece_asks_which() {
    assert_eq!(
        resolve(PAWN_TO_PROMOTE, "e8"),
        diction::Resolution::NeedsPromotion(shakmaty::Square::E8)
    );
}

#[test]
fn a_promotion_with_a_piece_resolves() {
    assert_eq!(
        resolve(PAWN_TO_PROMOTE, "e8 queen"),
        diction::Resolution::Move(common::a("e7e8q"))
    );
    // A pawn capture-promotion, disambiguated by the pawn's own square.
    assert_eq!(
        resolve(PAWN_TO_PROMOTE, "e takes f8 knight"),
        diction::Resolution::Move(common::a("e7f8n"))
    );
}

#[test]
fn a_stray_promotion_on_a_non_promoting_move_is_dropped() {
    // "knight e5 queen" — the knight is not promoting, so the extra "queen" is ignored
    // rather than turned into an illegal promoting move.
    assert_eq!(
        resolve(ONE_KNIGHT, "knight e5 queen"),
        diction::Resolution::Move(common::a("f3e5"))
    );
}

// --- resolving a castle ------------------------------------------------------

#[test]
fn a_bare_castle_with_two_options_asks_which_side() {
    assert_eq!(
        resolve(BOTH_CASTLES, "castle"),
        diction::Resolution::NeedsCastleSide
    );
}

#[test]
fn a_named_castle_side_resolves_to_the_kings_travel() {
    // e1g1 / e1c1 — the king's two-square travel, the arrow a drag produces, not the
    // king-takes-rook spelling.
    assert_eq!(
        resolve(BOTH_CASTLES, "castle kingside"),
        diction::Resolution::Move(common::a("e1g1"))
    );
    assert_eq!(
        resolve(BOTH_CASTLES, "castle queenside"),
        diction::Resolution::Move(common::a("e1c1"))
    );
}

#[test]
fn a_bare_castle_with_one_option_resolves() {
    // Only kingside is legal, so "castle" needs no side.
    assert_eq!(
        resolve(ONE_CASTLE, "castle"),
        diction::Resolution::Move(common::a("e1g1"))
    );
}

#[test]
fn an_illegal_castle_side_is_illegal() {
    assert_eq!(
        resolve(ONE_CASTLE, "castle queenside"),
        diction::Resolution::Illegal
    );
}
