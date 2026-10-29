//! Guide-scoped calibration; absent overrides retain the original estimate exactly.
use retold_core::LanguageTag;
use retold_core::num;
use retold_guide::manifest::GuideManifest;
use retold_lang::text;

pub fn rate(m: &GuideManifest, lang: &LanguageTag) -> f64 {
    m.narration
        .words_per_second
        .get(lang)
        .copied()
        .unwrap_or_else(|| text::words_per_second(lang))
}

pub fn seconds(text: &str, lang: &LanguageTag, m: &GuideManifest) -> f64 {
    let original = text::speech_seconds(text, lang);
    if !m.narration.words_per_second.contains_key(lang) {
        return original;
    }
    let words = num::count_f64(text::word_count(text));
    original + words * (1.0 / rate(m, lang) - 1.0 / text::words_per_second(lang))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_keeps_pauses_and_does_not_change_legacy_or_other_languages() {
        let mut m: GuideManifest = toml::from_str(include_str!(
            "../../retold-guide/tests/fixtures/guides/narrative-fixture/guide.toml"
        ))
        .unwrap();
        let hi = "hi".parse().unwrap();
        let en = "en".parse().unwrap();
        let sentence = "one two, three four.";
        let old = text::speech_seconds(sentence, &hi);
        assert!((seconds(sentence, &hi, &m) - (4.0 / 2.75 + 0.35)).abs() < 1e-12);
        assert_eq!(
            seconds(sentence, &en, &m),
            text::speech_seconds(sentence, &en)
        );
        m.narration.words_per_second.clear();
        assert_eq!(seconds(sentence, &hi, &m), old);
    }
}
