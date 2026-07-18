# Browser test

One Playwright spec that drives the **built** app in a real browser. It exists for
the one class of bug native tests cannot see: reactive wiring.

The frozen-replay bug is the motivating case. The reveal's replay read `ply` with
`get_untracked`, so its effect fired once and stopped — the board took a single
ply and froze, still captioned "Mate." Every native test passed: `judge`,
`playback` and the pointer geometry were all correct. The fault was in the Leptos
wiring, and only a browser runs that. See CLAUDE.md, "The browser is the only
place some bugs exist".

## What it checks

- `reveal.spec.js` loads the app, confirms the board is a void with real piece
  artwork in the roster, then picks the mate-in-2 on screen, reads its id, draws
  **that puzzle's** recorded solution by dragging on the blank board, submits, and
  asserts:
  - the board reveals and the verdict says "Mate";
  - the replay **lights more than one square** over its animation — the frozen
    replay lit exactly one and stopped;
  - the mating position actually shows pieces;
  - the page logged no errors.

The solution comes from the committed `database/mate_in_2.jsonl`, read off disk, so
the test cannot drift from what the app was built with.

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
