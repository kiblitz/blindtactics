//! Text to speech — the output half of the voice mode.
//!
//! A thin wrapper over the browser's `speechSynthesis` so the rest of the app reads
//! aloud by calling [`say`] and never touches `web_sys` directly. This module also
//! holds the app's *opinion* about which voice to read in — see [`voice_score`]. That
//! opinion is deliberately not a user setting: a picker would push the platform's
//! quirks onto the user, when the app can just choose well.
//!
//! Everything degrades to silence rather than an error: a browser with no speech
//! synthesis, or one that refuses to speak before the first user gesture, simply
//! says nothing while the on-screen text carries on. Audio is an enhancement layered
//! over a UI that already works without it.

use crate::constants;
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
    // A calm, unhurried read — a touch slower and lower than the platform default.
    utterance.set_rate(constants::SPEECH_RATE);
    utterance.set_pitch(constants::SPEECH_PITCH);
    // A chosen voice, so the reading is not the platform's robotic or novelty default.
    // `None` → leave the browser default, which is still better than not speaking.
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

/// Nudge the browser to load its voice list.
///
/// Some browsers (Chrome, Android) populate `getVoices()` asynchronously and return an
/// empty list until they do. Calling it once at startup starts that load, so by the
/// time the user enables sound the good voice is already known — otherwise the *first*
/// announcement falls back to the default and only later ones get the chosen voice.
pub fn warm() {
    if let Some(synthesis) = synthesis() {
        let _ = synthesis.get_voices();
    }
}

/// The best available voice by the app's opinion ([`voice_score`]), or `None` to fall
/// back to the browser default (no English voice, or the list not yet loaded).
fn best_voice(synthesis: &web_sys::SpeechSynthesis) -> Option<web_sys::SpeechSynthesisVoice> {
    let mut best: Option<(i32, web_sys::SpeechSynthesisVoice)> = None;
    for entry in synthesis.get_voices().iter() {
        let Ok(voice) = entry.dyn_into::<web_sys::SpeechSynthesisVoice>() else {
            continue;
        };
        let Some(score) = voice_score(&voice.name(), &voice.voice_uri(), &voice.lang()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(top, _)| score > *top) {
            best = Some((score, voice));
        }
    }
    best.map(|(_, voice)| voice)
}

/// The app's opinion of a voice, higher being better; `None` for one it will not use.
///
/// Pure string logic over the three things the API exposes — name, `voiceURI`, and
/// BCP-47 `lang` — so it is native-tested (`tests/speech.rs`) rather than left to a
/// browser. The signals, tuned for **Apple and Android**, the two platforms that
/// matter here:
///
/// - **Tier lives in the `voiceURI`**, not the name. Apple spells it out
///   (`com.apple.voice.premium.en-US.Ava`, `…enhanced…`, `…compact…`), so the neural
///   tiers win and the low-quality `compact` one is ranked last but still kept (on an
///   iPhone with nothing downloaded it may be the *only* Samantha there is).
/// - **Android's good voices are the Google-named ones** (`Google US English`), which
///   are neural; `google` in the name is a strong signal.
/// - **Novelty and legacy voices are excluded** — the joke voices (`Zarvox`, `Bells`,
///   the newer `Rocko`/`Reed`/`Sandy` gimmicks) and the ancient robotic ones (`Fred`,
///   `Albert`) are English and would otherwise score, so they return `None`.
/// - **English only.** A French voice reading English coordinates is worse than the
///   default, so non-`en` langs are skipped; US English is preferred since the wording
///   assumes it.
pub fn voice_score(name: &str, voice_uri: &str, lang: &str) -> Option<i32> {
    let lang = lang.to_ascii_lowercase();
    if !lang.starts_with("en") {
        return None;
    }
    let name = name.to_ascii_lowercase();
    let uri = voice_uri.to_ascii_lowercase();

    // Joke / novelty / ancient voices — matched on the exact persona name, since these
    // are all real English voices a substring test on quality keywords would miss.
    const NOVELTY: &[&str] = &[
        "albert",
        "bad news",
        "bahh",
        "bells",
        "boing",
        "bubbles",
        "cellos",
        "good news",
        "jester",
        "organ",
        "pipe organ",
        "trinoids",
        "whisper",
        "wobble",
        "zarvox",
        "deranged",
        "hysterical",
        "superstar",
        "fred",
        "ralph",
        "kathy",
        "junior",
        "princess",
        "bruce",
        "agnes",
        "vicki",
        "victoria",
        "grandma",
        "grandpa",
        "rocko",
        "sandy",
        "shelley",
        "flo",
        "reed",
        "eddy",
    ];
    if NOVELTY.contains(&name.as_str()) {
        return None;
    }
    // Robotic engines, whatever persona name they wear.
    if name.contains("eloquence") || uri.contains("eloquence") || uri.contains("espeak") {
        return None;
    }

    let has = |needle: &str| name.contains(needle) || uri.contains(needle);
    let mut score = 0;

    // Language variant: the wording assumes US English, then British, then any English.
    score += if lang.starts_with("en-us") {
        6
    } else if lang.starts_with("en-gb") {
        3
    } else {
        1
    };

    // Neural / premium tiers — the big wins, and cross-platform.
    if has("neural") || has("natural") {
        score += 60;
    }
    if has("premium") {
        score += 55;
    }
    if has("siri") {
        score += 55;
    }
    if has("enhanced") {
        score += 50;
    }
    if has("google") {
        score += 50;
    }

    // Modern, natural-sounding Apple personas — good even at the default tier, so a name
    // match earns a lift on top of any tier the URI reveals.
    const APPLE_GOOD: &[&str] = &[
        "ava",
        "samantha",
        "allison",
        "susan",
        "zoe",
        "nicky",
        "evan",
        "nathan",
        "joelle",
        "noelle",
        "daniel",
        "kate",
        "serena",
        "stephanie",
        "oliver",
        "martha",
        "karen",
        "matilda",
        "moira",
        "tessa",
        "fiona",
        "rishi",
        "gordon",
    ];
    if APPLE_GOOD.iter().any(|persona| name.starts_with(persona)) {
        score += 25;
    }
    if name.starts_with("ava") {
        score += 12; // Apple's most natural US voice.
    }

    // The low-quality on-device tier: kept as a last resort, ranked below anything neural.
    if has("compact") {
        score -= 25;
    }

    Some(score)
}

/// The browser's speech-synthesis handle, or `None` where there is none — an old
/// browser, or a headless one. Private so callers deal only in strings.
fn synthesis() -> Option<web_sys::SpeechSynthesis> {
    web_sys::window()?.speech_synthesis().ok()
}
