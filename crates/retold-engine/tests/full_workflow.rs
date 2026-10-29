//! Whole-workflow tests stop before any real editor launch or video export.
#[cfg(test)]
mod tests {
    mod support {
        include!("support/mod.rs");
    }
    use retold_core::Usd;
    use retold_engine::Stage;
    use retold_engine::media_runtime::MediaError;
    use retold_engine::pipeline::Until;
    use retold_engine::workflow::{self, Options, WorkflowError};
    use std::sync::{Arc, atomic::Ordering};
    use support::*;
    fn options() -> Options {
        Options {
            fonts: Some(Arc::new(EstimatedFonts)),
            ..Options::default()
        }
    }
    #[tokio::test]
    async fn full_language_matrix_reaches_ready_and_resume_calls_no_provider() {
        let (tmp, ctx, text, media) =
            setup(&["en:en", "hi:hi,en.srt", "ur:ur", "ur:hi"], Usd::ZERO);
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        let manifest: workflow::Manifest = serde_json::from_slice(
            &std::fs::read(ctx.layout.deliverables().join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.editions.len(), 4);
        assert!(!manifest.video_approved);
        assert!(
            manifest
                .editions
                .iter()
                .all(|e| e.timeline.exists() && !e.video.exists())
        );
        assert_eq!(media.speech.load(Ordering::SeqCst), 3);
        assert_eq!(media.transcripts.load(Ordering::SeqCst), 3);
        assert!(media.max_active.load(Ordering::SeqCst) <= 2);
        assert!(media.max_active.load(Ordering::SeqCst) >= 2);
        assert!(ctx.layout.review_page().exists());
        assert!(
            ctx.layout
                .subtitle_file(&"hi".parse().unwrap(), &"en".parse().unwrap(), "srt")
                .exists()
        );
        let editions: Vec<retold_engine::languages::Edition> = serde_json::from_slice(
            &std::fs::read(ctx.layout.stage(Stage::Captions).join("editions.json")).unwrap(),
        )
        .unwrap();
        assert!(editions.iter().all(|e| e.alignment.matched > 0.99));
        assert!(
            editions
                .iter()
                .find(|e| e.variant.to_string() == "ur:hi")
                .unwrap()
                .captions
                .iter()
                .all(|b| b.highlight.is_some())
        );
        let counts = (
            text.calls.load(Ordering::SeqCst),
            media.images.load(Ordering::SeqCst),
            media.speech.load(Ordering::SeqCst),
            media.transcripts.load(Ordering::SeqCst),
        );
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        assert_eq!(
            counts,
            (
                text.calls.load(Ordering::SeqCst),
                media.images.load(Ordering::SeqCst),
                media.speech.load(Ordering::SeqCst),
                media.transcripts.load(Ordering::SeqCst)
            )
        );
        assert!(
            walk(tmp.path())
                .iter()
                .all(|p| p.extension().is_none_or(|e| e != "mp4"))
        );
        if std::env::var_os("RETOLD_KEEP_FIXTURE").is_some() {
            println!("Review fixture retained at {}", tmp.keep().display());
        }
    }
    #[tokio::test]
    async fn character_review_requires_current_sheet_approval_before_continuing() {
        use retold_engine::character_approval::{self, Selection};
        let (_tmp, mut ctx, _text, _media) = setup(&["en"], Usd::ZERO);
        Arc::get_mut(&mut ctx).unwrap().skip_reviews = false;
        let result = workflow::run(Arc::clone(&ctx), Until::Ready, &options()).await;
        assert!(matches!(
            result,
            Err(WorkflowError::Media(MediaError::Review(_)))
        ));
        let path = ctx.layout.stage(Stage::Characters).join("review.json");
        let mut sheets: std::collections::BTreeMap<String, retold_engine::media_runtime::Asset> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert!(!sheets.is_empty());
        assert!(!character_approval::approved(&ctx.layout, &sheets).unwrap());
        character_approval::approve(&ctx.layout, Selection::All).unwrap();
        assert!(character_approval::approved(&ctx.layout, &sheets).unwrap());
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        sheets.values_mut().next().unwrap().content_hash = "regenerated-sheet".into();
        assert!(!character_approval::approved(&ctx.layout, &sheets).unwrap());
    }
    #[tokio::test]
    async fn registered_shot_reaches_timeline_and_resume_reuses_all_provider_work() {
        let (_tmp, ctx, text, media) = setup(&["en"], Usd::ZERO);
        media.small_images.store(true, Ordering::SeqCst);
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        let path = ctx.layout.meta().join("shots/1.json");
        let image = ctx.layout.stage(Stage::Images).join("unused.png");
        let shot = retold_render::Shot {
            image,
            depth: None,
            frames: 1,
            fps: 30,
            motion_x: 2.0,
            motion_y: 0.0,
            sprites: vec![],
            attached: vec![],
        };
        retold_engine::fsx::write_json_atomic(&path, &shot).unwrap();
        let counts = (
            text.calls.load(Ordering::SeqCst),
            media.images.load(Ordering::SeqCst),
            media.speech.load(Ordering::SeqCst),
        );
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        let timeline: retold_filmcraft::timeline::Timeline = serde_json::from_slice(
            &std::fs::read(ctx.layout.stage(Stage::Edit).join("en.timeline.json")).unwrap(),
        )
        .unwrap();
        let clip = timeline
            .clips
            .iter()
            .find_map(|c| match &c.source {
                retold_filmcraft::timeline::Source::Media { path }
                    if path.extension().is_some_and(|e| e == "mov") =>
                {
                    Some(path.clone())
                }
                _ => None,
            })
            .unwrap();
        let modified = std::fs::metadata(&clip).unwrap().modified().unwrap();
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        assert_eq!(
            modified,
            std::fs::metadata(&clip).unwrap().modified().unwrap()
        );
        ctx.media
            .store
            .lock()
            .await
            .bump_revision("shots-0")
            .unwrap();
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        assert_eq!(
            counts,
            (
                text.calls.load(Ordering::SeqCst),
                media.images.load(Ordering::SeqCst),
                media.speech.load(Ordering::SeqCst)
            )
        );
    }
    #[tokio::test]
    async fn review_does_not_authorize_video_and_export_permission_fails_before_launch() {
        let (_tmp, ctx, _, media) = setup(&["en"], Usd::ZERO);
        let mut context = (*ctx).clone();
        context.skip_reviews = false;
        let context = Arc::new(context);
        let result = workflow::run(Arc::clone(&context), Until::Ready, &options()).await;
        assert!(matches!(
            result,
            Err(WorkflowError::Media(MediaError::Review(_)))
        ));
        assert_eq!(media.speech.load(Ordering::SeqCst), 0);
        let mut context = (*context).clone();
        context.skip_reviews = true;
        let context = Arc::new(context);
        let mut opts = options();
        opts.filmcraft_binary = Some("/definitely/not/a/real/editor".into());
        let result = workflow::run(context, Until::Export, &opts).await;
        assert!(matches!(
            result,
            Err(WorkflowError::Media(MediaError::Review(_)))
        ));
    }
    #[tokio::test]
    async fn paid_budget_and_ambiguous_requests_never_repeat() {
        let (_tmp, ctx, _, media) = setup(&["en"], Usd::from_micros(1));
        let result = workflow::run(Arc::clone(&ctx), Until::Characters, &options()).await;
        assert!(matches!(
            result,
            Err(WorkflowError::Media(MediaError::Ledger(
                retold_engine::ledger::LedgerError::Budget { .. }
            )))
        ));
        assert_eq!(media.images.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn selective_redo_invalidates_only_the_selected_frame() {
        let (_tmp, ctx, _, media) = setup(&["en"], Usd::ZERO);
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        let images = media.images.load(Ordering::SeqCst);
        let voice = media.speech.load(Ordering::SeqCst);
        ctx.media
            .store
            .lock()
            .await
            .bump_revision("image-3")
            .unwrap();
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        assert_eq!(media.images.load(Ordering::SeqCst), images + 1);
        assert_eq!(media.speech.load(Ordering::SeqCst), voice);
    }
    #[tokio::test]
    async fn paid_cached_voice_is_not_dispatched_twice() {
        let (_tmp, ctx, _, media) = setup(&["en"], Usd::from_micros(100_000));
        let request = retold_providers::media::SpeechRequest {
            text: "A complete sentence.".into(),
            voice: "fixture".into(),
            instructions: None,
            language: "en".parse().unwrap(),
        };
        ctx.media
            .ledger
            .reserve("reserved", Usd::from_micros(10_000))
            .unwrap();
        // A real ambiguous hold uses the same deterministic request identity on restart.
        let first = ctx
            .media
            .speech(
                "voice-test",
                "openrouter",
                "fixture",
                &request,
                &ctx.layout.stage(Stage::Voice).join("voice.mp3"),
            )
            .await
            .unwrap();
        let before = media.speech.load(Ordering::SeqCst);
        let cached = ctx
            .media
            .speech("voice-test", "openrouter", "fixture", &request, &first.path)
            .await
            .unwrap();
        assert_eq!(first.content_hash, cached.content_hash);
        assert_eq!(media.speech.load(Ordering::SeqCst), before);
        assert_eq!(ctx.media.ledger.total().unwrap(), Usd::from_micros(15_000));
    }
    #[tokio::test]
    async fn invalid_paid_transcription_is_saved_and_never_dispatched_again() {
        let (_tmp, ctx, _, media) = setup(&["en"], Usd::from_micros(100_000));
        media.invalid_transcript.store(true, Ordering::SeqCst);
        let request = retold_providers::media::SpeechRequest {
            text: "A complete sentence.".into(),
            voice: "fixture".into(),
            instructions: None,
            language: "en".parse().unwrap(),
        };
        let audio = ctx
            .media
            .speech(
                "voice-test",
                "openrouter",
                "fixture",
                &request,
                &ctx.layout.stage(Stage::Voice).join("voice.mp3"),
            )
            .await
            .unwrap();
        for _ in 0..2 {
            let result = ctx
                .media
                .transcribe(
                    "transcript-test",
                    "openrouter",
                    "fixture",
                    &audio,
                    &request.language,
                    &ctx.layout.stage(Stage::Voice).join("transcript.json"),
                )
                .await;
            assert!(matches!(
                result,
                Err(MediaError::Provider(
                    retold_providers::ProviderError::Malformed(_)
                ))
            ));
        }
        assert_eq!(media.transcripts.load(Ordering::SeqCst), 1);
        assert_eq!(ctx.media.ledger.total().unwrap(), Usd::from_micros(10_000));
        let saved: Vec<_> = std::fs::read_dir(ctx.layout.stage(Stage::Voice))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .collect();
        assert_eq!(
            saved.len(),
            1,
            "invalid response must remain available for recovery"
        );
    }
    #[tokio::test]
    async fn ambiguous_paid_failure_keeps_hold_and_refuses_retry() {
        let (_tmp, ctx, _, media) = setup(&["en"], Usd::from_micros(100_000));
        media.fail_speech.store(true, Ordering::SeqCst);
        let request = retold_providers::media::SpeechRequest {
            text: "A complete sentence.".into(),
            voice: "fixture".into(),
            instructions: None,
            language: "en".parse().unwrap(),
        };
        for _ in 0..2 {
            let result = ctx
                .media
                .speech(
                    "voice-test",
                    "openrouter",
                    "fixture",
                    &request,
                    &ctx.layout.stage(Stage::Voice).join("voice.mp3"),
                )
                .await;
            assert!(matches!(
                result,
                Err(MediaError::Ledger(
                    retold_engine::ledger::LedgerError::Uncertain(_)
                ))
            ));
        }
        assert_eq!(media.speech.load(Ordering::SeqCst), 1);
        assert_eq!(ctx.media.ledger.total().unwrap(), Usd::from_micros(10_000));
    }
    #[tokio::test]
    async fn adding_language_reuses_all_art() {
        let (_tmp, ctx, _, media) = setup(&["en"], Usd::ZERO);
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        let images = media.images.load(Ordering::SeqCst);
        let mut next = (*ctx).clone();
        next.meta.variants.push("hi:hi,en.srt".parse().unwrap());
        workflow::run(Arc::new(next), Until::Ready, &options())
            .await
            .unwrap();
        assert_eq!(images, media.images.load(Ordering::SeqCst));
        assert_eq!(media.speech.load(Ordering::SeqCst), 2);
    }
    fn walk(path: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                out.extend(walk(&entry.path()));
            } else {
                out.push(entry.path());
            }
        }
        out
    }
}
