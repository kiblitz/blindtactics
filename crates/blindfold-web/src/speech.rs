//! Text to speech — the output half of the voice mode.
//!
//! A thin wrapper over the browser's `speechSynthesis` so the rest of the app reads
//! aloud by calling [`say`] and never touches `web_sys` directly. There is nothing
//! here with a right answer — *what* to say (the roster sentence, the verdict) is
//! built in [`blindfold_core::roster`] and [`crate::session`], the seams a native
//! test can reach. This module only hands a finished string to the browser.
//!
//! Everything degrades to silence rather than an error: a browser with no speech
//! synthesis, or one that refuses to speak before the first user gesture, simply
//! says nothing while the on-screen text carries on. Audio is an enhancement layered
//! over a UI that already works without it.

/// Speak `text` aloud, cancelling whatever was being said first.
///
/// The cancel is deliberate: a new puzzle's announcement should not wait in a queue
/// behind the previous one, and a verdict should interrupt a roster still being read
/// — the user has moved on, so the voice should too.
pub fn say(text: &str) {
    let Some(synthesis) = synthesis() else {
        return;
    };
    synthesis.cancel();
    if let Ok(utterance) = web_sys::SpeechSynthesisUtterance::new_with_text(text) {
        synthesis.speak(&utterance);
    }
}

/// Stop any speech in progress — used when muting, so turning sound off is immediate
/// rather than "after the current sentence finishes".
pub fn silence() {
    if let Some(synthesis) = synthesis() {
        synthesis.cancel();
    }
}

/// The browser's speech-synthesis handle, or `None` where there is none — an old
/// browser, or a headless one. Private so callers deal only in strings.
fn synthesis() -> Option<web_sys::SpeechSynthesis> {
    web_sys::window()?.speech_synthesis().ok()
}
