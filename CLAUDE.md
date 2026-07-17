# CLAUDE.md — Blindfold Chess Trainer context for AI assistants

This file is the load-bearing context for any AI working on this codebase. Read it
before doing anything else. Update it when load-bearing facts change.

Global rules in `~/.claude/CLAUDE.md` still apply (module-qualified imports, no
redundant module-name prefixes, `[Claude]` commit prefix, 11-agent review protocol
before every push). This file records only what is *specific to this project*.

## What is this?

A blindfold chess tactics trainer. The user is shown an **empty board** and a
**roster of piece locations** (visually: a knight icon next to "d5"). They must find
a forced mate without ever seeing the pieces on the board.

Input: the user drags across the blank board to draw **numbered arrows**, one per move
of their own side. When they hit submit, the app plays the line out. If it mates, the
board is revealed.

## The single most important design constraint

**The arrow UI is linear. It cannot express a branch.**

The user commits to a fixed sequence of their own moves *before* seeing any opponent
reply. So a puzzle is only usable if the user's move sequence works **regardless of how
the opponent defends**. This one constraint drives the entire database design.

Definition — a puzzle is **linear** iff there exists a sequence of solver moves
`m1..mk` such that, for *every* legal opponent defense, each `m_i` is legal when it is
the solver's turn and checkmate is delivered by `m_k` at the latest.

Note what this does *not* require: the opponent may have many legal defenses. Branching
is fine as long as it is **invisible to the user** — the same arrows mate against all of
them. (A stricter tier, where the opponent has exactly one legal move at every turn, was
considered and rejected: it is a subset of linear, and it would make mate-in-4 nearly
empty. See "Decisions" below.)

## Decisions already made — do not re-litigate

These were explicitly decided by the user. Do not reopen without asking.

- **Linear-only filtering.** Not "strict" (opponent forced to one legal move), not both
  tiers. Just linear, as defined above.
- **shakmaty + GPL-3.0.** shakmaty is Lichess's own chess library and is
  GPL-3.0-or-later, which forces this project to be GPL-3.0-or-later too. The user
  accepted this tradeoff over a permissive library with weaker SAN/UCI support.
- **Small database for now.** ~100 puzzles per mate depth (~400 total). Deliberately
  small so we can iterate on the app first and scale the DB later. The curation tool
  can regenerate a bigger set on demand.
- **Mates only.** Mate in 1, 2, 3, 4. No other tactical motifs.
- **Leptos CSR, all-Rust, static site.** Decided by the AI when the user deferred
  ("do whatever you think is best"), on this reasoning: the user asked for "a web app
  written in Rust"; the app has no backend needs (no accounts, no persistence, no shared
  state); a static WASM site validates submissions instantly and offline and deploys to
  GitHub Pages with no server to run. The user's other project (`dexterity`) is Axum +
  hand-written vanilla JS — we keep its *conventions* but not its server, because
  dexterity is multi-user and real-time and this is neither.
- **Audio is designed-for, not built yet.** The roster is modeled as structured data,
  never a display string, so text / SVG / speech all render from one source.

## Data source: the Lichess puzzle database

- License **CC0**. Download: `https://database.lichess.org/lichess_db_puzzle.csv.zst`
- **6,057,357 lines** in the 2026-07-05 dump (1 header + 6,057,356 puzzles). Verified
  two ways: python-zstandard, and our own reader in `dump.rs::the_real_dump_reads_every_line`.
- Columns: `PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes,GameUrl,OpeningTags`
- **The file is 302,111,223 bytes.** A `curl` that returns 197 KB and exits **0** is a
  thing that happened here. Check the size, and resume with `-C -`.

**CRITICAL SEMANTICS — two separate traps, and the second one is worse:**

> **Trap 1.** The `FEN` column is the position **before** the opponent's setup move.
> `Moves[0]` is that setup move. You apply it to the FEN, and *the resulting position*
> is what the player sees.
>
> **Trap 2.** `Moves[1..]` is the **whole line, alternating** — it is *not* a list of
> the user's moves. Only the **odd indices** are theirs. Depth is `Moves.len() / 2`,
> not `Moves.len() - 1`.

```text
Moves:  e5f6   e8e1    g1f2     e1f1        (real mateIn2 row, 000Zo)
        setup  SOLVER  defence  SOLVER
        [0]    [1]     [2]      [3]
```

