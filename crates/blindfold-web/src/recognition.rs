//! Speech recognition — the input half of voice mode's browser plumbing.
//!
//! A thin wrapper over the browser's `webkitSpeechRecognition`, the mirror image of
//! [`crate::speech`]: that one hands finished strings *to* the browser, this one takes
//! heard strings *from* it. All the interpretation — turning a transcript into a move
//! or a command — is [`blindfold_core::diction`] and [`crate::session::interpret`], so
//! this module only starts and stops the recogniser and forwards each transcript to a
//! Rust callback.
//!
//! **Interim results are on**, so the callback fires as the user is still speaking
//! (with `is_final == false`) and again when the phrase settles (`is_final == true`).
//! That is what lets the board *stream* the move — draw a provisional arrow as the
//! words arrive — and commit it only when final. The caller decides; this module just
//! forwards both.
//!
//! **The mic is paused, not just ignored, while the app speaks.** With the mic on, the
//! recogniser would otherwise hear the app's own text-to-speech and re-parse it as a
//! move — an echo that draws a spurious arrow. [`pause`] stops the recogniser (and
//! remembers it should resume); [`crate::speech::say`] calls it before speaking and
//! [`resume`] when the utterance ends. Genuinely stopping it, rather than dropping
//! transcripts inside a time window, is the robust version: nothing is heard to
//! mishear.
//!
//! Why `inline_js` rather than `web-sys`: the recognition types in `web-sys` are gated
//! behind the `web_sys_unstable_apis` cfg (the recognition half of the Web Speech API
//! is not a finished standard), which would push a build flag onto `trunk` and CI. A
//! couple dozen lines of JS keep the browser quirks — the `webkit` prefix, the lazy
//! voice list, the auto-restart on silence — in one legible place and off the build.
//!
//! Like `speech`, everything degrades to "not available": a browser without recognition
//! (Firefox, or anywhere offline — Chrome streams audio to Google) simply reports
//! unsupported and the mic control stays hidden.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
// The active recognition object, or null. `_wantListening` is the *desired* state —
// whether the user has the mic armed — kept apart from whether it is running right now,
// because the app pauses the running recogniser for its own speech without the user
// having turned anything off. `_paused` is that pause; `onend` restarts only when we
// want to listen and are not paused.
let _recognition = null;
let _wantListening = false;
let _paused = false;
let _onTranscript = null;

export function bft_recognition_supported() {
  return typeof (window.SpeechRecognition || window.webkitSpeechRecognition) !== "undefined";
}

function bft_make() {
  const Recognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!Recognition) return null;
  const recognition = new Recognition();
  recognition.lang = "en-US";
  // continuous = true: the user is not made to pause between moves. Chrome then batches a
  // whole spoken line into its results and finalises late, so we forward the *entire*
  // accumulated transcript on every event (below) and let the Rust side segment it into
  // moves and stream them — see `diction::parse_line` and `app::handle_voice`.
  recognition.continuous = true;
  // Interim results let the board preview a move as it is spoken and let earlier moves in
  // a line commit before the line is finished.
  recognition.interimResults = true;
  // Forward the full transcript so far — every result concatenated — not just the newest
  // slice. The parser wants the whole line ("queen f5 queen g6") to segment it; is_final
  // is true only once *every* result has settled, i.e. the whole utterance is done.
  recognition.onresult = (event) => {
    if (!_onTranscript) return;
    let full = "";
    let allFinal = true;
    for (let i = 0; i < event.results.length; i++) {
      full += event.results[i][0].transcript + " ";
      if (!event.results[i].isFinal) allFinal = false;
    }
    _onTranscript(full.trim(), allFinal);
  };
  // The recogniser stops itself after a long enough silence. While we still want to
  // listen and are not deliberately paused (for the app's own speech), start it again —
  // so a session outlives the pause after the user finishes a line and stays hands-free
  // rather than dying at the first silence.
  recognition.onend = () => {
    if (_recognition === recognition) {
      _recognition = null;
      if (_wantListening && !_paused) bft_start_internal();
    }
  };
  return recognition;
}

