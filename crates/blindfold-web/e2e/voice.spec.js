// @ts-check
const { test, expect } = require("@playwright/test");
const fs = require("node:fs");
const path = require("node:path");
const { collectErrors } = require("./helpers");

// Voice input's streaming path — the hardest thing in the project to test, and the
// one CLAUDE.md long said was *not* e2e-testable ("headless chromium has no speech
// recognition, so there is no way to feed it audio"). That is true of the *real*
// recogniser, but the app talks to it only through `webkitSpeechRecognition`, so a
// fake one stubbed in before load lets us drive the whole browser flow — segment a
// spoken line into moves, stream each onto the board, and fire a final command — the
// exact wiring (`recognition` → `session::interpret` → `app::handle_voice`) that no
// native test can reach. The decision logic is covered natively (`diction`,
// `session::interpret`); this covers the reactive glue around it.

// A fake Vosk: `window.Vosk` with a createModel that yields a KaldiRecognizer capturing
// itself as `window.__rec`, so the test can fire recognition events on the live one. The
// real `recognition` module finds this stub already on `window.Vosk` and skips the network
// (no 41 MB model, no library download) — then runs its true audio-graph setup against it.
// getUserMedia/AudioContext/ScriptProcessor are real in headless (see the fake-media launch
// args in playwright.config.js), so only the recogniser itself is stubbed.
const FAKE_VOSK = `
window.__rec = null;
window.Vosk = {
  createModel: async () => ({
    KaldiRecognizer: function (sampleRate, grammar) {
      this._handlers = {};
      this.on = (event, cb) => { this._handlers[event] = cb; };
      this.acceptWaveform = () => {};
      this.remove = () => {};
      window.__rec = this;
    },
  }),
};`;

// Pin the random puzzle and install the fake Vosk, both before first load. The app picks
// its puzzle with Math.random and reads window.Vosk when the mic is armed, so both
// overrides must be in place before `goto`.
async function pinAndFake(page, seed) {
  await page.addInitScript(FAKE_VOSK);
  await page.addInitScript((s) => {
    Math.random = () => s;
    try {
      window.localStorage.clear();
    } catch (e) {
      // Storage can be unavailable (private mode); the default rating still makes
      // selection deterministic.
    }
  }, seed);
}

// Every committed puzzle, keyed by id → solution — the same JSONL the app compiles in.
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

