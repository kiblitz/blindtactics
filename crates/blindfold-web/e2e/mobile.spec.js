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

test("on a touch phone the layout stacks and a touch drag draws an arrow", async ({ page }) => {
  const errors = collectErrors(page);

  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();

  const board = await page.locator(".board").boundingBox();
  if (!board) throw new Error("the board has no box");

  // The whole board is on screen, so both endpoints of a drag land on a square.
  const viewport = page.viewportSize();
  expect(viewport, "a viewport size must be configured").toBeTruthy();
  expect(
    board.y + board.height,
    "the whole board must fit the viewport or drag endpoints fall off-screen"
  ).toBeLessThanOrEqual(viewport.height);

  // On a narrow screen the two-column layout collapses: the panels stack *below* the
  // board rather than sitting beside it.
  const panels = await page.locator(".layout__panels").boundingBox();
  if (!panels) throw new Error("the panels have no box");
  expect(
    panels.y,
    "the roster and line panels stack below the board on a phone"
  ).toBeGreaterThanOrEqual(board.y + board.height - 1);

  // `touch-action: none` is set on the board — without it the browser pans the page
  // instead of delivering the drag to the app.
  const touchAction = await page
    .locator(".board")
    .evaluate((el) => getComputedStyle(el).touchAction);
  expect(touchAction).toBe("none");

  // A touch drag between two squares draws an arrow — the core interaction on a
  // phone. A vertical drag in the middle of the board is two distinct squares
  // whatever puzzle is on screen, so no puzzle needs pinning.
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
