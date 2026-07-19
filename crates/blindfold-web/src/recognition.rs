//! Speech recognition — the input half of voice mode's browser plumbing.
//!
//! A thin wrapper over the browser's `webkitSpeechRecognition`, the mirror image of
//! [`crate::speech`]: that one hands finished strings *to* the browser, this one takes
//! heard strings *from* it. All the interpretation — turning a transcript into a move
//! or a command — is [`blindfold_core::diction`] and [`crate::session::interpret`], so
//! this module only starts and stops the recogniser and forwards each final transcript
//! to a Rust callback.
//!
//! Why `inline_js` rather than `web-sys`: the recognition types in `web-sys` are gated
//! behind the `web_sys_unstable_apis` cfg (the recognition half of the Web Speech API
//! is not a finished standard), which would push a build flag onto `trunk` and CI. A
//! dozen lines of JS keep the browser quirks — the `webkit` prefix, the lazy voice
//! list, the auto-restart on silence — in one legible place and off the build.
//!
//! Like `speech`, everything degrades to "not available": a browser without recognition
//! (Firefox, or anywhere offline — Chrome streams audio to Google) simply reports
//! unsupported and the mic control stays hidden.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
let _recognition = null;
// While the app is speaking, the recogniser hears its own text-to-speech and would
// re-parse it as input — an echo that could draw a spurious move. Transcripts finalised
// before this time (set whenever the app speaks) are dropped. Turn-based UX means the
// user is not talking while the app talks, so this cannot swallow real input.
let _suppressUntil = 0;

export function bft_recognition_supported() {
  return typeof (window.SpeechRecognition || window.webkitSpeechRecognition) !== "undefined";
}

export function bft_recognition_suppress(ms) {
  _suppressUntil = Date.now() + ms;
}

export function bft_recognition_start(onTranscript) {
  const Recognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!Recognition) return false;
  bft_recognition_stop();
  const recognition = new Recognition();
  recognition.lang = "en-US";
  recognition.continuous = true;
  recognition.interimResults = false;
  recognition.onresult = (event) => {
    if (Date.now() < _suppressUntil) return;
    for (let i = event.resultIndex; i < event.results.length; i++) {
      const result = event.results[i];
      if (result.isFinal) onTranscript(result[0].transcript);
    }
  };
  // The recogniser stops itself after a pause; while listening is on, start it again so
  // the session stays hands-free rather than dying at the first silence.
  recognition.onend = () => {
    if (_recognition === recognition) {
      try { recognition.start(); } catch (_) {}
    }
  };
  _recognition = recognition;
  try { recognition.start(); } catch (_) { _recognition = null; return false; }
  return true;
}

export function bft_recognition_stop() {
  const recognition = _recognition;
  _recognition = null;
  if (recognition) {
    recognition.onend = null;
    try { recognition.stop(); } catch (_) {}
  }
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = bft_recognition_supported)]
    fn supported_js() -> bool;
    #[wasm_bindgen(js_name = bft_recognition_suppress)]
    fn suppress_js(ms: f64);
    #[wasm_bindgen(js_name = bft_recognition_start)]
    fn start_js(on_transcript: &Closure<dyn FnMut(String)>) -> bool;
    #[wasm_bindgen(js_name = bft_recognition_stop)]
    fn stop_js();
}

/// Whether this browser can do speech recognition at all — Chrome / Edge / Android
/// Chrome / iOS Safari 14.5+, not Firefox. The app hides the mic control when this is
/// `false`, so the feature is simply absent rather than a button that does nothing.
pub fn is_supported() -> bool {
    supported_js()
}

/// Start listening, forwarding each final transcript to `on_transcript`. Returns
/// whether it started — `false` if recognition is unsupported or the browser refused
/// (no mic permission, no gesture). Idempotent: starting again replaces any prior
/// session, so a re-toggle does not stack recognisers.
///
/// The callback is leaked (`Closure::forget`) rather than returned for the caller to
/// keep alive: it must outlive every `onresult` the browser fires, which continues
/// across the auto-restart on silence, for the whole listening session. [`stop`] ends
/// that session. One leaked closure per start is a rounding error against a toggle a
/// user presses a handful of times.
pub fn start(on_transcript: impl FnMut(String) + 'static) -> bool {
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

/// Stop listening. Safe to call when not listening — a no-op then.
pub fn stop() {
    stop_js();
}

/// Ignore any transcript finalised within the next `ms` milliseconds.
///
/// Called by [`crate::speech::say`] whenever the app speaks, so the recogniser does not
/// hear its own text-to-speech and re-parse it as a move. A no-op when not listening
/// (it only sets a timestamp the running recogniser consults). The flow is turn-based —
/// the user waits for the app to finish speaking — so this cannot swallow real input.
pub fn suppress(ms: f64) {
    suppress_js(ms);
}