Trap 2 is the dangerous one because it fails *quietly*: taking `Moves[1..]` wholesale
gives a line that is legal-looking and mostly the right length, so it produces puzzles
rather than errors. `lichess::of_row` uses `.step_by(2)` for exactly this, and
`tests/lichess.rs::only_every_other_move_is_the_users` pins it against real rows.

So the side the user plays is the side to move **after** `Moves[0]` is applied — i.e.
the *opposite* of the side to move in the raw FEN.

**The dump is not a single zstd frame.** Byte 0 begins a 12-byte *skippable* frame;
the real data starts at byte 12. `ruzstd::StreamingDecoder` decodes exactly one frame
and fails on this with `SkipFrame { magic_number: 0x184D2A50 }`. `curate`'s `dump`
module walks frames properly instead of seeking 12 bytes in, because a `seek(12)` plus
single-frame decoder would **silently truncate** a multi-frame archive: it would report
clean EOF at the end of frame one and curate a smaller database with no error anywhere.
Do not "simplify" it back.

**Why we cannot trust the Lichess data and must re-verify every puzzle ourselves.**
Both of these were confirmed at source level in the `lila` / `lichess-puzzler` repos, not
inferred:

1. **Lichess accepts any mating move as correct, and deliberately waives uniqueness for
   mate puzzles.** `ui/puzzle/src/moveTest.ts` returns `'win'` for any move whose SAN ends
   in `#` *before* it compares against the stored solution. And `ui/puzzle/src/report.ts`
   skips its multiple-solution detector for any puzzle whose themes contain `mate`, with
   the literal comment `// do not check, checkmate puzzles`. So a `mateInN` puzzle may
   have several distinct mating solutions, by design.
2. **The puzzle line encodes exactly one opponent reply.** In `generator.py`'s `cook_mate`,
   the defender's move is a single `engine.play()` call at depth 15 — no multipv, no
   branching. The tag tells us nothing about whether the line is linear.

Therefore the `mateInN` theme tag is used **only as a cheap prefilter** to shrink the
candidate pool. Every surviving puzzle is then re-proved from scratch by our own solver.

Approximate pool sizes (sampled, ±1pp — no per-theme stats are published):
`mateIn1` ~845k, `mateIn2` ~824k, `mateIn3` ~162k, `mateIn4` ~32k. Mate-in-4 is the tight
one; everything else has room to filter hard.

## Architecture

**`blindfold-web` does not exist yet.** Everything marked `(planned)` below is design
intent, not something you can go read. Do not cite it as though it were built.

```
blindfold-chess-trainer/
  crates/
    blindfold-core/      Pure logic. No WASM, no DOM, no I/O. The testable heart.
    blindfold-curate/    Offline CLI: lichess csv.zst -> database/*.jsonl
    blindfold-web/       (planned) Leptos CSR app. Thin. Rendering only.
  database/              The curated puzzle subset, committed to the repo.
```

### Current status

- `blindfold-core` — built, 109 tests, clippy clean.
- `blindfold-curate` — built, 18 tests + 1 `#[ignore]`d. Streams the dump, re-proves
  every candidate, writes `database/*.jsonl`. The ignored one needs the 300 MB dump:
  `BLINDFOLD_DUMP=<path> cargo test -p blindfold-curate -- --ignored`.
- `blindfold-web` — **not started. This is the next thing to build.**
- `database/` — **400 puzzles, 100 per depth**, curated from the 2026-07-05 dump and
  committed. Every one is re-proved by `crates/blindfold-curate/tests/database.rs`.
- CI — **does not exist.** There is no `.github/`. Wherever this file says a check
  "runs in CI", read it as "is intended to, once CI exists".

**`blindfold-curate` has both a lib and a bin target.** The lib is not there for
reuse — nothing else links it — it exists so `tests/` can reach `constants`, `select`,
and `dump`. An integration test cannot import a *binary* crate's modules, so without
it `constants::PER_DEPTH` and the database test's idea of how many puzzles a file
holds would be two numbers with nothing keeping them in step.

**The load-bearing architectural rule:** `blindfold-core` has no dependency on `web-sys`,
`leptos`, or any I/O. It is tested with plain native `cargo test` — instant, no browser,
no toolchain. Anything that can live in core, must. The web crate should contain almost
no logic worth testing.

