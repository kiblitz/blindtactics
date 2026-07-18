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
  // The reveal is a timed animation; running specs in parallel on one page would
  // interleave their timers. One at a time.
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
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
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
