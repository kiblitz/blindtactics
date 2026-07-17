//! Turning a FEN into a legal position.
//!
//! Its own module rather than part of [`crate::puzzle`], because parsing a FEN has
//! nothing to do with puzzles. The curation tool in particular must parse the raw
//! *Lichess* FEN, which is explicitly not a puzzle FEN — the position shown to the
//! user is the one after Lichess's setup move has been applied.
//!
//! Both directions live here so the curation tool does not have to invent the
//! outbound one. It will apply Lichess's setup move and then need a FEN for
//! [`crate::puzzle::Puzzle::fen`], and writing one requires choosing an
//! [`shakmaty::EnPassantMode`] — a choice with no obviously right answer at the
//! call site and no reason to make twice. See [`to_fen`].

/// Why a FEN could not be turned into a legal position.
#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("could not parse FEN `{fen}`: {message}")]
    Parse { fen: String, message: String },
    #[error("FEN `{fen}` is not a legal position: {message}")]
    Illegal { fen: String, message: String },
}

/// Write a position back out as a FEN.
///
/// Uses [`shakmaty::EnPassantMode::Legal`], so the en-passant square is recorded
/// only when a capture can actually be played there.
///
/// Every mode round-trips, so this is a choice about *canonical form* rather than
/// correctness — which is exactly why it belongs here once rather than at each call
/// site, where two authors would pick differently and the database would carry both
/// spellings of the same position.
///
/// But mind *why* they all round-trip, because the obvious explanation is wrong and
/// the real one has teeth. Parsing does **not** normalise a dead en-passant square
/// away: `of_fen("… w - b6")` gives a position whose `ep_square(Always)` is still
/// `Some(B6)`. What happens is that `impl PartialEq for Chess` compares
/// `legal_ep_square()`, so it simply cannot see the difference. In other words
/// `Chess`'s equality is blind to precisely the field this function is choosing
/// about — so no test written in terms of `==` can check that choice, and one here
/// has to read the FEN's fourth field directly. `Chess::eq` ignores the halfmove and
/// fullmove clocks for the same reason.
///
/// `Legal` is the one that matches [`crate::roster`], which announces the square
/// under the same rule — so the FEN records an en-passant square exactly when the
/// user is told about one. `Always` would preserve dead en-passant squares, which
/// the raw Lichess FEN carries routinely and which no user could ever use.
///
/// Note the claim is about the *en-passant square only*, not about the FEN as a
/// whole: a FEN also carries the halfmove clock and fullmove number, which the
/// roster never announces. They are not a gap for the *solver* — shakmaty implements
/// neither the 50-move rule nor repetition, so no mate can turn on them (see
/// CLAUDE.md) — but they are written, so "the FEN holds exactly what the user is
/// told" would be too strong. The halfmove clock is read by exactly one caller,
/// [`halfmove_clock`], and never by `judge`.
pub fn to_fen(pos: &shakmaty::Chess) -> String {
    shakmaty::fen::Fen::from_position(pos, shakmaty::EnPassantMode::Legal).to_string()
}

/// The halfmove clock: plies since the last capture or pawn move.
///
/// Exposed for one reason, and it is not the solver's. shakmaty has no 50-move rule,
/// so `judge` cannot see a draw the defender could *claim* — and a mate the defender
/// can decline to lose is not a mate. Curation rejects candidates whose clock is high
/// enough for that to bite.
///
/// This deliberately does not live in `mate`. `judge` must stay a pure function of
/// exactly the four things the roster carries (placement, turn, castling, en passant),
/// because that is what makes a puzzle solvable from the roster alone.
pub fn halfmove_clock(pos: &shakmaty::Chess) -> u32 {
    shakmaty::Position::halfmoves(pos)
}

/// Parse a FEN into a legal position.
pub fn of_fen(fen: &str) -> Result<shakmaty::Chess, Error> {
    let parsed: shakmaty::fen::Fen =
        fen.parse()
            .map_err(|e: shakmaty::fen::ParseFenError| Error::Parse {
                fen: fen.to_owned(),
                message: e.to_string(),
            })?;
    parsed
        .into_position(shakmaty::CastlingMode::Standard)
        .map_err(
            |e: shakmaty::PositionError<shakmaty::Chess>| Error::Illegal {
                fen: fen.to_owned(),
                message: e.to_string(),
            },
        )
}
