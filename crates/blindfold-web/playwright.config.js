// @ts-check
const { defineConfig, devices } = require("@playwright/test");

// The single source of truth for the test server's port: passed to the static
// server on its command line and used to build the URLs Playwright waits on, so
// the browser and the server can never disagree about where dist/ is served.
const PORT = 8199;
const ORIGIN = `http://127.0.0.1:${PORT}`;

// Drives the real, built app in a real browser. This is the only test that can
// catch a reactive-wiring bug: the frozen replay (an effect that fired once and
// stopped) passed every native test because `judge`, `playback` and the geometry
// were all correct — the fault was in the Leptos wiring, which only a browser
// runs. See CLAUDE.md, "The browser is the only place some bugs exist".
module.exports = defineConfig({
  testDir: "./e2e",
  // A handful of specs sharing one built bundle on one static server. Run them
  // serially so the output stays readable and the run stays deterministic —
  // there is no timer to interleave, just no reason to contend for one server.
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "list" : "line",
  timeout: 60_000,
  use: {
    baseURL: ORIGIN,
    trace: "on-first-retry",
  },
  // A viewport tall enough for the whole board to be on-screen at once. The board
  // is an `aspect-ratio: 1` box as wide as the layout allows (~660px at this
  // width) and sits below the masthead, so its lower edge falls near y≈990 —
  // past the 720px default. A drag endpoint below the fold is unreachable:
  // `mouse.down()` there lands on no square and no arrow is drawn, which fails
  // deterministically for any puzzle whose line touches the lower ranks. Both
  // endpoints of every arrow must be visible at once, so scrolling per-square
  // cannot substitute — the viewport has to hold the entire board.
  projects: [
    {
      name: "chromium",
      testMatch: /reveal\.spec\.js/,
      use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 1280 } },
    },
    // A phone: a narrow viewport with touch, so the mobile spec exercises the
    // responsive layout and real touch-type pointer input. Explicit chromium options
    // rather than a device descriptor, so the emulation (mobile viewport meta +
    // touch) is stated here and does not drift with a Playwright device-list bump.
    // Tall enough that the whole board still fits above the fold at this width.
    {
      name: "mobile",
      testMatch: /mobile\.spec\.js/,
      use: {
        browserName: "chromium",
        viewport: { width: 412, height: 915 },
        isMobile: true,
        hasTouch: true,
      },
    },
  ],
  // Build the release bundle, then serve it statically. The first CI run has a
  // cold cache: a full release wasm compile plus trunk downloading wasm-bindgen
  // and wasm-opt, all inside this budget before the health check — hence the
  // generous timeout.
  webServer: {
    command: `trunk build --release && node e2e/serve.mjs ${PORT}`,
    url: ORIGIN,
    timeout: 360_000,
    reuseExistingServer: !process.env.CI,
  },
});
