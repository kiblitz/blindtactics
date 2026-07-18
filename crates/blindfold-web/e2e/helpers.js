// @ts-check
// Shared e2e utilities, so the desktop reveal spec and the mobile spec agree on the
// board geometry, the drag-retry budget, and what counts as a page error.

// Files and ranks per side — the divisor turning a square index into a board
// fraction. Matches `crate::constants::BOARD_SIDE`.
const BOARD_SIDE = 8;

// How hard a single drag is retried before giving up, and how long each attempt
// waits for the line to gain its arrow. The timeout is generous on purpose: a drag
// that has registered but not yet re-rendered must not be mistaken for a dropped
// one, because a spurious retry would draw the arrow a second time.
const DRAG_RETRIES = 4;
const DRAG_TIMEOUT_MS = 2500;

// Collect real page errors, filtering the one non-app console error our static
// server provokes. Returned array is asserted empty at the end of each test.
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

module.exports = { BOARD_SIDE, DRAG_RETRIES, DRAG_TIMEOUT_MS, collectErrors };
