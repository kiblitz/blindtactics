// @ts-check
const { test, expect } = require("@playwright/test");
const fs = require("node:fs");
const path = require("node:path");

// Files and ranks per side — the divisor turning a square index into a board
// fraction. Matches `crate::constants::BOARD_SIDE`.
const BOARD_SIDE = 8;

// How hard a single drag is retried before giving up, and how long each attempt
// waits for the line to gain its arrow. The timeout is generous on purpose: a drag
// that has registered but not yet re-rendered must not be mistaken for a dropped
// one, because a spurious retry would draw the arrow a second time.
const DRAG_RETRIES = 4;
const DRAG_TIMEOUT_MS = 2500;

// Pin the app's random puzzle choice so the reveal is reproducible. The app picks
// the next puzzle with `Math::random()`; overriding it to a constant (and clearing
// any stored rating, so selection starts from the 1200 default) fixes exactly which
// puzzle appears. Without this a CI retry re-rolls a *different* puzzle and a
// puzzle-specific failure passes on the retry — masking the bug and defeating local
// reproduction. Registered before `goto` so it is in place for the first load.
async function pinPuzzle(page, seed) {
  await page.addInitScript((s) => {
    Math.random = () => s;
    try {
      window.localStorage.clear();
    } catch (e) {
      // Storage can be unavailable (private mode); the default rating is 1200
      // either way, so selection is still deterministic.
    }
  }, seed);
}

// Guard the invariant the viewport size exists to hold: the whole board is on
// screen. A drag endpoint below the fold registers on no square (see
// playwright.config.js), so a layout change that grew the board past the viewport
// would otherwise resurface only as a baffling "arrow never registered" failure.
async function assertBoardFitsViewport(page, board) {
  const viewport = page.viewportSize();
  expect(viewport, "a viewport size must be configured").toBeTruthy();
  expect(
    board.y + board.height,
    "the whole board must fit the viewport or drag endpoints fall off-screen"
  ).toBeLessThanOrEqual(viewport.height);
  expect(
    board.x + board.width,
    "the whole board must fit the viewport or drag endpoints fall off-screen"
  ).toBeLessThanOrEqual(viewport.width);
}

// Every committed puzzle, keyed by id → solution, read straight off the same
// JSONL files the app compiles in. The app now serves one combined pool (no
// depth tiers), picked at random near the user's rating, so the test reads
// whichever puzzle is on screen and looks up *its* solution rather than assuming
// a depth or an order.
function solutionsById() {
  const byId = new Map();
  for (const depth of [1, 2, 3, 4]) {
    const file = path.join(__dirname, "..", "..", "..", "database", `mate_in_${depth}.jsonl`);
    for (const row of fs.readFileSync(file, "utf8").split("\n")) {
      if (!row.trim()) continue;
      const puzzle = JSON.parse(row);
      byId.set(puzzle.id, puzzle.solution);
    }
  }
  return byId;
}

// The centre of a square, in page pixels — asked of the board itself via
// `data-square` rather than recomputed here. The point is to test the app's own
// pointer-to-square mapping (the one place a sign flip would put every arrow on
// the wrong square), not to reimplement it and have the two agree vacuously.
async function squareCentre(page, board, name) {
  const index = await page.evaluate((want) => {
    const squares = [...document.querySelectorAll(".square")];
    return squares.findIndex((s) => s instanceof HTMLElement && s.dataset.square === want);
  }, name);
  if (index < 0) throw new Error(`no square ${name} on the board`);
  const col = index % BOARD_SIDE;
  const row = Math.floor(index / BOARD_SIDE);
  return {
    x: board.x + ((col + 0.5) * board.width) / BOARD_SIDE,
    y: board.y + ((row + 0.5) * board.height) / BOARD_SIDE,
  };
}

// Draw one arrow the way a user does: press on `from`, drag to `to`, release. If
// the UCI carries a promotion suffix, pick the piece from the board popup that
// appears at the destination square.
// Mirror of `arrow::Arrow::could_be_promotion` for a solver move. `promoRank` is
// the solver's promotion rank ("8" for a white solver, "1" for a black one, read
// off the board's orientation), so a solver move matches only its own colour's
// pattern — no false positives that would wait for a picker that never opens.
function couldBePromotion(uci, promoRank) {
  const fromRank = promoRank === "8" ? "7" : "2";
  const straightOrOneOver = Math.abs(uci.charCodeAt(0) - uci.charCodeAt(2)) <= 1;
  return uci[1] === fromRank && uci[3] === promoRank && straightOrOneOver;
}

// The solver's promotion rank, from the board's own orientation: the board is drawn
// from the solver's side, so the top-left square is a8 for a white solver (rank 8)
// and h1 for a black one (rank 1).
async function solverPromotionRank(page) {
  const topLeft = await page.locator(".square").first().getAttribute("data-square");
  return topLeft[1];
}

// Whether a UCI already carries a promotion piece — its 5th character, as in
// "g7g8q". Plain moves, castles (`e1g1`), and en passant are four characters, so
// the slot is absent. One predicate for both the picker logic and the coverage
// assertion, so the two cannot disagree on what "promotes" means.
function hasPromotion(uci) {
  return Boolean(uci[4]);
}

