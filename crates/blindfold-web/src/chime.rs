//! Short feedback tones — a "ding" on a correct answer, an "err" on a wrong one.
//!
//! A cousin of [`crate::speech`]: both make sound, but this one carries no words. Where
//! the spoken verdict is optional (gated on the read-aloud output mode) and detailed,
//! these are a single, instant cue that a submission landed — a rising two-note ding for
//! a solved mate, a low buzz for a miss. They play on *every* scored result, in any mode:
//! a chime is feedback, not reading aloud, so it is not something the output mode governs.
//!
//! Synthesized with the Web Audio API rather than played from asset files, so there is
//! nothing to download and nothing that could be confused with the read-aloud voice. Like
//! [`crate::recognition`], the audio-graph work is a small `inline_js` block — the tone
//! design (frequencies, envelope) lives there, local to the synthesis, the same way the
//! recogniser's buffer sizes and sample rates do.
//!
//! Degrades to silence where there is no `AudioContext`, exactly like the rest of the
//! audio layer.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
// One shared AudioContext, created lazily on the first tone. A context made before any
// user gesture starts "suspended"; the submit/give-up click (or the mic tap) that
// precedes a verdict gives the page sticky activation, so resume() then succeeds.
let _ctx = null;

function bft_ctx() {
  const AudioCtx = window.AudioContext || window.webkitAudioContext;
  if (!AudioCtx) return null;
  if (!_ctx) _ctx = new AudioCtx();
  if (_ctx.state === "suspended") _ctx.resume();
  return _ctx;
}

// Play one sine note: `freq` Hz, beginning `start` seconds from now, lasting `dur`, with
// a fast attack and an exponential decay so it reads as a soft chime rather than a click.
function bft_note(ctx, freq, start, dur, peak) {
  const t0 = ctx.currentTime + start;
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  osc.type = "sine";
  osc.frequency.value = freq;
  gain.gain.setValueAtTime(0.0001, t0);
  gain.gain.linearRampToValueAtTime(peak, t0 + 0.01);
  gain.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
  osc.connect(gain);
  gain.connect(ctx.destination);
  osc.start(t0);
  osc.stop(t0 + dur);
}

export function bft_chime_correct() {
  const ctx = bft_ctx();
  if (!ctx) return;
  // Two quick ascending notes — an affirmative "ding-ding".
  bft_note(ctx, 660, 0.0, 0.13, 0.18);
  bft_note(ctx, 988, 0.10, 0.20, 0.18);
}

export function bft_chime_wrong() {
  const ctx = bft_ctx();
  if (!ctx) return;
  // One low, short note — a gentle "err", not a harsh alarm.
  bft_note(ctx, 196, 0.0, 0.26, 0.16);
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = bft_chime_correct)]
    fn correct_js();
    #[wasm_bindgen(js_name = bft_chime_wrong)]
    fn wrong_js();
}

/// A rising two-note "ding" — the cue for a correct answer (a solved mate).
pub fn correct() {
    correct_js();
}

/// A low "err" buzz — the cue for a wrong answer (a miss, an overshoot, or a give-up:
/// every result that scores as a loss).
pub fn wrong() {
    wrong_js();
}