function bft_start_internal() {
  const recognition = bft_make();
  if (!recognition) return false;
  _recognition = recognition;
  try { recognition.start(); } catch (_) { _recognition = null; return false; }
  return true;
}

function bft_stop_current() {
  const recognition = _recognition;
  _recognition = null;
  if (recognition) {
    recognition.onend = null;
    try { recognition.stop(); } catch (_) {}
  }
}

export function bft_recognition_start(onTranscript) {
  _onTranscript = onTranscript;
  _wantListening = true;
  _paused = false;
  bft_stop_current();
  return bft_start_internal();
}

export function bft_recognition_stop() {
  _wantListening = false;
  _paused = false;
  bft_stop_current();
}

// Pause for the app's own speech: stop the recogniser but keep `_wantListening`, so
// `onend` will not restart it until `resume`. A no-op when the mic is off.
export function bft_recognition_pause() {
  if (!_wantListening || _paused) return;
  _paused = true;
  bft_stop_current();
}

export function bft_recognition_resume() {
  if (_wantListening && _paused) {
    _paused = false;
    bft_start_internal();
  }
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = bft_recognition_supported)]
    fn supported_js() -> bool;
    #[wasm_bindgen(js_name = bft_recognition_start)]
    fn start_js(on_transcript: &Closure<dyn FnMut(String, bool)>) -> bool;
    #[wasm_bindgen(js_name = bft_recognition_stop)]
    fn stop_js();
    #[wasm_bindgen(js_name = bft_recognition_pause)]
    fn pause_js();
    #[wasm_bindgen(js_name = bft_recognition_resume)]
    fn resume_js();
}

/// Whether this browser can do speech recognition at all — Chrome / Edge / Android
/// Chrome / iOS Safari 14.5+, not Firefox. The app hides the mic control when this is
/// `false`, so the feature is simply absent rather than a button that does nothing.
pub fn is_supported() -> bool {
    supported_js()
}

/// Start listening, forwarding each transcript to `on_transcript` as
/// `(transcript, is_final)`. Interim (`is_final == false`) transcripts arrive as the
/// user is still speaking; the same phrase arrives again `is_final == true` once it
/// settles. Returns whether it started — `false` if recognition is unsupported or the
/// browser refused (no mic permission, no gesture). Idempotent: starting again replaces
/// any prior session, so a re-toggle does not stack recognisers.
///
/// The callback is leaked (`Closure::forget`) rather than returned for the caller to
/// keep alive: it must outlive every `onresult` the browser fires, which continues
/// across the auto-restart on silence and across a [`pause`]/[`resume`] for the app's
/// speech, for the whole listening session. One leaked closure per start is a rounding
/// error against a toggle a user presses a handful of times.
pub fn start(on_transcript: impl FnMut(String, bool) + 'static) -> bool {
    let closure = Closure::new(on_transcript);
    if start_js(&closure) {
        closure.forget();
        true
    } else {
        // Unsupported or refused: the browser never took a reference to the closure, so
        // dropping it here frees it rather than leaking.
        false
    }
}

/// Stop listening for good. Safe to call when not listening — a no-op then.
pub fn stop() {
    stop_js();
}

/// Pause the recogniser while the app speaks, remembering it should resume.
///
/// Called by [`crate::speech::say`] before it speaks, so the recogniser does not hear
/// its own text-to-speech and re-parse it as a move. Unlike [`stop`], this keeps the
/// "we want to listen" intent, so [`resume`] can bring it straight back. A no-op when
/// the mic is off.
pub fn pause() {
    pause_js();
}

/// Resume a mic paused by [`pause`]. Called when the app's utterance ends. A no-op if
/// the mic is off or was not paused.
pub fn resume() {
    resume_js();
}