// Draw one arrow, resolving the promotion picker when the move opens one.
//
// Two independent robustness needs. First, the drag is retried until the line gains
// its arrow. This is only insurance against a rare synthetic-input drop — the
// failure that actually bit here was a drag endpoint below the fold registering on
// no square, deterministically, for any puzzle reaching the lower ranks, and that is
// closed by sizing the viewport to the whole board (see playwright.config.js). The
// retry re-reads the count at the top of each attempt: a drag that has registered
// by then is detected and not redrawn, which would double the arrow and never match.
// The generous timeout makes it very unlikely a registration outruns that check;
// were one to (a render slower than the timeout), the post-loop count guard turns it
// into a loud failure rather than a silent duplicate. Second, when the move matches promotion
// geometry the picker *must* be dismissed — it is modal, so a stray-open picker makes
// every later drag bail — and whether it opens is predicted from geometry rather than
// a racy DOM read (the picker and the line update in separate effects), with the
// dismissing click auto-waiting for the button to render.
async function drawArrow(page, board, uci, promoRank) {
  const steps = page.locator(".line__step");
  const before = await steps.count();
  const suffix = uci[4];
  const opensPicker = hasPromotion(uci) || couldBePromotion(uci, promoRank);

  for (let attempt = 0; attempt < DRAG_RETRIES; attempt++) {
    // A prior attempt's drag may have registered late — if the arrow is already
    // there, do not draw it again.
    if ((await steps.count()) > before) break;

    const from = await squareCentre(page, board, uci.slice(0, 2));
    const to = await squareCentre(page, board, uci.slice(2, 4));
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 8 });
    await page.mouse.up();

    await expect(steps)
      .toHaveCount(before + 1, { timeout: DRAG_TIMEOUT_MS })
      .catch(() => {}); // the top-of-loop re-check decides landed vs dropped
  }

  const after = await steps.count();
  if (after !== before + 1) {
    throw new Error(`arrow ${uci} did not register: line has ${after}, expected ${before + 1}`);
  }

  if (opensPicker) {
    const picker = page.locator(".promotion-picker");
    if (suffix) {
      const name = { q: "queen", r: "rook", b: "bishop", n: "knight" }[suffix];
      await picker.getByRole("button", { name }).click();
    } else {
      await page.getByRole("button", { name: "Move without promoting" }).click();
    }
    await expect(picker).toHaveCount(0);
  }
}

function collectErrors(page) {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() !== "error") return;
    const text = m.text();
    // Trunk's autoreload snippet opens a dev-server websocket. A fresh
    // `trunk build --release` does not inject it (CI rebuilds, so never sees it),
    // but a locally reused `trunk serve` dist does, and our static server has no
    // such endpoint. That failure is a serving artifact, never app code — the app
    // opens no websocket — so it must not fail the run.
    if (text.includes("trunk/ws") || text.includes("__trunk_address__")) return;
    errors.push(text);
  });
  return errors;
}

// The puzzle on screen, read from its own `.facts` id span — never assume an
// ordering. The id is its own span; read that rather than the whole panel, whose
// spans concatenate without separators ("id 373a2rating 1100...").
async function currentSolution(page, solutions) {
  const idText = (await page.locator(".facts span", { hasText: "id " }).textContent()) ?? "";
  const id = idText.replace(/^id\s+/, "").trim();
  const line = solutions.get(id);
  expect(line, `puzzle ${id} must be in the committed database`).toBeTruthy();
  return line;
}

test("the board is blind until solved, and the roster shows real piece artwork", async ({ page }) => {
  const errors = collectErrors(page);

  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();

  // The user asked for "an actual knight picture", not a letter — the roster
  // draws SVG pieces.
  expect(await page.locator(".entry__piece svg").count()).toBeGreaterThan(0);
  // And nothing is on the board itself yet: it is a void until solved.
  await expect(page.locator(".board--revealed")).toHaveCount(0);
  expect(await page.locator(".piece").count()).toBe(0);

  expect(errors).toEqual([]);
});

// The reveal is exercised on pinned puzzles rather than a random one, so a CI
// retry re-runs the identical case (see `pinPuzzle`). The seeds are chosen against
// the committed database's rating order (fresh rating 1200) to cover a spread
// deliberately: a mate-in-3 whose line includes a real promotion, so the picker's
// piece-choice path is walked, and a mate-in-4 whose first move *originates* on the
// lowest rank — the below-the-fold endpoint the viewport fix exists to keep
// on-screen. The test still reads whichever puzzle is on screen and looks up *its*
// solution, so it cannot silently drift from what the seed selects.
//
// `expectSuffix` guards the promotion coverage against database regeneration: were
// seed 0.05 to stop selecting a promotion puzzle, the suffix→piece path would quietly
// stop being tested with a green run, so the case asserts its solution really promotes.
const REVEAL_CASES = [
  { seed: 0.05, note: "mate in 3, with a promotion", expectSuffix: true },
  { seed: 0.8, note: "mate in 4, first move from the lowest rank", expectSuffix: false },
];

