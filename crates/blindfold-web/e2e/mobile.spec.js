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

// Viewport-relative geometry (getBoundingClientRect), so a scroll is reflected —
// which `boundingBox()` alone does not make unambiguous, and the sticky-pin check
// turns on exactly that.
const topOf = (page, sel) =>
  page.locator(sel).evaluate((el) => el.getBoundingClientRect().top);
const rectOf = (page, sel) =>
  page.locator(sel).evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  });

test("on a touch phone the roster pins above the board and a touch drag draws an arrow", async ({
  page,
}) => {
  const errors = collectErrors(page);

  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();

  const viewport = page.viewportSize();
  expect(viewport, "a viewport size must be configured").toBeTruthy();

  // On a phone the roster is hoisted *above* the board (so piece locations can be
  // read while drawing) and the line sits below it — the reorder the layout does at
  // this width.
  await page.evaluate(() => window.scrollTo(0, 0));
  const boardTop0 = await topOf(page, ".board");
  expect(await topOf(page, ".layout__roster"), "roster is above the board").toBeLessThan(boardTop0);
  expect(await topOf(page, ".layout__line"), "the line is below the board").toBeGreaterThan(
    boardTop0
  );

  // `touch-action: none` is set on the board — without it the browser pans the page
  // instead of delivering the drag to the app.
  const touchAction = await page
    .locator(".board")
    .evaluate((el) => getComputedStyle(el).touchAction);
  expect(touchAction).toBe("none");

  // Scroll until the roster pins to the top; the board then sits right below it, fully
  // visible — the intended reading position, and the pin is a browser/layout concern a
  // native test cannot see. (Scrolling by the roster's own top pins it with the board
  // just beneath, rather than over-scrolling the board up under the pinned roster.)
  const rosterTop0 = await topOf(page, ".layout__roster");
  await page.evaluate((y) => window.scrollTo(0, y + 4), rosterTop0);
  const rosterPinned = await topOf(page, ".layout__roster");
  expect(rosterPinned, "the roster stays pinned near the top when scrolled").toBeGreaterThanOrEqual(
    -1
  );
  expect(rosterPinned, "and does not scroll away").toBeLessThan(viewport.height * 0.5);

  // The board is fully on screen below the pinned roster, so both endpoints of a drag
  // land on a square.
  const board = await rectOf(page, ".board");
  expect(
    board.y + board.height,
    "the board sits fully below the pinned roster, within the viewport"
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
