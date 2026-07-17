//! Proving that a fixed sequence of arrows forces mate against *every* defense.
//!
//! # What "linear" means and why it is the whole game
//!
//! A blindfold user draws their arrows and hits submit. They commit to their
//! entire line before seeing a single opponent reply. So a puzzle is only usable
//! here if the user's arrows work no matter how the opponent defends.
//!
//! A line is **linear** iff, for every legal opponent defense, each arrow is
//! legal when it is the solver's turn and mate arrives by the last arrow.
//!
//! Note what this deliberately permits: the opponent may have many legal
//! defenses. Branching is fine as long as it is *invisible to the user* — the
//! same arrows mate against all of it. Requiring the opponent to have exactly one
//! legal move would be a far smaller and duller set of puzzles.
//!
//! # Why we cannot take Lichess's word for any of this
//!
//! A Lichess puzzle line records exactly one opponent reply — a single
//! `engine.play()` call at generation time — even where the opponent has a dozen
//! legal defenses. And Lichess explicitly waives solution uniqueness for mate
//! puzzles (`ui/puzzle/src/report.ts` carries the comment `// do not check,
//! checkmate puzzles`, and `moveTest.ts` accepts *any* move whose SAN ends in
//! `#`). So the `mateInN` theme tag is a candidate filter and nothing more.
//! Everything here is re-proved from scratch.
//!
//! # Search soundness
//!
//! [`find_linear`] considers *all* solver moves at every ply, not just checks.
//! Restricting to checking moves is sound only at the final ply (mate implies
//! check) and unsound before it — it misses quiet keys and zugzwang mates
//! entirely. Since a false positive here ships a broken puzzle, the search stays
//! full-width.

use crate::arrow;
use shakmaty::Position as _;

/// The result of playing a line out against every defense.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// Mate against every defense, using `plies` of the submitted line.
    ///
    /// `plies` may be fewer than the line's length: if every defense is already
    /// mated, trailing arrows are simply never played.
    Mates { plies: usize },
    /// Some defense survives. `defense` is the opponent's replies that reach the
    /// refuting position, which is exactly what the UI should replay to show the
    /// user where their idea broke.
    Refuted {
        defense: Vec<arrow::Arrow>,
        reason: Reason,
    },
}

/// How a defense refuted a line.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Reason {
    /// The arrow was not a legal move in the position this defense reached.
    Illegal(arrow::Arrow),
    /// The line ran out and the opponent was not mated.
    NoMate,
    /// The opponent had no legal move but was not in check. A draw refutes a
    /// mate just as surely as a survival does — this is the classic mate-solver
    /// bug and it gets an explicit variant so it can never be conflated with
    /// checkmate.
    Stalemate,
}

impl Verdict {
    pub fn mates(&self) -> bool {
        matches!(self, Verdict::Mates { .. })
    }
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reason::Illegal(a) => write!(f, "arrow {a} is illegal against some defense"),
            Reason::NoMate => write!(f, "some defense survives the line"),
            Reason::Stalemate => write!(f, "some defense reaches stalemate"),
        }
    }
}

/// One live opponent defense: where it led, and how it got there.
struct Branch {
    pos: shakmaty::Chess,
    defense: Vec<arrow::Arrow>,
}

