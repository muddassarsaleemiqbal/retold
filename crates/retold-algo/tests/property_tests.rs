use proptest::prelude::*;
use retold_algo::dsp::{estimate_true_peak, extract_rms_envelope, trim_silence};
use retold_algo::linguistics::{
    count_syllables, dale_chall_score, flesch_kincaid_grade_level, flesch_reading_ease,
};
use retold_algo::vision::{Lab, ciede2000};

proptest! {
    #[test]
    fn syllables_non_empty_always_positive(word in "[a-zA-Z]{1,20}") {
        let count = count_syllables(&word);
        prop_assert!(count >= 1);
    }

    #[test]
    fn flesch_scores_never_panic(
        words in 0..10_000_usize,
        sentences in 0..1_000_usize,
        syllables in 0..20_000_usize,
        difficult in 0..10_000_usize,
    ) {
        let _ = flesch_kincaid_grade_level(words, sentences, syllables);
        let _ = flesch_reading_ease(words, sentences, syllables);
        let _ = dale_chall_score(words, sentences, difficult);
    }

    #[test]
    fn ciede2000_symmetry_and_non_negativity(
        l1 in 0.0..100.0_f64,
        a1 in -100.0..100.0_f64,
        b1 in -100.0..100.0_f64,
        l2 in 0.0..100.0_f64,
        a2 in -100.0..100.0_f64,
        b2 in -100.0..100.0_f64,
    ) {
        let c1 = Lab { l: l1, a: a1, b: b1 };
        let c2 = Lab { l: l2, a: a2, b: b2 };
        let de1 = ciede2000(c1, c2);
        let de2 = ciede2000(c2, c1);
        prop_assert!(de1 >= 0.0);
        prop_assert!((de1 - de2).abs() < 1e-4);
    }

    #[test]
    fn trim_silence_invariants(
        samples in prop::collection::vec(-1.0..1.0_f32, 0..1000),
        thresh in -60.0..0.0_f64,
    ) {
        let res = trim_silence(&samples, 48000, thresh);
        prop_assert!(res.start_sample <= res.end_sample);
        prop_assert!(res.end_sample <= samples.len());
        prop_assert_eq!(res.samples.len(), res.end_sample - res.start_sample);
    }

    #[test]
    fn dsp_true_peak_and_rms_never_panic(
        samples in prop::collection::vec(-1.0..1.0_f32, 0..1000)
    ) {
        let tp = estimate_true_peak(&samples);
        prop_assert!(tp <= 10.0);
        let env = extract_rms_envelope(&samples, 48000, 20, 10);
        for pt in env {
            prop_assert!(pt.linear_rms >= 0.0);
        }
    }
}
