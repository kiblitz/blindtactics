//! Where the pieces are — the only thing the blindfold user is told.
//!
//! Modelled as structured data rather than a display string, deliberately. The
//! same roster has to render three ways: as SVG piece icons beside coordinates,
//! as plain text, and (later) as speech — "white to play. white: king d5, bishop
//! b4 c6, pawns a6 b7 g5. black: king g7." Building a string here would force the
//! audio mode to parse it back apart.

use shakmaty::Position as _;

/// Every square holding a given role, for one side.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    pub role: shakmaty::Role,
    /// Sorted by file, then rank — `a6 b7 g5`, the order a person reads them out.
    pub squares: Vec<shakmaty::Square>,
}

/// One side's pieces, ordered king first down to pawns.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Side {
    pub color: shakmaty::Color,
    pub entries: Vec<Entry>,
}

/// The full piece placement, plus whose turn it is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Roster {
    pub to_move: shakmaty::Color,
    pub white: Side,
    pub black: Side,
}

/// Announcement order: the king anchors the position, then descending value.
/// `shakmaty::Role`'s own ordering runs the other way (pawn first), so it cannot
/// be used here.
const ANNOUNCE_ORDER: [shakmaty::Role; 6] = [
    shakmaty::Role::King,
    shakmaty::Role::Queen,
    shakmaty::Role::Rook,
    shakmaty::Role::Bishop,
    shakmaty::Role::Knight,
    shakmaty::Role::Pawn,
];

/// Read the roster out of a position.
pub fn of(pos: &shakmaty::Chess) -> Roster {
    Roster {
        to_move: pos.turn(),
        white: side_of(pos, shakmaty::Color::White),
        black: side_of(pos, shakmaty::Color::Black),
    }
}

fn side_of(pos: &shakmaty::Chess, color: shakmaty::Color) -> Side {
    let board = pos.board();
    let entries = ANNOUNCE_ORDER
        .iter()
        .filter_map(|&role| {
            let mut squares: Vec<shakmaty::Square> = (board.by_color(color) & board.by_role(role))
                .into_iter()
                .collect();
            if squares.is_empty() {
                return None;
            }
            squares.sort_by_key(|s| (s.file(), s.rank()));
            Some(Entry { role, squares })
        })
        .collect();
    Side { color, entries }
}

impl Roster {
    /// The side to move, then the other. Announcing the mover first is what a
    /// human does, and it is what the audio mode will want.
    pub fn sides_in_announce_order(&self) -> [&Side; 2] {
        match self.to_move {
            shakmaty::Color::White => [&self.white, &self.black],
            shakmaty::Color::Black => [&self.black, &self.white],
        }
    }

    /// Render for reading aloud or displaying as plain text.
    ///
    /// `"white to play. white: king d5, bishops b4 c6, pawns a6 b7 g5. black: king g7."`
    pub fn text(&self) -> String {
        let mut out = format!("{} to play.", color_name(self.to_move));
        for side in self.sides_in_announce_order() {
            out.push(' ');
            out.push_str(&side.text());
        }
        out
    }
}

impl Side {
    pub fn text(&self) -> String {
        let listed: Vec<String> = self.entries.iter().map(Entry::text).collect();
        format!("{}: {}.", color_name(self.color), listed.join(", "))
    }
}

impl Entry {
    /// `"pawns a6 b7 g5"`, or `"king d5"` for a lone piece.
    pub fn text(&self) -> String {
        let squares: Vec<String> = self.squares.iter().map(|s| s.to_string()).collect();
        format!("{} {}", self.name(), squares.join(" "))
    }

    /// The role's name, pluralized to match the number of squares.
    pub fn name(&self) -> &'static str {
        let plural = self.squares.len() > 1;
        match (self.role, plural) {
            (shakmaty::Role::King, _) => "king", // Only ever one.
            (shakmaty::Role::Queen, false) => "queen",
            (shakmaty::Role::Queen, true) => "queens",
            (shakmaty::Role::Rook, false) => "rook",
            (shakmaty::Role::Rook, true) => "rooks",
            (shakmaty::Role::Bishop, false) => "bishop",
            (shakmaty::Role::Bishop, true) => "bishops",
            (shakmaty::Role::Knight, false) => "knight",
            (shakmaty::Role::Knight, true) => "knights",
            (shakmaty::Role::Pawn, false) => "pawn",
            (shakmaty::Role::Pawn, true) => "pawns",
        }
    }
}

fn color_name(color: shakmaty::Color) -> &'static str {
    match color {
        shakmaty::Color::White => "white",
        shakmaty::Color::Black => "black",
    }
}
