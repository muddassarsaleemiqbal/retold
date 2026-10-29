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
    use retold_engine::assets::{Frame, ImageQa, Images};
    use retold_engine::languages::{Edition, LocalUnit, Packaging};
    use retold_engine::{
        context::Context,
        media_runtime::{Asset, MediaError},
        mouths,
        phonemes::{AcousticAligner, Alignment},
    };
    use retold_lipsync::viseme::{Phoneme, SpokenWord};
    use retold_providers::{
        Billing, ProviderError, Router, TextProvider, TextRequest, TextResponse, Usage,
    };
    use serde_json::json;
    use std::{
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };
    use tokio_util::sync::CancellationToken;

    #[derive(Debug)]
    struct RecordedVision {
        sheet_rejections: usize,
        reject_composite: AtomicBool,
        sheets: AtomicUsize,
        composites: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl TextProvider for RecordedVision {
        fn id(&self) -> &'static str {
            "fixture"
        }
        fn billing(&self) -> Billing {
            Billing::Subscription
        }
        fn supports_images(&self) -> bool {
            true
        }
        fn resolve_model(&self, selector: &str) -> String {
            selector.into()
        }
        async fn generate(
            &self,
            req: &TextRequest,
            _: &str,
            _: &CancellationToken,
        ) -> Result<TextResponse, ProviderError> {
            let p = &req.schema["properties"];
            let value = if p.get("anchors").is_some() {
                let labels: Vec<String> =
                    serde_json::from_str(req.prompt.split("Labels: ").nth(1).unwrap()).unwrap();
                json!({"anchors":labels.iter().map(|label|json!({"label":label,"first":{"x":0.3,"y":0.5},"second":{"x":0.7,"y":0.5}})).collect::<Vec<_>>()})
            } else if p.get("faces").is_some() {
                json!({"faces":[{"name":"Alice","left":0.15,"top":0.1,"right":0.8,"bottom":0.85}]})
            } else if p.get("mouth_visible").is_some() {
                json!({"mouth_visible":true,"eyes_visible":true,"mouth_left":{"x":0.35,"y":0.55},"mouth_right":{"x":0.65,"y":0.55},"left_eye":{"x":0.3,"y":0.3},"right_eye":{"x":0.7,"y":0.3}})
            } else if p.get("ok").is_some() {
                let ok = if req.prompt.starts_with("The first image is a mouth") {
                    self.sheets.fetch_add(1, Ordering::SeqCst) >= self.sheet_rejections
                } else {
                    self.composites.fetch_add(1, Ordering::SeqCst);
                    !self.reject_composite.load(Ordering::SeqCst)
                };
                let mut value = json!({"ok":ok,"problems":if ok {vec![]} else {vec!["recorded identity/placement rejection"]}});
                if p.get("bad_cells").is_some() {
                    value["bad_cells"] = if ok { json!([]) } else { json!([7]) };
                }
                value
            } else {
                return Err(ProviderError::Malformed(
                    "unexpected mouth fixture schema".into(),
                ));
            };
            Ok(TextResponse {
                value,
                usage: Usage::default(),
            })
        }
    }
    #[derive(Debug, Default)]
    struct RecordedTiming {
        calls: AtomicUsize,
        reject: bool,
    }
    #[async_trait::async_trait]
    impl AcousticAligner for RecordedTiming {
        fn identity(&self) -> Result<String, MediaError> {
            Ok("recorded-ctc-v1".into())
        }
        async fn phonemize(
            &self,
            _: &str,
            words: &[retold_lang::TimedWord],
        ) -> Result<Vec<SpokenWord>, MediaError> {
            Ok(words
                .iter()
                .map(|w| SpokenWord {
                    start: w.start,
                    end: w.end,
                    ipa: "m|a".into(),
                })
                .collect())
        }
        async fn align(
            &self,
            _: &Path,
            words: &[SpokenWord],
            _: &CancellationToken,
        ) -> Result<Alignment, MediaError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.reject {
                return Err(MediaError::Quality(
                    "recorded low acoustic confidence".into(),
                ));
            }
            Ok(Alignment {
                phonemes: words
                    .iter()
                    .flat_map(|w| {
                        let mid = f64::midpoint(w.start, w.end);
                        [
                            Phoneme {
                                ipa: "m".into(),
                                start: w.start,
                                end: mid,
                                confidence: 0.99,
                            },
                            Phoneme {
                                ipa: "a".into(),
                                start: mid,
                                end: w.end,
                                confidence: 0.99,
                            },
                        ]
                    })
                    .collect(),
                stride_seconds: 0.02,
                model_revision: "recorded-ctc-v1".into(),
            })
        }
    }
    fn fixture_asset(root: &Path) -> Asset {
        let image = root.join("frame.png");
        image::RgbaImage::from_pixel(320, 240, image::Rgba([210, 170, 130, 255]))
            .save(&image)
            .unwrap();
        Asset {
            path: image.clone(),
            content_hash: blake3::hash(&std::fs::read(image).unwrap())
                .to_hex()
                .to_string(),
            provider: "fixture".into(),
            model: "fixture".into(),
            usage: retold_providers::media::MediaUsage {
                generation: None,
                cost: Some(Usd::ZERO),
            },
            quote: retold_providers::media::Quote {
                ceiling: Usd::ZERO,
                provider_tag: None,
            },
            request_id: "fixture".into(),
        }
    }

    fn fixture(
        rejections: usize,
    ) -> (
        tempfile::TempDir,
        Context,
        Images,
        Edition,
        Arc<RecordedVision>,
        Arc<RecordedTiming>,
        Arc<support::MediaFake>,
    ) {
        let (tmp, ctx, _, media) = support::setup(&["en"], Usd::ZERO);
        media.mouth_sheets.store(true, Ordering::SeqCst);
        let vision = Arc::new(RecordedVision {
            sheet_rejections: rejections,
            reject_composite: AtomicBool::new(false),
            sheets: AtomicUsize::new(0),
            composites: AtomicUsize::new(0),
        });
        let timing = Arc::new(RecordedTiming::default());
        let mut ctx = (*ctx).clone();
        ctx.text = Arc::new(Router::from_providers(
            vec![(Arc::clone(&vision) as Arc<dyn TextProvider>, 1)],
            ctx.media.registry.config.routing.clone(),
        ));
        ctx.phonemes = Arc::clone(&timing) as Arc<dyn AcousticAligner>;
        let mut files = ctx.guide.files().clone();
        let mut m: toml::Value = toml::from_str(&files["guide.toml"]).unwrap();
        m["narration"]["mode"] = "dialogue".into();
        m["visuals"].as_table_mut().unwrap().insert(
            "lipsync".into(),
            toml::Value::Table(std::iter::once(("enabled".into(), true.into())).collect()),
        );
        files.insert("guide.toml".into(), toml::to_string(&m).unwrap());
        ctx.guide = retold_guide::Guide::from_files(retold_guide::Origin::Inline, files).unwrap();
        let asset = fixture_asset(tmp.path());
        let images = Images {
            cover: asset.clone(),
            style: asset.clone(),
            frames: vec![Frame {
                unit: 0,
                image: asset.clone(),
                qa: ImageQa {
                    style_score: 10.0,
                    character_score: 10.0,
                    text_present: false,
                    anatomy_errors: vec![],
                    problems: vec![],
                    notes: vec![],
                    focus: [0.5, 0.5],
                },
                regenerated: false,
            }],
            sheets: std::iter::once(("Alice".into(), asset.clone())).collect(),
        };
        let edition = Edition {
            variant: "en".parse().unwrap(),
            units: vec![LocalUnit {
                id: 0,
                section: "hook".into(),
                text: "Alice: mama\nAlice: mama".into(),
            }],
            audio: asset,
            alignment: retold_lang::Alignment {
                duration: 2.0,
                matched: 1.0,
                units: vec![retold_lang::align::TimedUnit {
                    id: "0".into(),
                    start: 0.0,
                    end: 2.0,
                    words: vec![
                        retold_lang::TimedWord {
                            text: "mama".into(),
                            start: 0.2,
                            end: 0.8,
                        },
                        retold_lang::TimedWord {
                            text: "mama".into(),
                            start: 1.2,
                            end: 1.8,
                        },
                    ],
                }],
            },
            captions: vec![],
            sidecars: vec![],
            packaging: Packaging { posts: vec![] },
        };
        (tmp, ctx, images, edition, vision, timing, media)
    }
    #[tokio::test]
    async fn repaired_sheet_animates_both_turns_and_resume_reuses_all_timing_and_vision() {
        let (_tmp, ctx, images, edition, vision, timing, ..) = fixture(1);
        let sprites = mouths::attachments(&ctx, &images, &edition, 0, 60, 0.0)
            .await
            .unwrap();
        assert!(!sprites.is_empty());
        assert_eq!(vision.sheets.load(Ordering::SeqCst), 2);
        assert_eq!(timing.calls.load(Ordering::SeqCst), 2);
        let fps = ctx.guide.manifest().format.fps.get();
        for (a, b) in [(0.2, 0.8), (1.2, 1.8)] {
            assert!(
                sprites
                    .iter()
                    .any(|s| (f64::from(s.first_frame) / f64::from(fps) - a).abs()
                        <= 1.0 / f64::from(fps))
            );
            assert!(sprites.iter().any(
                |s| (f64::from(s.end_frame) / f64::from(fps) - b).abs() <= 1.0 / f64::from(fps)
            ));
        }
        let again = mouths::attachments(&ctx, &images, &edition, 0, 60, 0.0)
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(sprites).unwrap(),
            serde_json::to_value(again).unwrap()
        );
        assert_eq!(vision.sheets.load(Ordering::SeqCst), 2);
        assert_eq!(vision.composites.load(Ordering::SeqCst), 1);
        assert_eq!(timing.calls.load(Ordering::SeqCst), 2);
        assert_eq!(ctx.media.ledger.total().unwrap(), Usd::ZERO);
    }
    #[tokio::test]
    async fn enabled_shot_pipeline_binds_animation_and_preserves_camera_ownership() {
        let (_tmp, ctx, images, edition, vision, timing, ..) = fixture(0);
        let board = retold_engine::assets::Storyboard {
            plan: serde_json::from_str(include_str!("fixtures/full/plan.json")).unwrap(),
            units: vec![],
            direction: serde_json::from_value(json!({"shots":[{
                "unit":0,"shot":"close","location":"room","action":"speaking",
                "mood":"calm","characters":["Alice"],"focus":[0.5,0.5]
            }]}))
            .unwrap(),
        };
        for registered in [false, true] {
            if registered {
                let shot = retold_render::Shot {
                    image: images.frames[0].image.path.clone(),
                    depth: None,
                    frames: 1,
                    fps: 1,
                    motion_x: 4.0,
                    motion_y: 2.0,
                    sprites: vec![],
                    attached: vec![],
                };
                retold_engine::fsx::write_json_atomic(
                    &ctx.layout.meta().join("shots/1.json"),
                    &shot,
                )
                .unwrap();
            }
            retold_engine::local_stages::prepare_shots(&ctx, &images, &board, &edition)
                .await
                .unwrap();
            let bindings: std::collections::BTreeMap<
                usize,
                retold_engine::local_stages::ShotBinding,
            > = serde_json::from_slice(
                &std::fs::read(retold_engine::local_stages::bindings_file(
                    &ctx.layout,
                    &edition,
                ))
                .unwrap(),
            )
            .unwrap();
            assert_eq!(bindings.len(), 1);
            assert!(bindings[&0].path.is_file());
            assert_eq!(bindings[&0].dimensions, [320, 240]);
            assert_eq!(bindings[&0].camera, !registered);
        }
        assert_eq!(vision.composites.load(Ordering::SeqCst), 1);
        assert_eq!(timing.calls.load(Ordering::SeqCst), 2);
        assert_eq!(ctx.media.ledger.total().unwrap(), Usd::ZERO);
    }
    #[tokio::test]
    async fn updating_sheet_qa_revalidates_existing_art_without_new_image_calls() {
        let (_tmp, ctx, images, edition, vision, _, media) = fixture(0);
        assert!(
            !mouths::attachments(&ctx, &images, &edition, 0, 60, 0.0)
                .await
                .unwrap()
                .is_empty()
        );
        let calls = media.images.load(Ordering::SeqCst);
        let record = std::fs::read_dir(ctx.layout.cast_dir().join("mouths"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "json"))
            .unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
        value["qa_policy"] = 0.into();
        std::fs::write(&record, serde_json::to_vec(&value).unwrap()).unwrap();
        ctx.media
            .store
            .lock()
            .await
            .bump_revision("mouth-sheet-check-alice-0")
            .unwrap();
        assert!(
            !mouths::attachments(&ctx, &images, &edition, 0, 60, 0.0)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(media.images.load(Ordering::SeqCst), calls);
        assert_eq!(vision.sheets.load(Ordering::SeqCst), 2);
        let saved: mouths::MouthSheet =
            serde_json::from_slice(&std::fs::read(record).unwrap()).unwrap();
        assert!(saved.qa_policy > 0);
    }

    #[tokio::test]
    async fn a_zero_duration_interpolated_token_preserves_the_next_turn_mapping() {
        let (_tmp, ctx, images, mut edition, _, timing, ..) = fixture(0);
        edition.units[0].text = "Alice: mama and\nAlice: mama".into();
        edition.alignment.units[0].words.insert(
            1,
            retold_lang::TimedWord {
                text: "and".into(),
                start: 1.0,
                end: 1.0,
            },
        );
        let sprites = mouths::attachments(&ctx, &images, &edition, 0, 60, 0.0)
            .await
            .unwrap();
        assert!(!sprites.is_empty());
        assert_eq!(timing.calls.load(Ordering::SeqCst), 2);
        assert!(sprites.iter().any(|s| s.first_frame >= 36));
    }

    #[tokio::test]
    async fn low_confidence_acoustic_timing_keeps_the_still() {
        let (_tmp, mut ctx, images, edition, vision, ..) = fixture(0);
        ctx.phonemes = Arc::new(RecordedTiming {
            calls: AtomicUsize::new(0),
            reject: true,
        });
        assert!(
            mouths::attachments(&ctx, &images, &edition, 0, 60, 0.0)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(vision.composites.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn two_rejected_sheets_keep_still_without_acoustic_work() {
        let (_tmp, ctx, images, edition, vision, timing, ..) = fixture(2);
        assert!(
            mouths::attachments(&ctx, &images, &edition, 0, 60, 0.0)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(vision.sheets.load(Ordering::SeqCst), 2);
        assert_eq!(vision.composites.load(Ordering::SeqCst), 0);
        assert_eq!(timing.calls.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn composite_rejection_keeps_still_after_successful_sheet_and_timing() {
        let (_tmp, ctx, images, edition, vision, timing, ..) = fixture(0);
        vision.reject_composite.store(true, Ordering::SeqCst);
        assert!(
            mouths::attachments(&ctx, &images, &edition, 0, 60, 0.0)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(vision.composites.load(Ordering::SeqCst), 1);
        assert_eq!(timing.calls.load(Ordering::SeqCst), 2);
    }
}
