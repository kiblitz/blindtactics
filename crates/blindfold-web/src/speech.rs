//! Text to speech — the output half of the voice mode.
//!
//! A thin wrapper over the browser's `speechSynthesis` so the rest of the app reads
//! aloud by calling [`say`] and never touches `web_sys` directly. There is nothing
//! here with a right answer — *what* to say (the roster sentence, the verdict) is
//! built in [`blindfold_core::roster`] and [`crate::session`], the seams a native
//! test can reach. This module only hands a finished string to the browser and picks
//! the voice it is read in.
//!
//! Everything degrades to silence rather than an error: a browser with no speech
//! synthesis, or one that refuses to speak before the first user gesture, simply
//! says nothing while the on-screen text carries on. Audio is an enhancement layered
//! over a UI that already works without it.

use wasm_bindgen::JsCast as _;

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
    let Ok(utterance) = web_sys::SpeechSynthesisUtterance::new_with_text(text) else {
        return;
    };
    // A chosen English voice, so the reading is not the platform's robotic default
    // (Windows' SAPI "David"/"Zira", say). None → leave the browser's default, which is
    // still better than not speaking.
    if let Some(voice) = best_voice(&synthesis) {
        utterance.set_voice(Some(&voice));
    }
    synthesis.speak(&utterance);
}

/// Stop any speech in progress — used when muting, so turning sound off is immediate
/// rather than "after the current sentence finishes".
pub fn silence() {
    if let Some(synthesis) = synthesis() {
        synthesis.cancel();
    }
}

/// The best available English voice, or `None` to fall back to the browser default.
///
/// "Best" is a heuristic over the voice *name*, because that is the only quality
/// signal the API exposes: the natural / neural voices (Microsoft's "Natural",
/// Google's, Apple's "Enhanced") name themselves, and the old robotic desktop voices
/// are known by name too. Only English voices are considered — a French voice reading
/// English is worse than the default — so where none is English this returns `None`.
///
/// The voice list can be empty on the first call (some browsers load it
/// asynchronously), which simply yields the default until the next announcement; since
/// announcements follow user gestures, by then it is populated.
fn best_voice(synthesis: &web_sys::SpeechSynthesis) -> Option<web_sys::SpeechSynthesisVoice> {
    let mut best: Option<(i32, web_sys::SpeechSynthesisVoice)> = None;
    for entry in synthesis.get_voices().iter() {
        let Ok(voice) = entry.dyn_into::<web_sys::SpeechSynthesisVoice>() else {
            continue;
        };
        let lang = voice.lang().to_ascii_lowercase();
        if !lang.starts_with("en") {
            continue;
        }
        let score = quality(&voice.name().to_ascii_lowercase(), &lang);
        if best.as_ref().is_none_or(|(top, _)| score > *top) {
            best = Some((score, voice));
        }
    }
    best.map(|(_, voice)| voice)
}

/// Score an English voice by name, higher being more natural. Tuned to prefer the
/// neural/online voices and gently demote the known robotic desktop ones; the exact
/// numbers only have to order voices on one device, not mean anything absolute.
fn quality(name: &str, lang: &str) -> i32 {
    let mut score = 0;
    // The natural/neural families, best first.
    if name.contains("natural") || name.contains("neural") {
        score += 60;
    }
    if name.contains("enhanced") || name.contains("premium") {
        score += 55;
    }
    if name.contains("siri") {
        score += 50;
    }
    if name.contains("google") {
        score += 45;
    }
    // Nicer online Microsoft voices, named for the person.
    for good in ["aria", "jenny", "guy", "libby", "sonia", "natasha", "ryan"] {
        if name.contains(good) {
            score += 25;
        }
    }
    // The robotic bundled desktop voices — usable, but a last resort.
    for robotic in ["david", "zira", "mark", "hazel", "desktop", "espeak"] {
        if name.contains(robotic) {
            score -= 15;
        }
    }
    // A slight lean toward US English, the variant the wording assumes.
    if lang == "en-us" {
        score += 2;
    }
    score
}

/// The browser's speech-synthesis handle, or `None` where there is none — an old
/// browser, or a headless one. Private so callers deal only in strings.
fn synthesis() -> Option<web_sys::SpeechSynthesis> {
    web_sys::window()?.speech_synthesis().ok()
}
