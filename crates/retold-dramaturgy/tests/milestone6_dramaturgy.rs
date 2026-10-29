use retold_dramaturgy::beat::{BeatError, storybit_benchmark, validate_beat_gates};
use retold_dramaturgy::cadence::{CadenceCategory, CadenceEngine};

#[test]
fn test_storybit_benchmark_validation_and_rejection() {
    // 1. Gold standard arc must pass cleanly
    let gold = storybit_benchmark::gold_standard_arc();
    let metrics = validate_beat_gates(&gold).expect("Gold standard arc must pass gate");
    assert_eq!(metrics.total_beats, 4);
    assert!(metrics.average_delta_e >= 3.0);
    assert!(metrics.has_inversion);

    // 2. Static narrative filler must be rejected at ΔE < 1.0
    let static_arc = storybit_benchmark::static_flawed_arc();
    let err = validate_beat_gates(&static_arc).expect_err("Static arc must fail");
    match err {
        BeatError::StaticBeat {
            index,
            section,
            delta_e,
        } => {
            assert_eq!(index, 0);
            assert_eq!(section, "hook");
            assert!(delta_e < 1.0);
        }
        other => panic!("Expected StaticBeat error, got {other:?}"),
    }
}

#[test]
fn test_cadence_engine_breath_bounds_and_sensory_verbs() {
    let engine = CadenceEngine::new();
    let good_sentences = vec![
        "The steel hatch slammed against the stone wall.".to_string(),
        "He dragged the heavy chest through the dark passage.".to_string(),
        "Could they trust the shadows moving across the courtyard?".to_string(),
    ];

    let report = engine.audit_sentences(&good_sentences);
    assert!(
        report.is_clean,
        "Good sentences should pass: {:?}",
        report.violations
    );
    assert!(report.sensory_verb_density > 5.0);

    let flawed_sentences = vec![
        "Cold wind.".to_string(),
        "In conclusion, we must delve into the rich tapestry of life to remember truth."
            .to_string(),
    ];
    let bad_report = engine.audit_sentences(&flawed_sentences);
    assert!(!bad_report.is_clean);
    assert!(
        bad_report
            .violations
            .iter()
            .any(|v| v.category == CadenceCategory::TooShort)
    );
    assert!(
        bad_report
            .violations
            .iter()
            .any(|v| v.category == CadenceCategory::BannedCliche)
    );
    assert!(
        bad_report
            .violations
            .iter()
            .any(|v| v.category == CadenceCategory::MoralizingConclusion)
    );
}
