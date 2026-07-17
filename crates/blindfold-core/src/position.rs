//! Turning a FEN into a legal position.
//!
//! Its own module rather than part of [`crate::puzzle`], because parsing a FEN has
//! nothing to do with puzzles. The curation tool in particular must parse the raw
//! *Lichess* FEN, which is explicitly not a puzzle FEN — the position shown to the
//! user is the one after Lichess's setup move has been applied.

/// Why a FEN could not be turned into a legal position.
#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("could not parse FEN `{fen}`: {message}")]
    Parse { fen: String, message: String },
    #[error("FEN `{fen}` is not a legal position: {message}")]
    Illegal { fen: String, message: String },
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
