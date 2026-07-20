//! Speech recognition — the input half of voice mode's browser plumbing.
//!
//! Runs **Vosk** (a Kaldi recogniser compiled to WebAssembly) fully in the browser, the
//! mirror image of [`crate::speech`]: that one hands finished strings *to* the browser,
//! this one takes heard strings *from* it. All the interpretation — turning a transcript
//! into a move or a command — is [`blindfold_core::diction`] and [`crate::session::interpret`],
//! so this module only starts and stops the recogniser and forwards each transcript to a
//! Rust callback.
//!
//! # Why Vosk rather than the browser's own recogniser
//!
//! Chrome's `webkitSpeechRecognition` streams audio to Google's *general-purpose* model
//! and — crucially — ignores the Web Speech grammar API, so it cannot be told "expect
//! chess." It confidently returns "rugby" for "rook b" and "rookie" for "rook e". Vosk
//! takes a **grammar** (a fixed word list, [`GRAMMAR`]) and can only ever emit words in
//! it, so the whole class of everyday-word mishears disappears — and the audio never
//! leaves the machine. This is the same engine Lichess uses for its voice input.
//!
//! The cost is a one-time model download (~41 MB) plus a ~6 MB library, both served
//! **same-origin** from `dist/vosk/` (see `fetch-vosk.sh`) and lazy-loaded only when the
//! mic is first armed. A returning visitor pays nothing (browser cache).
//!
//! # Streaming
//!
//! Vosk emits `partialresult` events as the user speaks (forwarded with `is_final == false`)
//! and a `result` event when a phrase settles (`is_final == true`) — the same shape the old
//! wrapper produced, so the streaming commit loop in [`crate::app`] is unchanged.
//!
//! **The mic is paused, not just ignored, while the app speaks.** With the mic on, the
//! recogniser would otherwise hear the app's own text-to-speech and re-parse it as a move.
//! [`pause`] stops feeding it audio (and remembers to resume); [`crate::speech::say`] calls
//! it before speaking and [`resume`] when the utterance ends.
//!
//! Everything degrades to "not available": a browser with no microphone access or no
//! `AudioContext` reports unsupported and the mic control stays hidden.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
// Same-origin asset paths (see fetch-vosk.sh / Trunk.toml). Relative, so they resolve
// against the page whether it is served from a project subpath or a custom domain root.
const VOSK_JS = "vosk/vosk.js";
const MODEL_URL = "vosk/model.tar.gz";

// The recognition grammar: the *only* words Vosk may emit. Everything a move or command
// can contain, plus "[unk]" so silence and off-vocabulary noise become an ignored token
// rather than a phantom chess word. Kept in lockstep with `diction`'s homophone tables —
// diction still does the fuzzy mapping (number words to ranks, "night" to knight), this
// just bounds what can arrive.
const GRAMMAR = JSON.stringify([
  "king", "queen", "rook", "bishop", "knight", "pawn",
  "a", "b", "c", "d", "e", "f", "g", "h",
  "one", "two", "three", "four", "five", "six", "seven", "eight",
  // Castling: the small model's vocabulary has no "kingside"/"queenside", so those are said
  // as "castle short" / "castle long" (which `diction` maps to the sides), or a bare
  // "castle" that the app asks to disambiguate.
  "castle", "short", "long",
  "takes", "check", "mate", "promote", "to",
  "submit", "undo", "clear", "next", "back", "repeat", "done", "enter", "go", "resign",
  "[unk]",
]);

// The loaded model, kept across start/stop so it is downloaded and unpacked only once.
let _model = null;
let _modelLoading = null;

// The live recognition graph, torn down on stop.
let _recognizer = null;
let _audioContext = null;
let _source = null;
let _processor = null;
let _stream = null;

// Desired vs actual, mirroring the old wrapper: `_wantListening` is whether the user has
// the mic armed; `_paused` is a transient stop for the app's own speech. Audio is fed only
// when we want to listen and are not paused.
let _onTranscript = null;
let _wantListening = false;
let _paused = false;

export function bft_recognition_supported() {
  // Vosk needs a microphone, an AudioContext, and WebAssembly. This is true on Chrome,
  // Edge, Firefox, and Safari alike — unlike the old webkitSpeechRecognition, which was
  // Chromium-only. A stubbed `window.Vosk` (the e2e fake) also counts as supported.
  const hasMedia = !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia);
  const hasAudio = typeof (window.AudioContext || window.webkitAudioContext) !== "undefined";
  const hasWasm = typeof WebAssembly !== "undefined";
  return (hasMedia && hasAudio && hasWasm) || typeof window.Vosk !== "undefined";
}