This matters three ways: the test suite stays fast; the same solver is to be shared by the
offline curation tool and the live app, once they exist (so that the DB and the app cannot
disagree about what "solved" means); and it insulates us from frontend framework churn.

### `blindfold-core` modules

- `arrow` — the user's unit of input, `(from, to, promotion)`. **Read this module first**;
  the decision to make arrows rather than moves the unit of identity explains most of the
  rest of the design.
- `mate` — `judge` (does this line mate against every defense?), `playback` (what the UI
  animates on a solve), and `find_linear` / `min_depth` (search). The heart of the project.
- `roster` — piece locations as **structured data** (`roster::Entry { role, squares }`),
  ordered K/Q/R/B/N/P, **plus castling rights and the en-passant square**. Renders to text,
  to SVG, and later to speech. Never a string.
- `puzzle` — the `Puzzle` model, JSONL load/save, and `verify()`.
- `position` — FEN <-> legal position. Its own module because parsing a FEN has nothing to
  do with puzzles, and the curation tool must parse the *raw Lichess* FEN, which is
  explicitly not a puzzle FEN. `to_fen` commits to `EnPassantMode::Legal`, so the stored FEN
  names an ep square exactly when `roster` announces one. **`Chess`'s `PartialEq` ignores
  both the ep square (unless a capture is legal) and the clocks** — it is blind to the very
  field `to_fen` chooses about, so tests of `to_fen` must compare FEN *strings*. A test
  phrased as `Chess == Chess` here is vacuous, and one already was.
- `constants` — named constants. Per the global rule, they live here rather than inline.

There is deliberately **no `attempt` module**. Validating a submission is exactly
`mate::judge(&position, &submitted_arrows)` — the same call the curation tool will make. A
wrapper would only add a layer that could drift.

Text rendering lives in `roster` (core) rather than the web crate because two consumers
share it — plain text and, later, speech. SVG rendering belongs to the web crate, which is
its only consumer. That is the line; it is not "no strings in core".

### The roster must carry everything that decides the answer

Not a detail — the property the product rests on. The user cannot look at the board, so
anything the roster omits is not a presentational gap: it makes the puzzle **unsolvable**
and then marks a correct answer wrong.

This was caught late, and the way it hid is worth remembering. `Roster` was placement plus
`to_move`, which drops **castling rights** and the **en-passant square** — and both decide a
mate in our own fixtures. `EN_PASSANT_MATE` and the same position without the ep square
rendered *byte-identical* rosters and had different answers. The tests did not catch it
because `mate_edge_cases.rs` builds those two fixtures to prove the *solver* handles
castling and en passant, while `tests/roster.rs` never touched either: the solver's reach
and the roster's reach were each tested thoroughly, and never against each other.
`roster_distinguishes_positions_whose_answers_differ` is what holds this now.

En passant is read with `EnPassantMode::Legal`, so a square nobody can capture on is not
announced. Sound because ep rights expire after one ply — a square with no legal capture
now can never matter later.

**The roster is now complete, and this is checked rather than assumed.** A chess position is
placement + turn + castling + ep + halfmove clock + fullmove number. The roster carries the
first four, and the last two provably cannot matter: **shakmaty implements neither the
50-move rule, the 75-move rule, nor repetition.** `is_checkmate()` is `!checkers().is_empty()
&& legal_moves().is_empty()` and never reads `halfmoves`; `Chess` stores no history, so
repetition is not even representable; `halfmoves` exists only to round-trip a FEN. Verified
at source in shakmaty 0.30.1 and empirically (clock set to 0/49/99/100/149/150/200 across
fixtures — `find_linear` and `outcome` identical at every value). So `judge`, `find_linear`
and `min_depth` are functions of exactly the four things the roster announces. A collision
hunt over 217k positions found 79 equal-roster pairs and **zero** with different answers;
every pair differed only in the two clocks.

Two consequences worth not re-deriving. First, `text()` was incidentally shown injective over
217k rosters. Second, shakmaty has no 50-move rule, so a long enough all-quiet line hands a
real defender a **claimable** draw our solver cannot see. Two different thresholds, and this
paragraph has now been wrong about them twice, each time by reaching for the arithmetic
before the rule:

