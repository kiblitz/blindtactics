// @ts-check
const { test, expect } = require("@playwright/test");
const fs = require("node:fs");
const path = require("node:path");
const { BOARD_SIDE, DRAG_RETRIES, DRAG_TIMEOUT_MS, collectErrors } = require("./helpers");

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

// Whether a UCI already carries a promotion piece — its 5th character, as in
// "g7g8q". Plain moves, castles (`e1g1`), and en passant are four characters, so
// the slot is absent. One predicate for both the picker logic and the coverage
// assertion, so the two cannot disagree on what "promotes" means.
function hasPromotion(uci) {
  return Boolean(uci[4]);
}

// Draw one arrow, then set its promotion piece if the UCI carries one.
//
// The drag is retried until the line gains its arrow. This is only insurance against
// a rare synthetic-input drop — the failure that actually bit here was a drag
// endpoint below the fold registering on no square, deterministically, for any
// puzzle reaching the lower ranks, and that is closed by sizing the viewport to the
// whole board (see playwright.config.js). The retry re-reads the count at the top of
// each attempt: a drag that has registered by then is detected and not redrawn,
// which would double the arrow and never match. The generous timeout makes it very
// unlikely a registration outruns that check; were one to (a render slower than the
// timeout), the post-loop count guard turns it into a loud failure rather than a
// silent duplicate.
//
// Promotion is no longer a board modal: a last-rank move grows a per-move control in
// the line list that defaults to "no promotion", so a plain move (or a rook lift to
// the last rank) needs no action, and a real promotion just presses its piece there.
async function drawArrow(page, board, uci) {
  const steps = page.locator(".line__step");
  const before = await steps.count();

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

  const suffix = uci[4];
  if (suffix) {
    const name = { q: "queen", r: "rook", b: "bishop", n: "knight" }[suffix];
    await steps.nth(before).locator(".line__promote").getByRole("button", { name }).click();
  }
}

// The puzzle on screen, read from its own `.facts` line — never assume an ordering.
// The facts line is now just the id, prefixed with a subtle "#" (see the `Facts`
// component); strip it to get the bare id the database is keyed by.
async function currentSolution(page, solutions) {
  const idText = (await page.locator(".facts").textContent()) ?? "";
  const id = idText.replace(/^#/, "").trim();
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

// The arrowhead used to be a shared <marker>, which lives in <defs> and cannot
// inherit the arrow's per-move colour, so every head painted the board's base amber
// while the shaft was correctly coloured. The head is now a filled <polygon> in the
// arrow's own group; this pins the invariant the marker broke — head colour equals
// shaft colour — which only a browser resolves (currentColor → a concrete rgb). Any
// drag draws a committed arrow (legality is judged only on submit), so no pinned
// puzzle is needed.
test("an arrow's head is painted the same colour as its shaft", async ({ page }) => {
  const errors = collectErrors(page);

  await page.goto("/");
  const board = await page.locator(".board").boundingBox();
  expect(board, "the board must have a box to drag on").toBeTruthy();
  if (!board) return;
  await assertBoardFitsViewport(page, board);

  await drawArrow(page, board, "e2e4");

  const shaftStroke = await page
    .locator(".arrow line")
    .first()
    .evaluate((el) => getComputedStyle(el).stroke);
  const headFill = await page
    .locator(".arrow__head")
    .first()
    .evaluate((el) => getComputedStyle(el).fill);

  // A resolved colour, not "none"/transparent, and the two agree — a marker-painted
  // head would be the board's amber here, not the arrow's own colour.
  expect(headFill).toMatch(/^rgb/);
  expect(headFill).toBe(shaftStroke);

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
    for (const uci of line) await drawArrow(page, board, uci);
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
// Rf7-f8#, which 14% of the database needs. Promotion is no longer a board modal
// that hijacks the move; it is a per-move control in the line list that defaults to
// "no promotion". So a last-rank non-pawn move enters immediately as the plain,
// legal move it is — nothing interrupts the drag, submit is never blocked, and the
// control simply sits at its default. This guards the regression the old modal
// caused: those 14% of puzzles were unenterable because the modal's only exits were
// "pick a piece" (illegal) or "cancel" (throws the move away).
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
  const steps = page.locator(".line__step");
  // Retry until the drag registers (fast simulated drags occasionally drop).
  for (let attempt = 0; attempt < DRAG_RETRIES; attempt++) {
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 8 });
    await page.mouse.up();
    const landed = await expect(steps)
      .toHaveCount(1, { timeout: DRAG_TIMEOUT_MS })
      .then(() => true)
      .catch(() => false);
    if (landed) break;
  }

  // The move enters straight away, with no modal to dismiss — and the panel can
  // submit it, because nothing is left unresolved behind a picker.
  await expect(steps).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Submit", exact: true })).toBeEnabled();

  // Its per-move promotion control is present (the geometry matches) but sits at its
  // "no promotion" default, so the arrow carries no promotion piece.
  const control = steps.first().locator(".line__promote");
  await expect(control).toHaveCount(1);
  await expect(control.getByRole("button", { name: "No promotion" })).toHaveAttribute(
    "aria-pressed",
    "true"
  );

  expect(errors).toEqual([]);
});

