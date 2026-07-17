# Blindfold Chess Trainer

Train forced-mate calculation without ever seeing the pieces.

You get an **empty board** and a **roster** — "white: king d5, bishops b4 c6, pawns a6 b7
g5; black: king g7" — and nothing else. Work out the mate in your head, drag numbered
arrows across the blank squares to commit to your line, and hit submit. The app plays it
out. Solve it and the board is revealed.

Mate in 1, 2, 3 and 4.

## Status

Early. `blindfold-core` (the chess logic) is built and tested. The curation tool, the web
app, and the puzzle database are not written yet.

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
| `crates/blindfold-curate` | *(planned)* Offline CLI: Lichess dump → curated subset. |
| `crates/blindfold-web` | *(planned)* Leptos client-side app. |
| `database/` | *(planned)* The curated puzzle subset, committed. |

`blindfold-core` depends on no UI and no I/O, so its whole test suite runs under plain
`cargo test` with no browser or wasm toolchain. It is shared by the curation tool and the
app, which is what stops the database and the live app from ever disagreeing about what
"solved" means.

## Build

Requires the pinned toolchain in `rust-toolchain.toml` (1.97 — shakmaty needs MSRV 1.95).
rustup will fetch it automatically.

```sh
cargo test          # the core suite; fast, no browser needed
cargo clippy --all-targets
cargo fmt -- --check
```

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
| Piece images *(planned)* | [Colin M.L. Burnett, via Wikimedia Commons](https://commons.wikimedia.org/wiki/Category:SVG_chess_pieces) | 3-clause BSD |

On the piece images: Wikimedia's file pages show a prominent CC BY-SA 3.0 banner, but the
underlying template is `{{self|GFDL|migration=relicense|BSD|GPL}}` — Cburnett self-licensed
under four options and the licensee chooses. We elect **3-clause BSD**, which carries no
share-alike obligation. (Lichess's `COPYING.md` lists cburnett as GPLv2+; that is Lichess
electing a different option for their own distribution, and does not bind us. Take the
files from Wikimedia Commons, not from the lila repo.)