// Inject the Vosk library once and resolve to the global it defines. If something already
// set `window.Vosk` (the e2e stub), use it and skip the network entirely.
function bft_load_vosk_lib() {
  if (window.Vosk) return Promise.resolve(window.Vosk);
  if (window.__bft_vosk_lib) return window.__bft_vosk_lib;
  window.__bft_vosk_lib = new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = VOSK_JS;
    script.onload = () => (window.Vosk ? resolve(window.Vosk) : reject(new Error("vosk lib loaded but no global")));
    script.onerror = () => reject(new Error("failed to load " + VOSK_JS));
    document.head.appendChild(script);
  });
  return window.__bft_vosk_lib;
}

// Load and unpack the model once (~41 MB, cached by the browser afterward).
function bft_ensure_model() {
  if (_model) return Promise.resolve(_model);
  if (_modelLoading) return _modelLoading;
  _modelLoading = bft_load_vosk_lib()
    .then((Vosk) => {
      console.log("recognition: loading speech model (first time only)...");
      return Vosk.createModel(MODEL_URL);
    })
    .then((model) => {
      _model = model;
      console.log("recognition: model ready");
      return model;
    });
  return _modelLoading;
}

async function bft_start_internal() {
  // Kick the mic request off *immediately*, in parallel with the (first-time, ~41 MB)
  // model load — so the permission prompt / stream comes up while the record tap is still
  // fresh rather than after the download. Awaiting the model first (the old order) made the
  // very first arm feel dead: the mic did not activate until the whole model had landed.
  const modelPromise = bft_ensure_model();
  const streamPromise = navigator.mediaDevices.getUserMedia({
    video: false,
    audio: { echoCancellation: true, noiseSuppression: true, channelCount: 1 },
  });
  const model = await modelPromise;
  _stream = await streamPromise;
  // The mic may have been turned off while these were loading; if so, drop the stream.
  if (!_wantListening) { bft_stop_audio(); return; }

  const AudioCtx = window.AudioContext || window.webkitAudioContext;
  _audioContext = new AudioCtx();
  // Vosk resamples to the model's 16 kHz internally, so it must be told the *actual*
  // sample rate of the buffers we feed it — the context's rate, not a requested one.
  _recognizer = new model.KaldiRecognizer(_audioContext.sampleRate, GRAMMAR);
  _recognizer.on("result", (message) => {
    const text = message && message.result && message.result.text;
    if (_onTranscript && text) _onTranscript(text, true);
  });
  _recognizer.on("partialresult", (message) => {
    const partial = message && message.result && message.result.partial;
    if (_onTranscript && partial) _onTranscript(partial, false);
  });

  _source = _audioContext.createMediaStreamSource(_stream);
  _processor = _audioContext.createScriptProcessor(4096, 1, 1);
  _processor.onaudioprocess = (event) => {
    // Skip while paused (the app is speaking) so the recogniser never hears its own TTS.
    if (_recognizer && !_paused) _recognizer.acceptWaveform(event.inputBuffer);
  };
  // A ScriptProcessor only fires while connected to the destination; route through a
  // muted gain node so the mic is not echoed back to the speakers.
  const mute = _audioContext.createGain();
  mute.gain.value = 0;
  _source.connect(_processor);
  _processor.connect(mute);
  mute.connect(_audioContext.destination);
}

function bft_stop_audio() {
  if (_processor) { _processor.onaudioprocess = null; try { _processor.disconnect(); } catch (_) {} _processor = null; }
  if (_source) { try { _source.disconnect(); } catch (_) {} _source = null; }
  if (_recognizer) { try { _recognizer.remove(); } catch (_) {} _recognizer = null; }
  if (_audioContext) { try { _audioContext.close(); } catch (_) {} _audioContext = null; }
  if (_stream) { _stream.getTracks().forEach((t) => t.stop()); _stream = null; }
}

export function bft_recognition_start(onTranscript) {
  _onTranscript = onTranscript;
  _wantListening = true;
  _paused = false;
  bft_stop_audio();
  bft_start_internal().catch((err) => {
    console.log("recognition: could not start —", err && err.message ? err.message : err);
    _wantListening = false;
    bft_stop_audio();
  });
  // Optimistic: the graph comes up asynchronously (permission prompt, first-time model
  // download). Returning true keeps the control armed; a real failure logs and disarms.
  return true;
}

// The echo guard un-gates the mic once the app has finished speaking. It polls
// `speechSynthesis` rather than trusting an utterance's `end` event, which Chrome fires
// unreliably — and sometimes with `speaking` still true inside the handler. That was the
// bug behind a mic that never came back on in read-aloud mode: the roster (or verdict)
// deafened the recogniser and the `end`-driven resume never ran. Polling un-deafens
// robustly whether or not `end` fires. 150 ms is imperceptible against turn-based speech.
let _resumePoll = null;

function bft_clear_resume_poll() {
  if (_resumePoll !== null) { clearInterval(_resumePoll); _resumePoll = null; }
}

export function bft_recognition_stop() {
  _wantListening = false;
  _paused = false;
  bft_clear_resume_poll();
  bft_stop_audio();
}

