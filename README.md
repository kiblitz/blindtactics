# Blindfold Chess Trainer

Train forced-mate calculation without ever seeing the pieces.

You get an **empty board** and a **roster** — "white: king d5, bishops b4 c6, pawns a6 b7
g5; black: king g7" — and nothing else. Work out the mate in your head, drag numbered
arrows across the blank squares to commit to your line, and hit submit. The app plays it
out. Solve it and the board is revealed.

Mate in 1, 2, 3 and 4.

## Status

All three crates are built and tested, and the database is committed. `blindfold-core` is
the chess logic; `blindfold-curate` streams the Lichess dump into `database/`; and
`blindfold-web` is the Leptos app — a blank board you draw numbered arrows on, with an
animated reveal on a solve. Run `cargo test --workspace` for the current test count rather
than trusting a number written here, which is how counts go stale.

## The one idea worth knowing

The arrow UI is **linear** — it cannot express a branch. You commit to your whole line
before seeing a single opponent reply. So a puzzle is only usable here if your arrows mate
*no matter how the opponent defends*.

That is a stronger property than "this is a mate in 3", and no puzzle database records it.
Lichess stores exactly one engine-chosen defense per puzzle, and explicitly waives
solution uniqueness for mate puzzles. So its `mateInN` tag is treated here as nothing more
than a cheap candidate filter, and every puzzle is re-proved from scratch against every
legal defense before it earns a place in `database/`.

Note that branching itself is fine — it just has to be *invisible to you*. A puzzle where
the opponent has five defenses and the same two arrows mate all five is exactly what this
trainer wants.

## Layout

| Path | What |
|---|---|
| `crates/blindfold-core` | Pure logic: arrows, the linearity prover, rosters, the puzzle model. No UI, no I/O. |
| `crates/blindfold-curate` | Offline CLI: Lichess dump → curated subset. |
| `crates/blindfold-web` | Leptos client-side app. |
| `database/` | The curated puzzle subset, committed. |

`blindfold-core` depends on no UI and no I/O, so its whole test suite runs under plain
`cargo test` with no browser or wasm toolchain. It is shared by the curation tool and the
app, so that the database and the live app cannot disagree about what "solved" means: both
call `mate::judge`.

## Build

Requires the pinned toolchain in `rust-toolchain.toml` (1.97 — shakmaty needs MSRV 1.95).
rustup will fetch it automatically.

```sh
cargo test --workspace    # all three crates; core is fast and needs no browser
cargo clippy --workspace --all-targets
cargo fmt --all -- --check
```

To run the web app: `trunk serve` in `crates/blindfold-web` (`trunk build --release` for
`dist/`).

There are also browser tests — `crates/blindfold-web/e2e/`, run as two Playwright projects
(`reveal.spec.js` on a desktop viewport, `mobile.spec.js` on a phone with touch) — for the
class of bug native tests cannot see (reactive wiring; they caught a replay that froze after
one ply). From `crates/blindfold-web`: `npm install`, `npx playwright install chromium`, then
`npm test`.

## Continuous integration

`.github/workflows/ci.yml` runs on every push and pull request: one job for `fmt --check`,
clippy (native and wasm, warnings denied), and `cargo test --workspace`; a second that
builds the release bundle and runs the browser test against real chromium.

## Licensing

**This project is GPL-3.0-or-later.** Not by preference — it depends on
[shakmaty](https://github.com/niklasf/shakmaty), which is GPL-3.0-or-later, and shipping a
WASM bundle to a browser is distribution. shakmaty is Lichess's own chess library and by
some distance the best fit here, so the trade was made deliberately.

Third-party material:

| What | Source | License |
|---|---|---|
| Puzzle data | [Lichess open database](https://database.lichess.org/) | CC0 1.0 — public domain |
| Chess logic | [shakmaty](https://github.com/niklasf/shakmaty) | GPL-3.0-or-later |
| Piece images | Colin M.L. Burnett, via [`lichess-org/lila`](https://github.com/lichess-org/lila/tree/master/public/piece/cburnett) | GPLv2-or-later |

On the piece images: these are Lichess's optimized Cburnett set, taken from lila and shipped
verbatim, so lila's own election governs — its `COPYING.md` lists
`public/piece/cburnett | Colin M.L. Burnett | GPLv2+`. The "or later" is what makes them
compatible with this project, which is GPL-3.0-or-later already because shakmaty forces it.

Cburnett does self-license the *originals* on Wikimedia Commons under four options (GFDL /
CC BY-SA 3.0 / 3-clause BSD / GPLv2+ — "You may select the license of your choice"), and an
earlier draft of this file elected BSD and told you to take the files from Commons. We do
not: the shipped files are lila's, which are optimized derivatives roughly half the size of
the Commons originals and are what Lichess actually renders and tests. Electing BSD would
also have bought this bundle nothing — shakmaty forces GPL-3.0-or-later regardless — so it
would only have mattered to someone extracting the artwork downstream, who can still fetch
Cburnett's originals from Commons and elect BSD there. See
`crates/blindfold-web/assets/pieces/README.md` for the per-file attribution.