// The point-of-view setting and the flip button both re-orient the board — a
// reactive concern a native test cannot see: it drives the real settings menu and
// flip control and reads the board's own top-left `data-square` to confirm the
// re-render actually happened. White/Black are puzzle-independent (a8 / h1 in the
// corner whatever the solver is), so no puzzle needs pinning. The reload asserts the
// split the design rests on: the POV persists to localStorage, the flip does not.
test("the POV setting and flip control re-orient the board", async ({ page }) => {
  const errors = collectErrors(page);

  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();
  const topLeft = () => page.locator(".square").first().getAttribute("data-square");

  // POV = White: white's a8 is the top-left corner.
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("menuitemradio", { name: "White" }).click();
  await expect.poll(topLeft).toBe("a8");

  // POV = Black: the board mirrors through both axes, so h1 is top-left.
  await page.getByRole("menuitemradio", { name: "Black" }).click();
  await expect.poll(topLeft).toBe("h1");

  // Flip inverts the current view back to white-at-bottom, without touching the POV.
  await page.getByRole("button", { name: "Flip board" }).click();
  await expect.poll(topLeft).toBe("a8");
  await expect(page.getByRole("button", { name: "Flip board" })).toHaveAttribute(
    "aria-pressed",
    "true"
  );

  // Reload: the POV (Black) persisted, the transient flip did not — so h1 again.
  await page.reload();
  await expect(page.locator(".board")).toBeVisible();
  await expect.poll(topLeft).toBe("h1");

  expect(errors).toEqual([]);
});

// Output mode and read-aloud drive the browser's speechSynthesis, which a native test
// cannot reach — so this guards the wiring: the output mode persists across a reload
// like the POV, selecting it actuates nothing, and the roster's speak button reads
// aloud without a page error. Headless chromium has a speechSynthesis with no voices,
// so `speak()` is a silent no-op here; what matters is nothing throws.
test("the output mode persists and the roster speak button raises no error", async ({ page }) => {
  const errors = collectErrors(page);

  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();

  // The roster's speak button reads the puzzle aloud on demand (the click is the
  // required gesture). Checked first, while the settings menu is closed and not
  // floating over it. A silent no-op in headless, but it must never throw.
  await page.getByRole("button", { name: "Read the roster aloud" }).click();

  // Output defaults to "Show" — nothing is spoken automatically (audio is opt-in).
  await page.getByRole("button", { name: "Settings" }).click();
  const readAloud = page.getByRole("menuitemradio", { name: "Read aloud" });
  await expect(readAloud).toHaveAttribute("aria-checked", "false");

  // Selecting "Read aloud" marks the mode and persists it, but — deliberately —
  // actuates nothing (no speech on select; selecting a setting is not a gesture to
  // start talking). We only assert the mode took; that it speaks on the *next* puzzle
  // is a timing-dependent effect not worth pinning in a browser test.
  await readAloud.click();
  await expect(readAloud).toHaveAttribute("aria-checked", "true");

  // Persists across a reload, like the POV.
  await page.reload();
  await expect(page.locator(".board")).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("menuitemradio", { name: "Read aloud" })).toHaveAttribute(
    "aria-checked",
    "true"
  );

  expect(errors).toEqual([]);
});

// Giving up reveals the stored solution as a scored loss, and the post-tactic
// analysis — the SAN move list — is navigable by click and by arrow key. This is a
// reactive concern (a click or a keypress must actually move the board), so it lives
// in the browser test. Pinned to the mate-in-4 seed so the move list is several plies
// long. No drawing is needed: giving up is exactly for when you are stuck with
// nothing drawn, so the button is pressed on an empty line.
test("giving up reveals the solution, and the move list navigates by click and arrow key", async ({
  page,
}) => {
  const errors = collectErrors(page);

  await pinPuzzle(page, 0.8);
  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();

  const rating = () => page.locator(".elo strong").textContent().then((t) => Number(t));
  const before = await rating();

  await page.getByRole("button", { name: "Give up" }).click();

  // The board is revealed on the mate, the verdict names the concession, and giving
  // up cost rating (the delta shows a drop).
  await expect(page.locator(".board--revealed")).toHaveCount(1);
  await expect(page.locator(".verdict")).toContainText("gave up");
  expect(await page.locator(".piece").count()).toBeGreaterThan(0);
  await expect(page.locator(".elo__delta")).toBeVisible();
  expect(await rating()).toBeLessThan(before);

  // The move list holds every ply of the solution (the mate-in-4 line is 7 plies).
  const plies = page.locator(".movelist__ply");
  expect(await plies.count()).toBeGreaterThan(1);

  // Clicking a move jumps the board to it: the move highlights and the square it
  // landed on lights up.
  await plies.first().click();
  await expect(plies.first()).toHaveClass(/movelist__ply--on/);
  expect(
    await page.locator(".square--played").count(),
    "clicking the first move lights the square it landed on"
  ).toBeGreaterThan(0);

  // Arrow keys walk the reveal, like a Lichess analysis board. Left to the start
  // (nothing played, so nothing lit), then right re-lights the first move.
  await page.keyboard.press("ArrowLeft");
  expect(
    await page.locator(".square--played").count(),
    "arrow-left steps back to the start, where nothing is played"
  ).toBe(0);
  await page.keyboard.press("ArrowRight");
  expect(
    await page.locator(".square--played").count(),
    "arrow-right steps forward and re-lights the first move"
  ).toBeGreaterThan(0);

  expect(errors).toEqual([]);
});