// Pause for the app's own speech: stop feeding audio but keep `_wantListening`, so the
// graph stays up and a resume just un-gates it. A no-op when the mic is off.
export function bft_recognition_pause() {
  if (!_wantListening || _paused) return;
  _paused = true;
}

// Resume immediately — the mic was re-armed, or nothing will actually speak. Cancels any
// pending resume poll so the two paths do not fight.
export function bft_recognition_resume() {
  bft_clear_resume_poll();
  if (_wantListening && _paused) _paused = false;
}

// Resume once the app finishes speaking. Called by `speech::say` right after `speak()`, so
// the utterance is already queued (`pending`); the poll waits for `speechSynthesis` to go
// idle, then un-gates the mic. Handles a cancel-then-speak chain correctly: while the newer
// utterance is speaking the synth is not idle, so it does not un-deafen mid-sentence. A
// no-op when the mic is off.
export function bft_recognition_resume_after_speech() {
  if (!_wantListening) return;
  if (_resumePoll !== null) return; // already watching
  _resumePoll = setInterval(() => {
    const s = window.speechSynthesis;
    if (!s || (!s.speaking && !s.pending)) {
      bft_clear_resume_poll();
      if (_wantListening) _paused = false;
    }
  }, 150);
}

// Kick off the one-time library + model download *now*, without touching the mic or the
// audio graph — no getUserMedia, no permission prompt, no gesture needed. Called when the
// user shows voice intent (switches to Speak, or lands with Speak persisted), so the ~41 MB
// model is already in memory by the first arm and the mic activates at once instead of
// stalling on the download. Idempotent: the model and the in-flight promise are cached.
export function bft_recognition_warm() {
  bft_ensure_model().catch((err) => {
    console.log("recognition: warm failed —", err && err.message ? err.message : err);
  });
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
    #[wasm_bindgen(js_name = bft_recognition_resume_after_speech)]
    fn resume_after_speech_js();
    #[wasm_bindgen(js_name = bft_recognition_warm)]
    fn warm_js();
}

/// Whether this browser can do speech recognition at all — needs a microphone, an
/// `AudioContext`, and WebAssembly, which every current browser has. The app hides the mic
/// control when this is `false`, so the feature is simply absent rather than a button that
/// does nothing.
pub fn is_supported() -> bool {
    supported_js()
}

/// Preload the ~41 MB model in the background, so the *first* arm activates the mic at once
/// instead of stalling on the download. Touches neither the mic nor the audio graph — no
/// permission prompt, no gesture needed — so it is safe to call the moment the user shows
/// voice intent (switching to Speak, or a page load with Speak persisted). Idempotent and a
/// no-op where recognition is unsupported.
pub fn warm() {
    warm_js();
}

/// Start listening, forwarding each transcript to `on_transcript` as
/// `(transcript, is_final)`. Interim (`is_final == false`) transcripts arrive as the user
/// is still speaking; the same phrase arrives again `is_final == true` once it settles.
/// Returns whether it *began* starting — the graph then comes up asynchronously (the
/// browser prompts for mic permission, and the first call downloads the ~41 MB model), so
/// `true` here is optimistic; a genuine failure logs to the console and disarms itself.
/// Idempotent: starting again replaces any prior session.
///
/// The callback is leaked (`Closure::forget`) rather than returned for the caller to keep
/// alive: it must outlive every recognition event for the whole listening session. One
/// leaked closure per start is a rounding error against a toggle a user presses a handful
/// of times.
pub fn start(on_transcript: impl FnMut(String, bool) + 'static) -> bool {
    let closure = Closure::new(on_transcript);
    let started = start_js(&closure);
    // The JS side holds the callback for the session's lifetime, so keep it alive.
    closure.forget();
    started
}

/// Stop listening for good and tear down the audio graph (the model stays cached). Safe to
/// call when not listening — a no-op then.
pub fn stop() {
    stop_js();
}

/// Pause the recogniser while the app speaks, remembering it should resume.
///
/// Called by [`crate::speech::say`] before it speaks, so the recogniser does not hear its
/// own text-to-speech and re-parse it as a move. Unlike [`stop`], this keeps the graph up
/// and the "we want to listen" intent, so a resume brings it straight back. A no-op when
/// the mic is off.
pub fn pause() {
    pause_js();
}

/// Resume a paused mic immediately. Used when the mic is re-armed, or when nothing will
/// actually be spoken. Cancels any pending [`resume_after_speech`] poll. A no-op if the mic
/// is off or was not paused.
pub fn resume() {
    resume_js();
}

/// Resume the mic once the app has finished speaking, by polling `speechSynthesis` rather
/// than trusting an utterance's `end` event (which Chrome fires unreliably — the cause of a
/// mic that never came back on in read-aloud mode). Called by [`crate::speech::say`] right
/// after it queues an utterance, so the poll sees the pending speech and waits for it. A
/// no-op when the mic is off.
pub fn resume_after_speech() {
    resume_after_speech_js();
}
