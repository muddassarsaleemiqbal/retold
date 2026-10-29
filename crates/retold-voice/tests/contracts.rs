#[cfg(test)]
mod tests {
    use retold_voice::{
        DialogueScript,
        audio::{Wave, accuracy, cosine},
    };
    #[test]
    fn cues_are_strict_and_speakers_are_plan_owned() {
        for bad in [
            "NARRATOR [invented]: Words.",
            "[furious]: Words.",
            "NARRATOR [furious] extra: Words.",
            "NARRATOR ]: Words.",
            "NARRATOR:",
        ] {
            assert!(DialogueScript::parse(bad).is_err(), "{bad}");
        }
        let parsed =
            DialogueScript::parse("NARRATOR [neutral]: Words.\nAlice [joyful]: More words.")
                .unwrap();
        assert!(parsed.validate_speakers(&["Alice".into()]).is_ok());
        assert!(parsed.validate_speakers(&[]).is_err());
    }
    #[test]
    fn real_wav_chunks_and_reference_limits_are_validated() {
        let bytes = include_bytes!("fixtures/speech.wav");
        let original = Wave::decode(bytes).unwrap();
        assert!(original.duration() > 4.0);
        assert!(original.reference().is_err());
        let mut chunked = bytes.to_vec();
        chunked.splice(12..12, b"JUNK\x04\0\0\0test".iter().copied());
        assert_eq!(Wave::decode(&chunked).unwrap().samples, original.samples);
        assert!(Wave::decode(&bytes[..bytes.len() - 2]).is_err());
        let mut reference = original;
        reference
            .samples
            .repeat(3)
            .clone_into(&mut reference.samples);
        assert!(reference.reference().is_ok());
        reference.samples.fill(0.0);
        assert!(reference.reference().is_err());
    }
    #[test]
    fn accuracy_counts_insertions_and_embedding_math_rejects_invalid_vectors() {
        assert_eq!(accuracy("One two three.", "one two three"), 1.0);
        assert!(accuracy("one two three", "one two three extra") < 0.95);
        assert_eq!(accuracy("one two", "wrong"), 0.0);
        assert_eq!(accuracy("बैठकर", "बैठ कर"), 1.0);
        assert_eq!(accuracy("शांत बगीचा", "शान्त बगीचा"), 1.0);
        assert!(accuracy("शांत", "शाथ") < 0.95);
        assert_eq!(accuracy("बैठ कर", "बैठकर"), 1.0);
        assert!(accuracy("carpet", "car pet") < 0.95);
        assert!(accuracy("बैठकर", "बैठ कर extra") < 0.95);
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]).unwrap() - 1.0).abs() < 1e-9);
        assert!(cosine(&[0.0, 0.0], &[1.0, 0.0]).is_err());
        assert!(cosine(&[f64::NAN], &[1.0]).is_err());
    }
}
