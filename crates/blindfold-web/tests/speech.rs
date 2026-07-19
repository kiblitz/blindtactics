//! Tests for the read-aloud voice *opinion*.
//!
//! `speech::voice_score` is pure string logic over what the Web Speech API exposes
//! (name, voiceURI, lang), so the app's choice of voice is pinned here rather than
//! left to whatever a browser happens to do. The cases use real voice identifiers
//! from Apple and Android, the two platforms this is tuned for.

use blindfold_web::speech;

/// Real-ish voice identifiers, so the ordering is checked against what devices
/// actually report.
fn score(name: &str, uri: &str, lang: &str) -> Option<i32> {
    speech::voice_score(name, uri, lang)
}

#[test]
fn non_english_voices_are_never_chosen() {
    assert_eq!(
        score("Thomas", "com.apple.voice.enhanced.fr-FR.Thomas", "fr-FR"),
        None
    );
    assert_eq!(score("Google Deutsch", "", "de-DE"), None);
}

#[test]
fn novelty_and_ancient_voices_are_excluded() {
    // The joke voices are real English voices that would otherwise score — they must
    // be refused outright, not merely ranked low.
    for name in [
        "Zarvox", "Bells", "Trinoids", "Rocko", "Reed", "Sandy", "Fred", "Albert",
    ] {
        assert_eq!(
            score(name, "", "en-US"),
            None,
            "{name} is a novelty/ancient voice and must not be used"
        );
    }
    // ...and espeak/eloquence engines whatever persona they wear.
    assert_eq!(
        score("English (America)", "urn:moz-tts:espeak:en-US", "en-US"),
        None
    );
}

#[test]
fn a_normal_english_voice_is_usable() {
    // Anything English and not excluded is a candidate — better a plain voice than the
    // browser's unknown default.
    assert!(score(
        "Samantha",
        "com.apple.voice.compact.en-US.Samantha",
        "en-US"
    )
    .is_some());
}

#[test]
fn apple_neural_tiers_beat_the_compact_tier() {
    // The tier lives in the voiceURI, not the name — all three are "Samantha".
    let premium = score(
        "Samantha",
        "com.apple.voice.premium.en-US.Samantha",
        "en-US",
    )
    .unwrap();
    let enhanced = score(
        "Samantha",
        "com.apple.voice.enhanced.en-US.Samantha",
        "en-US",
    )
    .unwrap();
    let compact = score(
        "Samantha",
        "com.apple.voice.compact.en-US.Samantha",
        "en-US",
    )
    .unwrap();
    assert!(
        premium > enhanced,
        "premium ({premium}) must beat enhanced ({enhanced})"
    );
    assert!(
        enhanced > compact,
        "enhanced ({enhanced}) must beat compact ({compact})"
    );
}

#[test]
fn a_good_voice_beats_a_low_tier_one_across_platforms() {
    // Android's Google voice and Apple's enhanced voice must both outrank the iPhone
    // compact default — the platform's-worst case this opinion exists to avoid.
    let google = score("Google US English", "", "en-US").unwrap();
    let enhanced = score("Ava", "com.apple.voice.enhanced.en-US.Ava", "en-US").unwrap();
    let compact = score(
        "Samantha",
        "com.apple.voice.compact.en-US.Samantha",
        "en-US",
    )
    .unwrap();
    assert!(
        google > compact,
        "Google ({google}) must beat compact ({compact})"
    );
    assert!(
        enhanced > compact,
        "enhanced Ava ({enhanced}) must beat compact ({compact})"
    );
}

#[test]
fn us_english_is_preferred_over_other_english_variants() {
    // Same tier, different region: the wording assumes US English.
    let us = score(
        "Samantha",
        "com.apple.voice.enhanced.en-US.Samantha",
        "en-US",
    )
    .unwrap();
    let gb = score("Daniel", "com.apple.voice.enhanced.en-GB.Daniel", "en-GB").unwrap();
    assert!(
        us > gb,
        "en-US ({us}) should edge out en-GB ({gb}) at the same tier"
    );
}
