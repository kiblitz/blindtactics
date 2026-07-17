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
/// only when a capture can actually be played there. All three modes round-trip
/// (shakmaty normalises an unusable square away on parse), so this is a choice
/// about *canonical form*, not correctness — which is exactly why it belongs here
/// once rather than at each call site, where two authors would pick differently
/// and the database would carry both spellings of the same position.
///
/// `Legal` is the one that matches [`crate::roster`], which announces the square
/// under the same rule. That keeps a property worth having: the stored FEN holds
/// exactly the state the user is told about — nothing hidden, nothing announced
/// that is not there. `Always` would preserve dead en-passant squares, which the
/// raw Lichess FEN carries routinely and which no user could ever use.
pub fn to_fen(pos: &shakmaty::Chess) -> String {
    shakmaty::fen::Fen::from_position(pos, shakmaty::EnPassantMode::Legal).to_string()
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
