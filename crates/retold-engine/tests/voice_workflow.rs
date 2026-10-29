#[cfg(test)]
mod tests {
    #[expect(
        dead_code,
        reason = "each test binary uses a different part of the shared support module"
    )]
    mod support {
        include!("support/mod.rs");
    }
    use retold_core::Usd;
    use retold_engine::{context::Context, languages::LocalUnit, voices};
    use retold_guide::{Guide, Origin};
    use std::sync::atomic::Ordering;
    fn enabled(context: &Context) -> Context {
        let mut result = context.clone();
        let mut files = result.guide.files().clone();
        let manifest = files.get_mut("guide.toml").unwrap();
        *manifest = manifest
            .replace("mode = \"continuous\"", "mode = \"dialogue\"")
            .replace(
                "target_seconds = [96.0, 110.0]",
                "target_seconds = [13.0, 20.0]",
            );
        // Fixture formatting varies; edit the parsed manifest through TOML.
        let mut value: toml::Value = toml::from_str(manifest).unwrap();
        value["narration"]["mode"] = "dialogue".into();
        value["format"]["target_seconds"] = toml::Value::Array(vec![13.0.into(), 20.0.into()]);
        value["edit"]["ending"]["seconds"] = 0.48.into();
        *manifest = toml::to_string(&value).unwrap();
        result.guide = Guide::from_files(Origin::Inline, files).unwrap();
        result
    }
    fn units() -> Vec<LocalUnit> {
        ["NARRATOR [neutral]: We listen carefully to every voice and remember the story together.","Alice [joyful]: We listen carefully to every voice and remember the story together.","Bob [solemn]: We listen carefully to every voice and remember the story together."].into_iter().enumerate().map(|(id,text)|LocalUnit{id,section:"hook".into(),text:text.into()}).collect()
    }
    #[tokio::test]
    async fn recorded_turns_resume_and_selected_redo_never_repeat_other_paid_calls() {
        let (_tmp, ctx, _text, media) = support::setup(&["en"], Usd::from_micros(300_000));
        media.recorded_speech.store(true, Ordering::SeqCst);
        let ctx = enabled(&ctx);
        let lang = "en".parse().unwrap();
        let speakers = vec!["Alice".into(), "Bob".into()];
        let first = voices::narrate(&ctx, &lang, &units(), &speakers)
            .await
            .unwrap();
        assert_eq!(first.turns.len(), 3);
        assert_eq!(
            first
                .turns
                .iter()
                .map(|t| t.track.as_str())
                .collect::<Vec<_>>(),
            ["A1", "A2", "A1"]
        );
        assert!(first.turns.iter().all(|t| t.accuracy >= 0.95));
        assert_eq!(first.alignment.units.len(), 3);
        assert!(!first.alignment.units[0].words[0].text.contains("NARRATOR"));
        let counts = (
            media.speech.load(Ordering::SeqCst),
            media.transcripts.load(Ordering::SeqCst),
        );
        let spend = ctx.media.ledger.total().unwrap();
        let stamps = first
            .turns
            .iter()
            .map(|t| {
                std::fs::metadata(&t.audio.path)
                    .unwrap()
                    .modified()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        voices::narrate(&ctx, &lang, &units(), &speakers)
            .await
            .unwrap();
        assert_eq!(
            counts,
            (
                media.speech.load(Ordering::SeqCst),
                media.transcripts.load(Ordering::SeqCst)
            )
        );
        assert_eq!(ctx.media.ledger.total().unwrap(), spend);
        ctx.media
            .store
            .lock()
            .await
            .bump_revision("voices-1")
            .unwrap();
        let redone = voices::narrate(&ctx, &lang, &units(), &speakers)
            .await
            .unwrap();
        assert_eq!(media.speech.load(Ordering::SeqCst), counts.0 + 1);
        for i in [0, 2] {
            assert_eq!(
                std::fs::metadata(&redone.turns[i].audio.path)
                    .unwrap()
                    .modified()
                    .unwrap(),
                stamps[i]
            );
        }
    }
    #[tokio::test]
    async fn unknown_cues_and_failed_accuracy_never_activate_a_scene() {
        let (_tmp, ctx, _text, media) = support::setup(&["en"], Usd::ZERO);
        media.recorded_speech.store(true, Ordering::SeqCst);
        let ctx = enabled(&ctx);
        let lang = "en".parse().unwrap();
        let speakers = vec!["Alice".into(), "Bob".into()];
        assert!(voices::narrate(&ctx, &lang, &units(), &[]).await.is_err());
        assert_eq!(media.speech.load(Ordering::SeqCst), 0);
        media.inaccurate_speech.store(true, Ordering::SeqCst);
        let error = voices::narrate(&ctx, &lang, &units(), &speakers)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("95%"), "{error}");
        assert_eq!(media.speech.load(Ordering::SeqCst), 2);
        assert!(!voices::file(&ctx, &lang).exists());
    }
    #[tokio::test]
    async fn interrupted_scene_reopens_and_reuses_finished_paid_turn() {
        let (_tmp, original, _text, media) = support::setup(&["en"], Usd::from_micros(300_000));
        media.recorded_speech.store(true, Ordering::SeqCst);
        media.block_second_quote.store(true, Ordering::SeqCst);
        let ctx = enabled(&original);
        drop(original);
        let running = ctx.clone();
        let task = tokio::spawn(async move {
            voices::narrate(
                &running,
                &"en".parse().unwrap(),
                &units(),
                &["Alice".into(), "Bob".into()],
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !media.quote_blocked.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        ctx.media.cancel.cancel();
        assert!(task.await.unwrap().is_err());
        assert!(!voices::file(&ctx, &"en".parse().unwrap()).exists());
        assert_eq!(media.speech.load(Ordering::SeqCst), 1);
        let mut resumed = ctx.clone();
        resumed.media.cancel = tokio_util::sync::CancellationToken::new();
        let layout = ctx.layout.clone();
        let (_other_tmp, other, _, _) = support::setup(&["en"], Usd::ZERO);
        resumed.media.store = std::sync::Arc::clone(&other.media.store);
        drop(ctx);
        // The cancelled task has dropped its copy; releasing the final Store
        // reference releases its process lock, just as process termination does.
        resumed.media.store = std::sync::Arc::new(tokio::sync::Mutex::new(
            retold_engine::store::Store::open(&layout).unwrap(),
        ));
        media.block_second_quote.store(false, Ordering::SeqCst);
        voices::narrate(
            &resumed,
            &"en".parse().unwrap(),
            &units(),
            &["Alice".into(), "Bob".into()],
        )
        .await
        .unwrap();
        assert_eq!(media.speech.load(Ordering::SeqCst), 3);
        assert_eq!(media.transcripts.load(Ordering::SeqCst), 3);
        assert_eq!(
            resumed.media.ledger.total().unwrap(),
            Usd::from_micros(30_000)
        );
    }
    #[tokio::test]
    async fn ambiguous_paid_speech_stops_and_never_replays_on_resume() {
        let (_tmp, ctx, _text, media) = support::setup(&["en"], Usd::from_micros(300_000));
        media.recorded_speech.store(true, Ordering::SeqCst);
        media.fail_speech.store(true, Ordering::SeqCst);
        let ctx = enabled(&ctx);
        for _ in 0..2 {
            assert!(
                voices::narrate(
                    &ctx,
                    &"en".parse().unwrap(),
                    &units(),
                    &["Alice".into(), "Bob".into()]
                )
                .await
                .is_err()
            );
        }
        assert_eq!(media.speech.load(Ordering::SeqCst), 1);
        assert!(!voices::file(&ctx, &"en".parse().unwrap()).exists());
    }
    #[test]
    fn script_gate_rejects_unknown_speakers_and_does_not_count_cues_as_speech() {
        use retold_engine::story::{self, Script, SectionText};
        let script = Script {
            sections: vec![SectionText {
                section: "hook".into(),
                text: "Alice [joyful]: One two three.\nNARRATOR [neutral]: Four five six.".into(),
            }],
        };
        assert!(story::dialogue_gates(&script, &["Alice".into()]).is_empty());
        assert_eq!(story::dialogue_gates(&script, &[]).len(), 1);
        let mut m = retold_guide::catalog::bundled()
            .unwrap()
            .remove(0)
            .manifest()
            .clone();
        m.narration.mode = retold_guide::manifest::NarrationMode::Dialogue;
        let length = m.length.get_mut(&m.format.source_language).unwrap();
        length.words = Some([6, 6]);
        length.seconds = None;
        assert!(story::gates(&script, &m).iter().all(|v| v.gate != "length"));
    }
    #[tokio::test]
    async fn unavailable_local_clone_does_not_block_an_explicit_cloud_fallback() {
        use retold_engine::media_runtime::MediaRegistry;
        use retold_providers::{ProviderConfig, media::MediaProvider};
        use std::sync::Arc;
        let (_tmp, original, _text, cloud) = support::setup(&["en"], Usd::ZERO);
        cloud.recorded_speech.store(true, Ordering::SeqCst);
        let mut ctx = enabled(&original);
        let sample = ctx.channel.dir().join("reference.wav");
        let mut wave = retold_voice::audio::Wave::decode(include_bytes!(
            "../../retold-voice/tests/fixtures/speech.wav"
        ))
        .unwrap();
        wave.samples = wave.samples.repeat(3);
        std::fs::write(sample, wave.encode().unwrap()).unwrap();
        let path = ctx.channel.dir().join("channel.toml");
        let mut config: toml::Value =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        config["voices"]["en"] = toml::Value::try_from(serde_json::from_value::<retold_guide::channel::VoiceSpec>(serde_json::json!({
            "provider":"local", "model":"chatterbox", "voice":"narrator", "sample":"reference.wav",
            "fallbacks":[{"provider":"openrouter","model":"model","voice":"narrator"}]
        })).unwrap()).unwrap();
        std::fs::write(path, toml::to_string(&config).unwrap()).unwrap();
        ctx.channel = retold_guide::Channel::load(ctx.channel.dir()).unwrap();
        let local = Arc::new(support::MediaFake {
            id_override: Some("local"),
            ..Default::default()
        });
        local.unavailable_speech.store(true, Ordering::SeqCst);
        let mut providers = ctx.media.registry.config.clone();
        providers.providers.insert(
            "local".into(),
            ProviderConfig::LocalVoice {
                espeak: "unused".into(),
                max_parallel: 1,
                timeout_secs: 30,
            },
        );
        ctx.media.registry = Arc::new(MediaRegistry::from_providers(
            providers,
            vec![
                (Arc::clone(&local) as Arc<dyn MediaProvider>, 1),
                (Arc::clone(&cloud) as Arc<dyn MediaProvider>, 1),
            ],
        ));
        let mut turns = units();
        for turn in &mut turns {
            turn.text = turns_text();
        }
        let scene = voices::narrate(&ctx, &"en".parse().unwrap(), &turns, &[])
            .await
            .unwrap();
        assert!(
            scene
                .turns
                .iter()
                .all(|t| t.fallback && t.audio.provider == "openrouter")
        );
        assert_eq!(local.speech.load(Ordering::SeqCst), 3);
        assert_eq!(cloud.speech.load(Ordering::SeqCst), 3);
    }
    fn turns_text() -> String {
        "NARRATOR [neutral]: We listen carefully to every voice and remember the story together."
            .into()
    }
    #[tokio::test]
    async fn changing_local_asr_weights_rechecks_turns_without_repeating_speech() {
        use retold_engine::media_runtime::MediaRegistry;
        use retold_providers::{ProviderConfig, media::MediaProvider};
        use std::sync::Arc;
        let (_tmp, original, _text, media) = support::setup(&["en"], Usd::ZERO);
        media.recorded_speech.store(true, Ordering::SeqCst);
        let mut ctx = enabled(&original);
        let model = ctx.layout.meta().join("asr-fixture.bin");
        std::fs::write(&model, b"fixture weights v1").unwrap();
        let mut config = ctx.media.registry.config.clone();
        // The fixture provider implements both media methods; declare its ASR
        // weights only to exercise the real cache identity without inference.
        config.providers.insert(
            "openrouter".into(),
            ProviderConfig::LocalWhisper {
                binary: "unused".into(),
                model: model.clone(),
                ffmpeg: "unused".into(),
                max_parallel: 1,
                timeout_secs: 30,
            },
        );
        ctx.media.registry = Arc::new(MediaRegistry::from_providers(
            config,
            vec![(Arc::clone(&media) as Arc<dyn MediaProvider>, 1)],
        ));
        let speakers = vec!["Alice".into(), "Bob".into()];
        voices::narrate(&ctx, &"en".parse().unwrap(), &units(), &speakers)
            .await
            .unwrap();
        assert_eq!(media.speech.load(Ordering::SeqCst), 3);
        assert_eq!(media.transcripts.load(Ordering::SeqCst), 3);
        std::fs::write(model, b"different fixture weights v2").unwrap();
        voices::narrate(&ctx, &"en".parse().unwrap(), &units(), &speakers)
            .await
            .unwrap();
        assert_eq!(media.speech.load(Ordering::SeqCst), 3);
        assert_eq!(media.transcripts.load(Ordering::SeqCst), 6);
    }
}
