use std::str::FromStr;

use retold_core::{LanguageTag, WritingScript};
use retold_lang::align::{SpokenWord, align, align_for_lang};
use retold_lang::captions::{Rules, group};
use retold_lang::profile::profile_for;
use retold_lang::text::speech_seconds;
use retold_lang::units::pack;

fn spoken(words: &[(&str, f64, f64)]) -> Vec<SpokenWord> {
    words
        .iter()
        .map(|(w, s, e)| SpokenWord {
            word: (*w).to_string(),
            start: *s,
            end: *e,
        })
        .collect()
}

fn units(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect()
}

#[test]
fn test_japanese_bunsetsu_and_alignment_exit_criteria() {
    let ja_tag = LanguageTag::from_str("ja").unwrap();
    let profile = profile_for(&ja_tag);
    assert_eq!(profile.language_code(), "ja");
    assert!(profile.is_cjk());

    // Bunsetsu segmentation test without spaces in source script
    let script_text = "日本の首都は東京で、世界最大の都市圏の一つです。";
    let tokens = profile.tokenize_subtitles(script_text);
    assert_ne!(tokens.len(), 0, "Tokens must not be empty");
    assert!(
        tokens.len() >= 4,
        "Japanese text must segment into Bunsetsu phrases, got: {tokens:?}"
    );

    // Verify joined tokens match original without inserting bogus spaces
    let joined = profile.join_tokens(&tokens.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(joined, script_text);

    // Audio-to-script alignment test
    let u = units(&[
        ("u01", "日本の首都は東京で、"),
        ("u02", "世界最大の都市圏の一つです。"),
    ]);

    // Spoken transcript as produced by Whisper Metal
    let s = spoken(&[
        ("日本の", 0.0, 0.7),
        ("首都は", 0.75, 1.4),
        ("東京で", 1.45, 2.1),
        ("世界最大の", 2.2, 3.0),
        ("都市圏の", 3.05, 3.8),
        ("一つです", 3.85, 4.5),
    ]);

    let res = align_for_lang(&u, &s, 4.8, &ja_tag);
    assert!(
        res.is_ok(),
        "Japanese alignment must succeed without AlignError::NoMatch: {res:?}"
    );
    let alignment = res.unwrap();
    assert!(
        alignment.matched >= 0.7,
        "Matched ratio must be >= 70%, got {}",
        alignment.matched
    );
    assert_eq!(alignment.units.len(), 2);
    assert_eq!(alignment.units[0].id, "u01");
    assert_eq!(alignment.units[1].id, "u02");

    // Subtitle caption grouping test (no spaces between Japanese characters)
    let all_timed_words: Vec<_> = alignment
        .units
        .iter()
        .flat_map(|u| u.words.clone())
        .collect();
    let rules = Rules {
        max_words: 3,
        max_chars: 20,
        uppercase: false,
        tail: 0.2,
    };
    let captions = group(&all_timed_words, WritingScript::Jpan, rules);
    assert_ne!(captions.len(), 0);
    for cap in &captions {
        let line = cap.line();
        assert!(
            !line.contains(' '),
            "Japanese caption lines should not contain space separators: {line}"
        );
    }
}

#[test]
fn test_arabic_diacritics_and_alignment_exit_criteria() {
    let ar_tag = LanguageTag::from_str("ar").unwrap();
    let profile = profile_for(&ar_tag);
    assert_eq!(profile.language_code(), "ar");

    // Arabic script with full Harakat / Tashkeel
    let u = units(&[
        ("u01", "فِي يَوْمٍ مِنَ الأَيَّامِ،"),
        ("u02", "كَانَ هُنَاكَ تَاجِرٌ يَعِيشُ فِي بَغْدَادَ."),
    ]);

    // Transcriber returns plain Arabic words without Tashkeel
    let s = spoken(&[
        ("في", 0.0, 0.4),
        ("يوم", 0.45, 0.8),
        ("من", 0.85, 1.1),
        ("الايام", 1.15, 1.8),
        ("كان", 2.0, 2.4),
        ("هناك", 2.45, 2.9),
        ("تاجر", 2.95, 3.5),
        ("يعيش", 3.55, 4.0),
        ("في", 4.05, 4.3),
        ("بغداد", 4.35, 5.0),
    ]);

    let res = align_for_lang(&u, &s, 5.2, &ar_tag);
    assert!(
        res.is_ok(),
        "Arabic alignment must succeed without AlignError::NoMatch despite Tashkeel: {res:?}"
    );
    let alignment = res.unwrap();
    assert!(
        alignment.matched >= 0.7,
        "Arabic matched ratio must be >= 70%, got {}",
        alignment.matched
    );

    // Auto-detection alignment test
    let res_auto = align(&u, &s, 5.2);
    assert!(
        res_auto.is_ok(),
        "Arabic alignment with auto-detection must also succeed: {res_auto:?}"
    );
}

#[test]
fn test_spanish_inverted_punctuation_and_accent_folding() {
    let es_tag = LanguageTag::from_str("es").unwrap();
    let profile = profile_for(&es_tag);
    assert_eq!(profile.language_code(), "es");

    let u = units(&[
        ("u01", "¡Hola amigo!"),
        ("u02", "¿Cómo estás en la ciudad?"),
    ]);

    // Spoken transcription lowercased without inverted marks or accents
    let s = spoken(&[
        ("hola", 0.0, 0.5),
        ("amigo", 0.55, 1.0),
        ("como", 1.2, 1.6),
        ("estas", 1.65, 2.1),
        ("en", 2.15, 2.3),
        ("la", 2.35, 2.5),
        ("ciudad", 2.55, 3.1),
    ]);

    let res = align_for_lang(&u, &s, 3.3, &es_tag);
    assert!(
        res.is_ok(),
        "Spanish alignment must succeed with accent folding: {res:?}"
    );
    let alignment = res.unwrap();
    assert_eq!(alignment.matched, 1.0);
}

#[test]
fn test_turkish_dotted_and_dotless_i_alignment() {
    let tr_tag = LanguageTag::from_str("tr").unwrap();
    let profile = profile_for(&tr_tag);
    assert_eq!(profile.language_code(), "tr");

    let u = units(&[("u01", "İstanbul Boğazı ışıl ışıl parlıyordu.")]);

    // Transcriber spoken forms
    let s = spoken(&[
        ("istanbul", 0.0, 0.8),
        ("boğazı", 0.85, 1.5),
        ("ışıl", 1.55, 2.0),
        ("ışıl", 2.05, 2.5),
        ("parlıyordu", 2.55, 3.4),
    ]);

    let res = align_for_lang(&u, &s, 3.6, &tr_tag);
    assert!(
        res.is_ok(),
        "Turkish alignment must properly fold dotted and dotless I: {res:?}"
    );
    let alignment = res.unwrap();
    assert!(alignment.matched >= 0.8);
}

#[test]
fn test_speech_timing_and_packing_across_languages() {
    let ja_tag = LanguageTag::from_str("ja").unwrap();
    let ar_tag = LanguageTag::from_str("ar").unwrap();
    let es_tag = LanguageTag::from_str("es").unwrap();

    let ja_time = speech_seconds("日本の首都は東京です。", &ja_tag);
    assert!(ja_time > 0.5 && ja_time < 5.0);

    let ar_time = speech_seconds("كان هناك تاجر في بغداد.", &ar_tag);
    assert!(ar_time > 0.5 && ar_time < 5.0);

    let es_time = speech_seconds("El sol brilla sobre la ciudad.", &es_tag);
    assert!(es_time > 0.5 && es_time < 5.0);

    // Test unit packing for Japanese
    let ja_sentences = vec![
        "昔々、ある所に心優しいおじいさんが住んでいました。".to_string(),
        "毎日山へ芝刈りに行きました。".to_string(),
    ];
    let packed = pack(&ja_sentences, [2.0, 5.0], &ja_tag);
    assert_ne!(packed.len(), 0);
}