for (const { seed, note, expectSuffix } of REVEAL_CASES) {
  test(`solving reveals the board, steps through the mate, and moves the rating (${note})`, async ({
    page,
  }) => {
    const errors = collectErrors(page);
    const solutions = solutionsById();

    await pinPuzzle(page, seed);
    await page.goto("/");
    await expect(page.locator(".board")).toBeVisible();

    const rating = () => page.locator(".elo strong").textContent().then((t) => Number(t));
    const before = await rating();

    const line = await currentSolution(page, solutions);
    expect(
      line.some(hasPromotion),
      `seed ${seed} must ${expectSuffix ? "" : "not "}select a puzzle whose solution promotes`
    ).toBe(expectSuffix);
    const board = await page.locator(".board").boundingBox();
    if (!board) throw new Error("the board has no box");
    await assertBoardFitsViewport(page, board);
    const promoRank = await solverPromotionRank(page);
    for (const uci of line) await drawArrow(page, board, uci, promoRank);
    expect(await page.locator(".line__step").count()).toBe(line.length);

    await page.getByRole("button", { name: "Submit", exact: true }).click();

    // The reveal happens, opening on the mating position: the verdict says so, the
    // pieces are shown, and the mating square is lit.
    await expect(page.locator(".board--revealed")).toHaveCount(1);
    await expect(page.locator(".verdict")).toContainText("Mate");
    expect(await page.locator(".piece").count()).toBeGreaterThan(0);
    expect(await page.locator(".square--played").count()).toBeGreaterThan(0);

    // Solving raises the rating, and the "+n" delta shows it.
    await expect(page.locator(".elo__delta")).toBeVisible();
    await expect(page.locator(".elo__delta")).toContainText("+");
    expect(await rating()).toBeGreaterThan(before);

    // The manual reveal wiring: this is where the frozen-replay class of bug lives
    // (a native test cannot see the Leptos effect). Step all the way back to the
    // start — where nothing has been played, so no square is lit — then forward one
    // ply, which lights the first move again. A reveal that did not actually move
    // would fail both halves.
    const back = page.getByRole("button", { name: "Step back" });
    while (await back.isEnabled()) await back.click();
    expect(
      await page.locator(".square--played").count(),
      "at the start position nothing is played, so nothing is lit"
    ).toBe(0);
    expect(await page.locator(".piece").count()).toBeGreaterThan(0);

    await page.getByRole("button", { name: "Step forward" }).click();
    expect(
      await page.locator(".square--played").count(),
      "stepping forward re-lights the square the first move landed on"
    ).toBeGreaterThan(0);

    expect(errors).toEqual([]);
  });
}

// A move that matches promotion *geometry* but is not a pawn — a rook lift like
// Rf7-f8#, which 14% of the database needs. The picker opens off geometry alone
// (`could_be_promotion` is necessary, not sufficient), so it must offer a way to
// finish the move without promoting, and it must not let the panel submit an
// unresolved move behind it. Without the "no promotion" exit those puzzles cannot
// be entered as the plain, legal move they are.
//
// The drag is puzzle-independent: the board is always drawn from the solver's
// side, so the solver promotes toward the *top* of the screen. A straight drag up
// one file from the second-from-top row to the top row is therefore always a
// promotion-geometry move, whatever puzzle is on screen.
test("a last-rank move that is not a promotion can still be entered", async ({ page }) => {
  const errors = collectErrors(page);

  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();
  const board = await page.locator(".board").boundingBox();
  if (!board) throw new Error("the board has no box");
  await assertBoardFitsViewport(page, board);

  const fileX = board.x + 4.5 * (board.width / BOARD_SIDE);
  const from = { x: fileX, y: board.y + 1.5 * (board.height / BOARD_SIDE) };
  const to = { x: fileX, y: board.y + 0.5 * (board.height / BOARD_SIDE) };
  // Retry until the drag registers (fast simulated drags occasionally drop) — a
  // registered promotion-geometry drag always opens the picker.
  const picker = page.locator(".promotion-picker");
  for (let attempt = 0; attempt < DRAG_RETRIES; attempt++) {
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 8 });
    await page.mouse.up();
    const opened = await expect(picker)
      .toBeVisible({ timeout: DRAG_TIMEOUT_MS })
      .then(() => true)
      .catch(() => false);
    if (opened) break;
  }

  // The picker opens on the geometry alone.
  await expect(picker).toBeVisible();
  // While it is open the move is unresolved, so the panel must not submit it.
  await expect(page.getByRole("button", { name: "Submit", exact: true })).toBeDisabled();

  // The move can be finished as a plain, non-promoting move — the exit the modal
  // was missing. The arrow stays; it just carries no promotion piece.
  await page.getByRole("button", { name: "Move without promoting" }).click();
  await expect(picker).toHaveCount(0);
  expect(await page.locator(".line__step").count()).toBe(1);
  expect(await page.locator(".line__promotion").count()).toBe(0);
  await expect(page.getByRole("button", { name: "Submit", exact: true })).toBeEnabled();

  expect(errors).toEqual([]);
});