- The **automatic** draw (halfmove 150) is genuinely unreachable. Lichess auto-draws at the
  50-move rule, so source clocks cap near 99, and a mate-in-4 is ≤7 plies: 99 + 7 = 106.
- The **claimable** draw (halfmove 100) is reachable, from a source clock of **94** or more,
  with every ply quiet — a pawn move or capture zeroes the clock.

**Why 94 and not 93.** `93 + 7 = 100` is the tempting sum and it is the wrong one: it counts
the clock *reaching* 100, but ply 7 is the *solver's mating move*, and mate ends the game
(FIDE 5.1.1) while a 50-move draw must be **claimed by the player having the move** (FIDE
9.3). The defender only has the move at plies 2, 4 and 6 — clocks `C+1`, `C+3`, `C+5` — so the
binding ply is their last turn, not the mate. From `C = 93` the defender is on move at 94, 96,
98 and can claim nothing, while the mate lands exactly on 100. From `C = 94` they reach 99 and
may declare a move making it 100, which is claimable under 9.3(a). (9.3(b), where the clock is
already at 100 on their turn, needs `C = 95`.)

So: rare, not impossible — and a mate the defender can simply decline to lose is not a mate.

Cheap to close: have the curation tool reject candidates whose halfmove clock is high enough
to matter. Do it there rather than in `judge`, which must stay a pure function of the four
things the roster carries — see the `to_fen` note about `Chess::eq` ignoring the clocks.

### Why arrows, not `shakmaty::Move`s

A `Move` carries the moving role and whether it captured; those are position-dependent.
The same drag is a quiet slide against one defense and a capture against another. Since
the user commits to their line before seeing any reply, the thing they commit to is the
drag. Defining linearity over `Move` equality would wrongly reject valid puzzles.
`tests/arrow.rs::one_arrow_resolves_to_different_moves_in_different_positions` pins this.

### Why we play out rather than compare

If a puzzle has two distinct mating lines and we string-compared, a user who found the
*other* mate would be told they were wrong — a serious failure in a blindfold trainer
where they cannot see that they were right. Playing out is semantically correct and makes
the multiple-solution question moot. Note this means **duals are harmless** and we do not
filter for uniqueness. Minimality (`min_depth`) is still enforced, because that is about
*labelling*: a mate-in-2 must not be advertised as a mate-in-4.

### shakmaty gotchas that cost real time

- **`Move::to()` for a castle returns the ROOK's square, not the king's.** Internally a
  castle is `Castle { king: E1, rook: H1 }`. So raw `to()` gives the king-takes-rook form.
  Use `to_uci(CastlingMode::Standard)` to get the king's travel (`e1g1`).
  `UciMove::to_move` accepts *both* spellings, which is why `arrow::resolve` just
  delegates to it instead of hand-rolling a mapping.
- **`Role::from_char('k')` returns `Some(King)`.** It is not a promotion validator;
  `arrow`'s parser filters against `PROMOTABLE` explicitly.
- **MSRV is 1.95** (and shakmaty is edition 2024), which is why this repo pins toolchain
  1.97 while sibling projects sit on 1.94.
- **`magics`** (694 KiB of attack tables) is a default-on shakmaty feature. Off by default
  here, re-enabled by the native curation tool via `blindfold-core`'s `magics` feature, so
  the wasm bundle does not carry it.
- `find_linear(pos, d)` means "at most `d`", not "exactly `d`" — when every defense is
  already mated the recursion bottoms out early and returns a shorter line. `min_depth`
  iteratively deepens for exactly this reason.
- **`UciMove::to_move` ignores the promotion suffix on an en-passant capture.**
  `Move::EnPassant { from, to }` has no promotion field to put it in, so `e5d6q` and `e5d6`
  resolve to the same move while comparing unequal as `Arrow`s.
  `arrow::resolve` rejects a suffix on a non-back-rank target to keep the identity honest.
- **Cargo's `[profile.dev.package."*"]` matches dependencies only, never workspace
  members.** Each member needs its own stanza or it stays at opt-level 0 — worth 3.5x on
  the mate search.

## Cost, and the bounds that exist because of it

Both the judge frontier and the search tree grow **~30x per ply**. Everything below is
measured, not guessed:

