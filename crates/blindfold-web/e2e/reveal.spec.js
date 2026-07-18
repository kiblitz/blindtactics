// @ts-check
const { test, expect } = require("@playwright/test");
const fs = require("node:fs");
const path = require("node:path");

// The committed database the app compiles in, read straight off disk. The test
// takes each puzzle's solution from the same file the app was built from, so the
// two can never drift: if curation regenerates the set, this follows.
function solutionsForDepth(depth) {
  const file = path.join(__dirname, "..", "..", "..", "database", `mate_in_${depth}.jsonl`);
  const byId = new Map();
  for (const row of fs.readFileSync(file, "utf8").split("\n")) {
    if (!row.trim()) continue;
    const puzzle = JSON.parse(row);
    byId.set(puzzle.id, puzzle.solution);
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
  const col = index % 8;
  const row = Math.floor(index / 8);
  return {
    x: board.x + ((col + 0.5) * board.width) / 8,
    y: board.y + ((row + 0.5) * board.height) / 8,
  };
}

// Draw one arrow the way a user does: press on `from`, drag to `to`, release. If
// the UCI carries a promotion suffix, click the matching piece in the picker.
async function drawArrow(page, board, uci) {
  const from = await squareCentre(page, board, uci.slice(0, 2));
  const to = await squareCentre(page, board, uci.slice(2, 4));
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 8 });
  await page.mouse.up();

  const promotion = uci[4];
  if (promotion) {
    const name = { q: "queen", r: "rook", b: "bishop", n: "knight" }[promotion];
    await page.locator(".line__step").last().getByRole("button", { name }).click();
  }
}

function collectErrors(page) {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  return errors;
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

test("solving a mate-in-2 reveals the board and replays every ply", async ({ page }) => {
  const errors = collectErrors(page);
  const solutions = solutionsForDepth(2);

  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();
  await page.getByRole("button", { name: "Mate in 2", exact: true }).click();

  // Whichever mate-in-2 is on screen, solve *that* one — read its id and take its
  // recorded solution, never assume an ordering. The id is its own `.facts` span;
  // read that span rather than the whole panel, whose spans concatenate without
  // separators ("...100id 373a2rating...").
  const idText = (await page.locator(".facts span", { hasText: "id " }).textContent()) ?? "";
  const id = idText.replace(/^id\s+/, "").trim();
  const line = solutions.get(id);
  expect(line, `puzzle ${id} must be in the committed mate_in_2 database`).toBeTruthy();

  const board = await page.locator(".board").boundingBox();
  if (!board) throw new Error("the board has no box");
  for (const uci of line) await drawArrow(page, board, uci);
  expect(await page.locator(".line__step").count()).toBe(line.length);

  await page.getByRole("button", { name: "Submit", exact: true }).click();

  // The reveal happens, and the verdict says so.
  await expect(page.locator(".board--revealed")).toHaveCount(1);
  await expect(page.locator(".verdict")).toContainText("Mate");

  // The frozen-replay guard. A mate in 2 replays 2*2 - 1 = 3 plies, each lighting
  // the square its move landed on. The bug this test exists for lit exactly one
  // square and froze there, still captioned "Mate" — so a working reveal must
  // light more than one distinct square over the animation. Poll across its whole
  // duration: REVEAL_MS 900 + 2 * PLAYBACK_MS 600 ~= 2.1s, and
  // POLL_SAMPLES * POLL_INTERVAL_MS comfortably covers it.
  const POLL_SAMPLES = 40;
  const POLL_INTERVAL_MS = 80;
  const lit = new Set();
  for (let i = 0; i < POLL_SAMPLES; i++) {
    const square = await page
      .locator(".square--played")
      .getAttribute("data-square")
      .catch(() => null);
    if (square) lit.add(square);
    await page.waitForTimeout(POLL_INTERVAL_MS);
  }

  expect(lit.size, "the replay must advance past a single ply, not freeze").toBeGreaterThan(1);
  expect(
    await page.locator(".piece").count(),
    "the mating position must show pieces once revealed"
  ).toBeGreaterThan(0);

  expect(errors).toEqual([]);
});
