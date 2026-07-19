# Browser test

One Playwright spec that drives the **built** app in a real browser. It exists for
the one class of bug native tests cannot see: reactive wiring.

The frozen-replay bug is the motivating case. The reveal's replay read `ply` with
`get_untracked`, so its effect fired once and stopped — the board took a single
ply and froze, still captioned "Mate." Every native test passed: `judge`,
`playback` and the pointer geometry were all correct. The fault was in the Leptos
wiring, and only a browser runs that. See CLAUDE.md, "The browser is the only
place some bugs exist". The reveal is now stepped by hand rather than by a timer,
but the same class of bug lives in the step wiring, which is what this spec drives.

## What it checks

Two specs, run as two Playwright projects: `reveal.spec.js` on a desktop viewport,
and `mobile.spec.js` on a phone viewport with touch. `reveal.spec.js` holds six tests
(the **Solve, step, and rate** one runs on two pinned puzzles):

- **Blind board.** Loads the app and confirms the board is a void with real piece
  artwork in the roster (no pieces on the board, no `board--revealed`).
- **Arrowhead colour.** Draws one arrow and asserts the head's computed `fill` equals
  the shaft's computed `stroke` (a resolved `rgb`, not the board's amber). Guards the
  bug where a shared `<marker>` could not inherit the per-arrow colour and painted
  every head amber — a browser-only rendering fault native tests could not see.
- **Solve, step, and rate.** The app serves one combined pool picked at random near
  the user's rating, so the test overrides `Math.random` to a fixed seed and clears
  the stored rating — pinning *exactly* which puzzle appears. This is run on two
  seeds chosen against the committed database's rating order: a mate-in-3 whose line
  includes a real promotion (so the picker's piece-choice is walked) and a mate-in-4
  whose first move originates on the lowest rank (the endpoint the viewport size
  exists to keep on-screen). Pinning matters because CI retries the whole test on failure — a
  random puzzle would let a puzzle-specific bug pass on the retry against an unrelated
  one. The test still reads *whichever* puzzle is on screen and looks up its recorded
  solution, so it cannot drift from what the seed selects. For each pinned puzzle it
  draws the solution by dragging on the blank board (setting the piece on the move's
  per-move promotion control where a move promotes), submits, and asserts:
  - the board reveals, the verdict says "Mate", and the mating square is lit;
  - the rating moves and the `+n` delta shows it;
  - stepping all the way back lands on the empty start (nothing lit) with pieces
    still shown, and one step forward re-lights the first move — the manual reveal
    actually moves, where the frozen replay would fail both halves;
  - the page logged no errors.
- **Last-rank non-pawn move.** Draws a move that matches promotion *geometry* but is
  not a pawn's (a straight drag up one file into the top row, which the solver-oriented
  board always makes a promotion-geometry move). Promotion is a per-move control in the
  line list, not a modal, so the move enters straight away as a plain move: it asserts
  the arrow lands, Submit is enabled (nothing is left unresolved behind a picker), and
  the move's promotion control sits at its "no promotion" default — the ~14% of puzzles
  whose key is a non-pawn move to the last rank depend on this staying enterable.
- **POV setting and flip.** Drives the real settings menu and flip button and reads the
  board's own top-left `data-square` to confirm the re-render happened: POV=White puts a8
  in the corner, POV=Black mirrors to h1, and flip inverts the current view. White/Black
  are puzzle-independent, so nothing is pinned. A reload then proves the design's split —
  the POV persisted to `localStorage`, the transient flip did not.
- **Give up and navigate the analysis.** On a pinned mate-in-4, presses **Give up** with
  nothing drawn and asserts the board reveals, the verdict names the concession, and the
  rating *drops* (giving up is a scored loss). Then it exercises the post-tactic SAN move
  list: clicking the first move jumps the board (the move highlights, its square lights),
  and the **arrow keys** step the reveal (left to the empty start, right re-lights the
  first move) — all reactive concerns a native test cannot see.

`mobile.spec.js` (the `mobile` project) asserts the phone-specific behaviour: the
two-column layout **stacks** (panels below the board), `touch-action` is `none`, the
whole board fits above the fold, and a **touch-type pointer drag** draws an arrow. The
reveal/step wiring is size-independent and not re-proved there.

The solutions come from all four committed `database/mate_in_*.jsonl` files, read
off disk, so the test cannot drift from what the app was built with. Shared helpers
(`collectErrors`, board/drag constants) live in `e2e/helpers.js`.

The Playwright viewport is sized (in `playwright.config.js`) to fit the whole board:
a drag endpoint below the fold registers on no square, which would fail
deterministically for any puzzle whose line reaches the lower ranks. Each dragging
test asserts the board is within the viewport, so a layout change that regrows it
fails loudly rather than resurfacing that bug.

## Running it

From `crates/blindfold-web`:

```sh
npm install                       # once
npx playwright install chromium   # once — downloads the browser
npm test
```

The Playwright config builds the release bundle with `trunk build --release` and
serves `dist/` on `127.0.0.1:8199` before the run (`e2e/serve.mjs`), so `trunk`
must be on `PATH`. Nothing needs to be running beforehand.

CI runs exactly this — see `.github/workflows/ci.yml`.