- `judge` on a real mate-in-4: ~14 µs. Instant in-browser, ~1.4 CPU-seconds over a 1M pool.
- `min_depth(pos, 4)`: ~1 second per position. The **deepest iteration is ~97% of the
  cost**, which is why `verify` searches `depth - 1` and not `depth` — `judge` has already
  proved a mate at `depth`, so searching it again is pure waste (measured 42x).
- `min_depth(pos, 8)` on **two bare kings** does not finish. Depth is therefore clamped to
  `constants::MAX_DEPTH` before any search, because `Puzzle::depth` arrives from untrusted
  JSON.
- An unrefuted line reaches ~30M branches (~5 GiB) in about six seconds, past wasm32's
  4 GB address space. Hence `constants::MAX_FRONTIER` and `Verdict::TooComplex`.
- **Do not size `MAX_FRONTIER` by `bound * size_of::<Branch>()`.** That product has been
  wrong twice, both times in this file's favour. `judge` keeps the old frontier alive while
  the new one doubles its way up, so the true peak is well over twice the flat figure: the
  old `1 << 20` penciled out to ~150 MB and measured **527 MB**. It is now `1 << 18`,
  measured at 97 MB by `examples/frontier_memory.rs` — rerun that and update the constant's
  doc if you touch the bound, `Branch`, or the frontier advance. Headroom here is free; wasm
  linear memory never shrinks back, so overshoot is not.
- **`1 << 18` does not reject legitimate work — do not "restore" it on a hunch.** Keep the
  two halves of the argument apart; an earlier draft ran them together and overclaimed.
  *Proven:* only three plies of a solution generate a frontier (the last arrow is `is_last`
  and pushes nothing; `MAX_DEPTH` is 4), and growth on `UNBOUNDED_FRONTIER` is
  `[30, 926, 29203, 933297, ...]`, so tripping the bound needs **5+ arrows** and no solution
  has them. *Empirical:* the third column's worst case is a measurement — sweeping the
  immune shape (black light-squared bishops vs an all-dark mating line) gives **63,308**,
  about 4x clear. *And it fails safe either way:* exceeding the bound is `TooComplex`, not
  `Refuted`, so no user is told they were wrong; `verify` demands `Mates`, so such a puzzle
  never reaches the database, so the app is never asked to judge one. That only holds while
  curation and the app read the **same** constant.

Prunings deliberately NOT added, with reasons:

- **Checks-only before the final ply — unsound.** Misses quiet keys and zugzwang. A false
  positive here ships a broken puzzle. Never add it.
- **Transposition table for iterative deepening — pointless.** The shallow iterations sum
  to ~2.4% of the deepest one.
- **Move ordering — pointless here.** `min_depth` is dominated by iterations that must
  prove *no* mate exists, which are exhaustive by nature.
- **Dedup in `judge` — measured net loss** (~68% slower). One arrow per ply means no
  candidate loop to amortize the hashing over.
- **Dedup in `search` — worth ~1.14x at depth >= 3** (24-34% of ply-2/3 positions are
  transpositions at mate-in-4, and exactly 0% at mate-in-2). Not done yet; do it after the
  bigger wins if curation ever needs it.

## Known deferred work

- **`judge` and `search` duplicate the same frontier-advance algorithm** (flagged at 62 in
  review). They must stay semantically identical or the database and the app disagree;
  today that is held by `search_and_judge_agree` sampling rather than by construction. The
  suggested fix is to extract the advance generic over a `Trail` trait (`()` for search,
  `Vec<Arrow>` for judge, monomorphized away). Deferred, not rejected.
- **Put the Lichess row -> `Puzzle` conversion in core, not in `blindfold-curate`** (flagged
  at 55). CLAUDE.md calls the `FEN`-is-before-`Moves[0]` semantics the project's worst
  footgun, and the rule here is "anything that can live in core, must". That conversion is
  pure logic with no I/O, so on both counts it belongs in a `lichess` module in core, tested
  by the fast native suite — rather than hand-rolled at the call site in the one crate with
  no test culture yet. Do this **when building the curation tool**, not before; it is the
  first thing that tool should reach for.
