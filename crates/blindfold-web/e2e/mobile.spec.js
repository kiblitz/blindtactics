// @ts-check
const { test, expect } = require("@playwright/test");
const { BOARD_SIDE, DRAG_RETRIES, DRAG_TIMEOUT_MS, collectErrors } = require("./helpers");

// Runs under the `mobile` Playwright project: a phone viewport with touch enabled
// (see playwright.config.js). It guards the things that are specific to a phone and
// invisible to the desktop reveal test — the responsive layout, and that a *touch*
// drag (not a mouse) draws an arrow. The reveal/step wiring itself is orientation-
// and size-independent, so it is not re-proved here.

// A drag between two board points, dispatched as touch-type pointer events — the
// input the board's `pointer*` handlers receive from a finger. Dispatched on the
// board element itself (which is where the handlers live and where pointer capture
// would retarget them anyway), with client coordinates, which is all `square_of`
// reads. `setPointerCapture` may reject a synthetic pointer id, but the board
// swallows that and the move/up still resolve from their coordinates.
async function touchDrag(page, from, to) {
  const init = (x, y, buttons) => ({
    pointerType: "touch",
    pointerId: 1,
    isPrimary: true,
    bubbles: true,
    button: 0,
    buttons,
    clientX: x,
    clientY: y,
  });
  await page.dispatchEvent(".board", "pointerdown", init(from.x, from.y, 1));
  await page.dispatchEvent(".board", "pointermove", init(to.x, to.y, 1));
  await page.dispatchEvent(".board", "pointerup", init(to.x, to.y, 0));
}

// Viewport-relative geometry (getBoundingClientRect), so a value reflects where an
// element actually sits on screen — what the no-scroll and ordering checks turn on.
const topOf = (page, sel) =>
  page.locator(sel).evaluate((el) => el.getBoundingClientRect().top);
const rectOf = (page, sel) =>
  page.locator(sel).evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  });

test("on a touch phone nothing scrolls and a touch drag draws an arrow", async ({ page }) => {
  const errors = collectErrors(page);

  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();

  const viewport = page.viewportSize();
  expect(viewport, "a viewport size must be configured").toBeTruthy();

  // The whole core loop fits one screen: on a phone the layout is a fixed-height shell
  // and the page cannot scroll at all. Try to scroll it to the bottom and confirm it
  // did not move — the guarantee the mobile layout exists to make.
  await page.evaluate(() => window.scrollTo(0, 100000));
  expect(await page.evaluate(() => window.scrollY), "the page must not scroll").toBe(0);
  expect(
    await page.evaluate(() => document.documentElement.scrollHeight - window.innerHeight),
    "the page content must not exceed the viewport height"
  ).toBeLessThanOrEqual(1);

  // The roster is hoisted *above* the board (so piece locations can be read while
  // drawing) and the line sits below it — the reorder the layout does at this width.
  const boardTop = await topOf(page, ".board");
  expect(await topOf(page, ".layout__roster"), "roster is above the board").toBeLessThan(boardTop);
  expect(await topOf(page, ".layout__line"), "the line is below the board").toBeGreaterThan(
    boardTop
  );

  // Submit is on screen without a scroll — no reaching for a control below the fold.
  const submit = await rectOf(page, ".button--primary");
  expect(submit.y + submit.height, "Submit is within the viewport").toBeLessThanOrEqual(
    viewport.height + 1
  );

  // `touch-action: none` is set on the board — without it the browser pans the page
  // instead of delivering the drag to the app.
  const touchAction = await page
    .locator(".board")
    .evaluate((el) => getComputedStyle(el).touchAction);
  expect(touchAction).toBe("none");

  // The board is fully on screen, so both endpoints of a drag land on a square.
  const board = await rectOf(page, ".board");
  expect(board.y, "the board's top is on screen").toBeGreaterThanOrEqual(-1);
  expect(
    board.y + board.height,
    "the whole board fits the viewport"
  ).toBeLessThanOrEqual(viewport.height + 1);

  // A touch drag between two squares draws an arrow — the core interaction on a phone.
  // A vertical drag in the middle of the board is two distinct squares whatever puzzle
  // is on screen, so no puzzle needs pinning.
  const cellW = board.width / BOARD_SIDE;
  const cellH = board.height / BOARD_SIDE;
  const fileX = board.x + 4.5 * cellW;
  const from = { x: fileX, y: board.y + 5.5 * cellH };
  const to = { x: fileX, y: board.y + 3.5 * cellH };
  const steps = page.locator(".line__step");
  for (let attempt = 0; attempt < DRAG_RETRIES; attempt++) {
    await touchDrag(page, from, to);
    const landed = await expect(steps)
      .toHaveCount(1, { timeout: DRAG_TIMEOUT_MS })
      .then(() => true)
      .catch(() => false);
    if (landed) break;
  }
  await expect(steps).toHaveCount(1);

  expect(errors).toEqual([]);
});