/// Play `line` out against every legal defense from `start`.
///
/// `start` must have the solver to move. This is the function that decides
/// whether a submitted attempt is correct, and it is the same one the curation
/// tool uses to decide whether a puzzle is admissible — so the database and the
/// app can never disagree about what "solved" means.
pub fn judge(start: &shakmaty::Chess, line: &[arrow::Arrow]) -> Verdict {
    if line.is_empty() {
        return Verdict::Refuted {
            defense: Vec::new(),
            reason: Reason::NoMate,
        };
    }

    let mut frontier = vec![Branch {
        pos: start.clone(),
        defense: Vec::new(),
    }];

    for (ply, &a) in line.iter().enumerate() {
        let is_last = ply + 1 == line.len();
        let mut next = Vec::new();

        for branch in frontier {
            let Ok(mv) = a.resolve(&branch.pos) else {
                return Verdict::Refuted {
                    defense: branch.defense,
                    reason: Reason::Illegal(a),
                };
            };

            let mut after = branch.pos.clone();
            after.play_unchecked(mv);

            if after.is_checkmate() {
                // This defense is finished. It contributes nothing to the next
                // frontier, which is how an early mate shortens the line.
                continue;
            }

            // Replies are computed before the `is_last` check on purpose: a line
            // whose last move stalemates must report `Stalemate`, not `NoMate`.
            // Both refute, but "you stalemated them" is a different mistake and
            // the user deserves to be told which one they made.
            let replies = after.legal_moves();
            if replies.is_empty() {
                return Verdict::Refuted {
                    defense: branch.defense,
                    reason: Reason::Stalemate,
                };
            }
            if is_last {
                return Verdict::Refuted {
                    defense: branch.defense,
                    reason: Reason::NoMate,
                };
            }

            for reply in replies.iter() {
                let mut child = after.clone();
                child.play_unchecked(*reply);
                let mut defense = branch.defense.clone();
                defense.push(arrow::Arrow::of_move(reply).expect("standard chess has no drops"));
                next.push(Branch {
                    pos: child,
                    defense,
                });
            }
        }

        if next.is_empty() {
            return Verdict::Mates { plies: ply + 1 };
        }
        frontier = next;
    }

    // The final iteration either refutes or empties the frontier, so it always
    // returns above.
    unreachable!("a non-empty line always resolves on its last ply")
}

/// Search for a linear mating line of **at most** `max_depth` solver moves.
///
/// The returned line's length is the depth it actually achieved, which may be
/// less than `max_depth`: once every defense is mated there is nothing left to
/// play. It is *not* necessarily the shortest such line — the first arrow tried
/// might mate in 2 in a position that also has a mate in 1 — so use [`min_depth`]
/// when the actual depth matters.
///
/// Returns the first line found. There may be others: two distinct linear mates
/// of the same length is a "dual". Duals are harmless for correctness, since
/// [`judge`] accepts any line that mates.
pub fn find_linear(start: &shakmaty::Chess, max_depth: usize) -> Option<Vec<arrow::Arrow>> {
    search(std::slice::from_ref(start), max_depth)
}

/// The shortest linear mate from `start`, searching up to `max` solver moves.
///
/// Iterative deepening rather than one deep search, precisely because
/// [`find_linear`] does not promise minimality. This is what stops a puzzle
/// advertised as mate-in-4 from secretly being a mate-in-2.
pub fn min_depth(start: &shakmaty::Chess, max: usize) -> Option<usize> {
    (1..=max).find(|&d| find_linear(start, d).is_some())
}

fn search(frontier: &[shakmaty::Chess], remaining: usize) -> Option<Vec<arrow::Arrow>> {
    if frontier.is_empty() {
        return Some(Vec::new()); // Every defense is already mated.
    }
    if remaining == 0 {
        return None;
    }

    // An arrow must be legal in every frontier position, so the moves of any one
    // of them are a superset of the candidates. Take the first as the generator
    // and let `resolve` reject the rest.
    let candidates: Vec<arrow::Arrow> = frontier[0]
        .legal_moves()
        .iter()
        .filter_map(arrow::Arrow::of_move)
        .collect();

    for a in candidates {
        let mut next = Vec::new();
        let mut viable = true;

        for pos in frontier {
            let Ok(mv) = a.resolve(pos) else {
                viable = false;
                break;
            };
            let mut after = pos.clone();
            after.play_unchecked(mv);

            if after.is_checkmate() {
                continue;
            }
            let replies = after.legal_moves();
            if replies.is_empty() {
                viable = false; // Stalemate: a draw refutes a mate.
                break;
            }
            if remaining == 1 {
                viable = false; // Had to mate now; did not.
                break;
            }
            for reply in replies.iter() {
                let mut child = after.clone();
                child.play_unchecked(*reply);
                next.push(child);
            }
        }

        if !viable {
            continue;
        }
        if let Some(rest) = search(&next, remaining - 1) {
            let mut line = vec![a];
            line.extend(rest);
            return Some(line);
        }
    }

    None
}