// The puzzle on screen, read from its own `.facts` line — never assume an ordering.
async function currentSolution(page, solutions) {
  const id = ((await page.locator(".facts").textContent()) ?? "").replace(/^#/, "").trim();
  const line = solutions.get(id);
  expect(line, `puzzle ${id} must be in the committed database`).toBeTruthy();
  return { id, line };
}

const PROMO = { q: "queen", r: "rook", b: "bishop", n: "knight" };

// Turn a solution (a list of UCI moves) into the words a user would speak: each move as
// "<role> <dest>", with a promotion suffix spoken after the destination ("g1 queen").
//
// The role must be read from the *evolving* position, not the initial roster: move 2's
// from-square only holds a piece after move 1 plays. So the roster is read once into a
// square→role map, then walked forward move by move — otherwise a later move whose piece
// arrived mid-line would be spoken as a bare pawn move and mis-segment.
async function spokenLine(page, solution) {
  const roleAt = await page.evaluate(() => {
    const map = {};
    for (const entry of document.querySelectorAll(".entry")) {
      const role = entry.querySelector(".entry__piece")?.getAttribute("aria-label") ?? "";
      const squares = entry.querySelector(".entry__squares")?.textContent ?? "";
      for (const sq of squares.trim().split(/\s+/)) if (sq) map[sq] = role;
    }
    return map;
  });

  const spoken = [];
  for (const uci of solution) {
    const from = uci.slice(0, 2);
    const to = uci.slice(2, 4);
    const role = roleAt[from] ?? "";
    const promo = uci[4] ? ` ${PROMO[uci[4]]}` : "";
    spoken.push(`${role} ${to}${promo}`.trim());
    // Advance the tracked position so the next move reads the right piece.
    delete roleAt[from];
    roleAt[to] = uci[4] ? PROMO[uci[4]] : role;
  }
  return spoken;
}

// Force the silence timeout to a chosen number of seconds, before load. Written to the
// same localStorage key `settings::save_silence` uses, so `load_silence` reads it at
// mount. Registered after the puzzle-pin script (which clears storage) so it survives.
async function setSilence(page, secs) {
  await page.addInitScript(
    ({ key, value }) => {
      try {
        window.localStorage.setItem(key, value);
      } catch (e) {
        // Storage unavailable (private mode); the default timeout applies instead.
      }
    },
    { key: "blindfold.silence", value: String(secs) }
  );
}

// Fire one transcript on the live fake recogniser, as Vosk would: a partialresult while
// speaking (is_final == false) or a result once settled (is_final == true).
async function fire(page, transcript, isFinal) {
  await page.evaluate(
    ({ t, f }) => {
      const rec = window.__rec;
      if (!rec) throw new Error("no live recognizer");
      if (f) rec._handlers.result?.({ result: { text: t } });
      else rec._handlers.partialresult?.({ result: { partial: t } });
    },
    { t: transcript, f: isFinal }
  );
}

// Arm the mic and wait for the (async) recognition graph to come up — the recogniser is
// created after the model "loads" and getUserMedia resolves, so `window.__rec` appears a
// tick after the click.
async function armMic(page) {
  await page.getByRole("button", { name: "Voice input" }).click();
  await expect(page.locator(".button--recording")).toHaveCount(1);
  await page.waitForFunction(() => !!window.__rec);
}

// Whether a UCI carries a promotion piece (its 5th character).
function hasPromotion(uci) {
  return Boolean(uci[4]);
}

// Seed 0.95 selects a mate-in-3 whose line promotes (f3g2, g2g1=Q, g1g3): a single
// case that walks every hard part of the streaming path at once — several moves in one
// breath, a mid-line promotion (the "g1 queen queen g3" segmentation that must not read
// the promotion as the next mover), and a spoken command to finish. Reads whichever
// puzzle the seed lands on and asserts it really promotes, so a database regeneration
// that stopped selecting a promotion puzzle fails loudly rather than silently dropping
// the coverage.
test("a spoken line streams each move onto the board, then a voice command submits", async ({
  page,
}) => {
  const errors = collectErrors(page);
  const solutions = solutionsById();

  await pinAndFake(page, 0.95);
  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();

  const { line } = await currentSolution(page, solutions);
  expect(line.length, "the streaming case needs a multi-move line").toBeGreaterThan(1);
  expect(line.some(hasPromotion), "seed 0.95 must select a puzzle whose line promotes").toBe(true);

  const spoken = await spokenLine(page, line);

  await armMic(page);

  // Stream the line the way a continuous recogniser delivers it in one breath: each move
  // arrives as a growing interim, so the *previous* complete move commits while the newest
  // is still held as a preview (confirm-on-next). After speaking move i (0-based), i moves
  // are drawn and move i is the ghost.
  const steps = page.locator(".line__step");
  for (let i = 0; i < spoken.length; i++) {
    await fire(page, spoken.slice(0, i + 1).join(" "), false);
    await expect(steps).toHaveCount(i, { timeout: 2000 });
  }

  // Close the same breath with a single final that also carries the command: "…​ submit".
  // The final settles the last held move *and* fires the command — the whole line then
  // plays out through the same judge a drawn submit runs, and the correct mate reveals the
  // board. (One utterance, so no `just_finalized` reset is in play — the fragile
  // speak-pause-speak-again path is deliberately not what a hands-free solver does.)
  //
  // On reveal the line panel swaps its drawn-arrow steps for the SAN move list, so
  // `.line__step` vanishes; the reveal itself is the assertion. A wrong or short line
  // would not reveal, leaving the steps in place and this failing loudly.
  await fire(page, `${spoken.join(" ")} submit`, true);
  await expect(page.locator(".board--revealed")).toHaveCount(1);
  await expect(page.locator(".verdict")).toContainText("Mate");
  await expect(page.locator(".movelist__ply")).toHaveCount(spoken.length * 2 - 1);

  expect(errors).toEqual([]);
});

// The primary hands-free flow the user described: speak the whole line, then *stop* — a
// pause past the silence threshold submits, no spoken "submit" needed. The last move is
// held as a preview (confirm-on-next) and no recogniser final is fired here, so this also
// pins that the silence-submit commits that held ghost before judging — otherwise the line
// would be short its final move. The timeout is forced to its 2s minimum so the wait is short.
test("a pause past the silence threshold submits the spoken line", async ({ page }) => {
  const errors = collectErrors(page);
  const solutions = solutionsById();

  await pinAndFake(page, 0.95);
  await setSilence(page, 2);
  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();

  const { line } = await currentSolution(page, solutions);
  const spoken = await spokenLine(page, line);

  await armMic(page);

  // Speak the whole line as growing interims — no final, no spoken command. Every move
  // but the last commits; the last is the held preview ghost.
  const steps = page.locator(".line__step");
  for (let i = 0; i < spoken.length; i++) {
    await fire(page, spoken.slice(0, i + 1).join(" "), false);
    await expect(steps).toHaveCount(i, { timeout: 2000 });
  }

  // Then go silent. The countdown elapses, commits the held last move, submits the full
  // line, and the mate reveals — with no further input from the user.
  await expect(page.locator(".board--revealed")).toHaveCount(1, { timeout: 8000 });
  await expect(page.locator(".verdict")).toContainText("Mate");
  await expect(page.locator(".movelist__ply")).toHaveCount(spoken.length * 2 - 1);

  expect(errors).toEqual([]);
});

// The mic has no grace-period timeout: the silence-to-submit countdown only begins once a
// move is actually in. So a mic armed and then left silent — the think time before the
// first move — must NOT submit (which would score a loss on an empty line) and must stay
// armed. This is the exact behaviour the old code got wrong: it started the countdown the
// moment the mic armed, so a silent think would turn the mic off (and, on a non-empty line,
// submit). Uses the 2s minimum timeout and waits comfortably past it.
test("the mic waits without submitting until the first move is spoken", async ({ page }) => {
  const errors = collectErrors(page);

  await pinAndFake(page, 0.95);
  await setSilence(page, 2);
  await page.goto("/");
  await expect(page.locator(".board")).toBeVisible();

  await armMic(page);

  // Stay silent well past the silence threshold — no transcript fired at all.
  await page.waitForTimeout(3500);

  // The mic is still armed and nothing has been submitted: the board stays blind.
  await expect(page.locator(".button--recording")).toHaveCount(1);
  await expect(page.locator(".board--revealed")).toHaveCount(0);

  expect(errors).toEqual([]);
});