- **Have the curation tool reject high-halfmove-clock candidates.** shakmaty has no 50-move
  rule, so an all-quiet line from a source clock of **94+** gives the defender a claimable draw
  the solver cannot see — rare rather than impossible, and a mate the defender can decline to
  lose is not a mate. Rejecting on the clock alone is the cheap filter and it is what to build;
  94 is derived in the roster-completeness section (and read the derivation before trusting the
  number — it is not `100 - 7`). Belongs in curation, not `judge`, which must stay a function
  of exactly what the roster carries.
- **`search` has no frontier bound while `judge` does** (flagged at 35). Currently safe:
  `MAX_DEPTH` caps `find_linear`'s frontier well under the bound, and the doc says not to
  hand it untrusted input. Worth closing anyway, since the two functions are documented as
  needing to agree and this asymmetry is guarded by a comment rather than by code.

## Testing

The user has been emphatic about this: aggressive testing is the point, not a chore. It
is what makes iterating on the UI safe.

- **Database invariant test** *(intended; neither `database/` nor CI exists yet)*. Every
  puzzle is to be re-proved via `Puzzle::verify()`: legal position, linear, mates in
  exactly the claimed depth, and minimal. That is what should stop a corrupt or
  mislabelled puzzle reaching the app.
- **Solver tests** against hand-built positions, each isolating one property. The
  fixtures live in `tests/common/mod.rs` and are documented individually — read them
  before adding more.
- Bug reported => reproduce as a failing test **first**, then fix. Same commit.

The two fixtures that matter most are `BRANCHING_LINEAR` and `BRANCHING_BLOCKED`: the same
shape, with Black's a7 pawn swapped for a rook and the c7 pawn dropped. The first is linear
despite five defenses; the second is not, because Black can interpose. Together they are
what "linear" does and does not mean.

Traps already encoded as tests (do not regress these):

- **Stalemate is a defender win.** Conflating "no legal moves" with "mated" is the classic
  mate-solver bug. `mate::Reason::Stalemate` exists so it can never be silently folded
  into `NoMate`.
- **"Only search checking moves" is unsound** at any ply but the last — it misses quiet
  keys and zugzwang. `finds_a_mate_whose_key_move_is_quiet` fails if anyone adds it.
- **`search_and_judge_agree`** — anything `find_linear` returns, `judge` must accept.
  If these drift, the app would present a puzzle whose own stored answer it rejects.

### A fixture-writing warning

Hand-built FENs are easy to get wrong, and a wrong fixture wastes a whole cycle. Two real
examples from this repo's history: a queen was placed on b1 and asked to reach c7 (not a
queen line), and a "pawn blocks the rook's file" idea was impossible in principle — a pawn
can only reach the b-file if it starts there, in which case the rook was already blocked at
move one. Blocking mid-line needs a piece that can change file. **Verify a fixture's claim
with a throwaway `examples/probe.rs` before building tests on it.**

## Where things live

- `crates/` — Rust workspace, one crate per logical concern.
- `database/` — *(planned)* curated puzzle JSONL, committed. To be regenerated with
  `blindfold-curate`.

There is no `docs/` directory. If design notes outgrow this file, create one.

## Measured facts worth not re-deriving

Linearity survival rate — the fraction of Lichess `mateInN` puzzles whose stored line is
actually linear. Sampled from ~59k real puzzles pulled from the live dump:

| depth | pool (approx) | linear (±1pp) | usable (approx) |
|---|---|---|---|
| mateIn1 | 845k | 100%* | 845k |
| mateIn2 | 824k | 93.8% | 773k |
| mateIn3 | 162k | 61.4% | 99k |
| mateIn4 | 32k | 34.8% | 11k |

\* Not a measurement. A mate-in-1 is linear by construction — the solver's single arrow is
the first ply, so no defense precedes it and there is nothing to branch. It cannot drift.
The other three are sample estimates; treat mateIn4 in particular as "about a third", not
as 34.8%.

This retires the one risk that could have invalidated the whole design: mate-in-4 is the
tight tier, and ~11k usable puzzles against a target of ~100 is ample.

## Working agreements with the user

- The user cares about code quality and readability over shipping hacks.
- The user wants notes committed here whenever a decision is "strongholded" so they
  never have to repeat themselves.
- Screenshots live in `C:\Users\Gabriel Lee\Pictures\Screenshots`.
