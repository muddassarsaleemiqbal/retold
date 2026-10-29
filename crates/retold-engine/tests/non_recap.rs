//! Non-recap workflow contract test.
//! Exercises arbitrary section names, zero characters, no open loop or emotional peak,
//! no ending question gate, custom packaging limits, and notice templates.
#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use retold_core::{LanguageTag, Tier, Usd};
    use retold_engine::assemble::FontResolver;
    use retold_engine::context::Context;
    use retold_engine::ledger::Ledger;
    use retold_engine::media_runtime::{MediaError, MediaRegistry, MediaRuntime};
    use retold_engine::pipeline::Until;
    use retold_engine::store::Store;
    use retold_engine::workflow::{self, Options};
    use retold_engine::{NewRun, Quality, Stage, fsx, run};
    use retold_guide::Guide;
    use retold_guide::channel::init_from_template;
    use retold_providers::media::{
        ImageRequest, Media, MediaProvider, MediaUsage, Quote, SpeechRequest, Transcript, Word,
    };
    use retold_providers::{
        Billing, ProviderError, ProvidersConfig, Router, TextProvider, TextRequest, TextResponse,
        Usage,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::Mutex as AsyncMutex;
    use tokio_util::sync::CancellationToken;

    #[derive(Debug)]
    struct EstimatedFonts;
    impl FontResolver for EstimatedFonts {
        fn face(
            &self,
            _: &retold_guide::channel::FontSpec,
        ) -> Result<Box<dyn retold_lang::fit::Measure>, MediaError> {
            Ok(Box::new(retold_lang::fit::Estimate))
        }
    }

    #[derive(Debug, Default)]
    struct NonRecapTextFake {
        pub calls: AtomicUsize,
    }

    #[async_trait]
    impl TextProvider for NonRecapTextFake {
        fn id(&self) -> &'static str {
            "fixture"
        }
        fn billing(&self) -> Billing {
            Billing::Subscription
        }
        fn supports_images(&self) -> bool {
            true
        }
        fn resolve_model(&self, s: &str) -> String {
            s.into()
        }

        #[expect(
            clippy::too_many_lines,
            reason = "test fixture mock matches schema properties"
        )]
        async fn generate(
            &self,
            req: &TextRequest,
            _: &str,
            _: &CancellationToken,
        ) -> Result<TextResponse, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let p = &req.schema["properties"];
            let wrap = |value| {
                Ok(TextResponse {
                    value,
                    usage: Usage::default(),
                })
            };
            if p.get("beats").is_some() {
                return wrap(json!({
                    "storyline": "Technical breakdown of memory safety through ownership.",
                    "characters": [],
                    "tone": "instructive, calm, precise",
                    "open_loop": "",
                    "emotional_peak": "",
                    "omitted": [
                        "History of programming languages."
                    ],
                    "beats": [
                        {
                            "section": "intro",
                            "purpose": "Introduce the mechanism",
                            "facts": [
                                "Rust eliminates data races at compile time."
                            ]
                        },
                        {
                            "section": "details",
                            "purpose": "Explain memory guarantees",
                            "facts": [
                                "Borrowing allows shared references without mutation."
                            ]
                        }
                    ]
                }));
            }
            if p.get("candidates").is_some() {
                return wrap(json!({
                    "candidates": [
                        "How does Rust enforce safety without a garbage collector?"
                    ]
                }));
            }
            if p.get("winner").is_some() {
                return wrap(json!({
                    "winner": 0,
                    "scores": [9.0],
                    "reason": "Clear and direct"
                }));
            }
            if p.get("sections").is_some() {
                return wrap(json!({
                    "sections": [
                        {
                            "section": "intro",
                            "text": "Rust guarantees memory safety at compile time without runtime overhead."
                        },
                        {
                            "section": "details",
                            "text": "Borrowing rules ensure shared references cannot modify memory concurrently."
                        }
                    ]
                }));
            }
            if p.get("issues").is_some() {
                return wrap(json!({ "issues": [] }));
            }
            if p.get("shots").is_some() {
                let tail = req.prompt.split_once("Units: ").unwrap().1;
                let units: Value = serde_json::Deserializer::from_str(tail)
                    .into_iter()
                    .next()
                    .unwrap()
                    .unwrap();
                return wrap(json!({
                    "shots": units.as_array().unwrap().iter().map(|u| json!({
                        "unit": u["id"],
                        "shot": "wide",
                        "location": "Abstract Diagram Space",
                        "action": "Memory cells light up in sequence with green bounds",
                        "mood": "precise",
                        "characters": [],
                        "focus": [0.5, 0.5],
                        "hero": u["id"] == 0
                    })).collect::<Vec<_>>()
                }));
            }
            if p.get("characters").is_some() {
                return wrap(json!({ "characters": [] }));
            }
            if p.get("style_score").is_some() {
                return wrap(json!({
                    "style_score": 9.0,
                    "character_score": 9.0,
                    "text_present": false,
                    "anatomy_errors": [],
                    "problems": [],
                    "focus": [0.5, 0.5]
                }));
            }
            if p.get("posts").is_some() {
                return wrap(json!({
                    "posts": [
                        {
                            "platform": "youtube",
                            "title": "Rust Safety Explained",
                            "description": "How the compiler prevents data races.",
                            "hashtags": ["#rust", "#systems"]
                        }
                    ]
                }));
            }
            Err(ProviderError::Malformed(format!(
                "unexpected schema in test: {}",
                req.schema
            )))
        }
    }

    #[derive(Debug, Default)]
    struct NonRecapMediaFake {
        pub texts: Mutex<BTreeMap<String, String>>,
    }

    #[async_trait]
    impl MediaProvider for NonRecapMediaFake {
        fn id(&self) -> &'static str {
            "openrouter"
        }
        fn billing(&self) -> Billing {
            Billing::Subscription
        }
        async fn image_quote(
            &self,
            _: &str,
            _: &ImageRequest,
            _: &CancellationToken,
        ) -> Result<Quote, ProviderError> {
            Ok(Quote {
                ceiling: Usd::ZERO,
                provider_tag: None,
            })
        }
        async fn speech_quote(
            &self,
            _: &str,
            _: &SpeechRequest,
            _: &CancellationToken,
        ) -> Result<Quote, ProviderError> {
            Ok(Quote {
                ceiling: Usd::ZERO,
                provider_tag: None,
            })
        }
        async fn transcript_quote(
            &self,
            _: &str,
            _: f64,
            _: &CancellationToken,
        ) -> Result<Quote, ProviderError> {
            Ok(Quote {
                ceiling: Usd::ZERO,
                provider_tag: None,
            })
        }
        async fn image(
            &self,
            _: &str,
            _: &ImageRequest,
            _: &Quote,
            _: &CancellationToken,
        ) -> Result<Media<Vec<u8>>, ProviderError> {
            let img = image::RgbImage::from_pixel(100, 100, image::Rgb([20, 30, 40]));
            let mut data = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut data, image::ImageFormat::Png)
                .unwrap();
            Ok(Media {
                value: data.into_inner(),
                usage: MediaUsage {
                    generation: Some("fixture".into()),
                    cost: Some(Usd::ZERO),
                },
            })
        }
        async fn speech(
            &self,
            _: &str,
            req: &SpeechRequest,
            _: &CancellationToken,
        ) -> Result<Media<Vec<u8>>, ProviderError> {
            self.texts
                .lock()
                .unwrap()
                .insert(req.language.to_string(), req.text.clone());
            Ok(Media {
                value: synthetic_wav(4),
                usage: MediaUsage {
                    generation: Some("fixture".into()),
                    cost: Some(Usd::ZERO),
                },
            })
        }
        async fn transcribe(
            &self,
            _: &str,
            _: &[u8],
            _: &str,
            lang: &LanguageTag,
            _: &CancellationToken,
        ) -> Result<Media<Transcript>, ProviderError> {
            let text = self
                .texts
                .lock()
                .unwrap()
                .get(&lang.to_string())
                .cloned()
                .unwrap_or_default();
            let tokens: Vec<_> = text.split_whitespace().collect();
            let count = u32::try_from(tokens.len().max(1)).unwrap();
            let step = 4.0 / f64::from(count);
            let words: Vec<Word> = tokens
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    let idx = u32::try_from(i).unwrap();
                    Word {
                        word: (*w).into(),
                        start: f64::from(idx) * step,
                        end: f64::from(idx + 1) * step - 0.01,
                    }
                })
                .collect();
            Ok(Media {
                value: Transcript {
                    text,
                    duration: 4.0,
                    words,
                },
                usage: MediaUsage {
                    generation: Some("fixture".into()),
                    cost: Some(Usd::ZERO),
                },
            })
        }
    }

    fn synthetic_wav(seconds: u32) -> Vec<u8> {
        let bytes = 16_000 * seconds * 2;
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((36 + bytes).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(16_000u32.to_le_bytes());
        out.extend(32_000u32.to_le_bytes());
        out.extend(2u16.to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend(bytes.to_le_bytes());
        out.resize(44 + bytes as usize, 0);
        out
    }

    #[expect(clippy::too_many_lines, reason = "a single TOML fixture literal")]
    fn guide_manifest_toml() -> &'static str {
        r#"
schema = 1
id = "tech-explainer"
version = "1.0.0"
name = "Tech Explainer"
summary = "Explains technical concepts clearly without narrative recap tropes."

[inputs.topic]
type = "text"
required = true
prompt = "Concept name"
help = "Name of the concept"
example = "Rust Ownership"

[run]
folder = ["{{ inputs.topic }}"]
cast_depth = 1

[format]
aspect = "16:9"
resolution = [1920, 1080]
fps = 30
target_seconds = [4.0, 10.0]
source_language = "en"

[length.en]
seconds = [4.0, 10.0]

[research]
policy = "source_only"
max_chars = 10000
[[research.sources]]
kind = "file"
optional = false

[structure]
open_loops = 0
emotional_peak = false
max_named_characters = 0

[[structure.sections]]
id = "intro"
share = 0.5
purpose = "Introduce the mechanism"

[[structure.sections]]
id = "details"
share = 0.5
purpose = "Explain memory guarantees"

[writing]
voice = "voice.md"
examples = []
tense = "present"
person = "third"
hook_candidates = 1
drafts = { subscription = 1, paid = 1 }
gates = []

[quality]
max_fix_passes = 1
fact_check = "off"
image_qa = false

[visuals]
seconds_per_unit = [4.0, 8.0]
lean_seconds_per_unit = [6.0, 12.0]
style = "Clean diagrams in {{ channel.colors.accent }}. No text, no photorealism."
safe_zones = { top = 0.12, bottom = 0.25 }
shots = ["wide", "medium"]
max_same_shot_run = 2

[visuals.characters]
reference_sheet = false
originality = "strict"
review = false

[narration]
mode = "continuous"
[narration.direction]
en = "[neutral, explanatory]"

[edit]
motions = ["zoom_in", "pan_right"]
opening = { kind = "cover_title", seconds = 1.0 }
ending = { kind = "hold_fade", seconds = 0.5 }
music = { mood_from = "plan.tone", bed_lufs = -24.0, duck_under_voice_db = -6.0 }
sfx_on_transitions = false

[packaging]
platforms = ["youtube"]
rules = "Short technical title and description."
title_max_chars = 40
hashtag_count = 2

[packaging.notices.en]
prefix = "Briefing: {{ inputs.topic }}."
suffix = "Ref: {{ channel.name }}."

[roles]
plan = "text.smart"
write = "text.smart"
judge = "text.smart"
check = "text.smart"
transcreate = "text.smart"
direct = "text.smart"
design = "text.smart"
image = "image"
image_qa = "vision.fast"
voice = "tts"
transcribe = "transcribe"
package = "text.fast"
"#
    }

    fn setup_test_context(tmp: &tempfile::TempDir) -> Arc<Context> {
        let mut files = BTreeMap::new();
        files.insert("guide.toml".into(), guide_manifest_toml().into());
        files.insert("voice.md".into(), "Clear technical prose.".into());

        let guide = Guide::from_files(retold_guide::guide::Origin::Inline, files).unwrap();
        let channel = init_from_template(
            "test-studio",
            &tmp.path().join("config/channels/test-studio"),
        )
        .unwrap();

        let raw: indexmap::IndexMap<_, _> = [("topic".into(), "Rust Ownership".into())].into();
        let inputs = retold_guide::inputs::resolve(guide.manifest(), &raw).unwrap();

        let source_path = tmp.path().join("source.txt");
        std::fs::write(
            &source_path,
            b"Rust ensures memory safety at compile time using ownership and borrowing rules.",
        )
        .unwrap();

        let (layout, meta) = run::create(
            &tmp.path().join("library"),
            NewRun {
                guide: &guide,
                channel: &channel,
                inputs,
                variants: vec!["en".parse().unwrap()],
                quality: Quality::Standard,
                budget: Usd::ZERO,
                skip_reviews: true,
                source_file: Some(&source_path),
            },
        )
        .unwrap();

        fsx::write_atomic(
            &layout.stage(Stage::Research).join("source-file.txt"),
            b"Rust ensures memory safety at compile time using ownership and borrowing rules.",
        )
        .unwrap();

        let text = Arc::new(NonRecapTextFake::default());
        let media = Arc::new(NonRecapMediaFake::default());

        let mut config = ProvidersConfig::suggested(retold_providers::config::Found {
            claude: false,
            codex: false,
        });
        for tier in Tier::ALL {
            config.routing.insert(
                tier,
                vec![if matches!(
                    tier,
                    Tier::Image | Tier::ImageNoRefs | Tier::Tts | Tier::Transcribe
                ) {
                    "openrouter:model".parse().unwrap()
                } else {
                    "fixture:model".parse().unwrap()
                }],
            );
        }

        let registry = Arc::new(MediaRegistry::from_providers(
            config,
            vec![(Arc::clone(&media) as Arc<dyn MediaProvider>, 2)],
        ));
        let router = Arc::new(
            Router::from_providers(
                vec![(Arc::clone(&text) as Arc<dyn TextProvider>, 2)],
                registry.config.routing.clone(),
            )
            .with_retry_backoff(std::time::Duration::ZERO),
        );
        let store = Arc::new(AsyncMutex::new(Store::open(&layout).unwrap()));
        let ledger = Arc::new(Ledger::open(&layout.meta().join("ledger.json"), Usd::ZERO).unwrap());

        Arc::new(Context {
            depth: Arc::new(retold_parallax::depth::ModelDepth::default()),
            phonemes: Arc::new(retold_engine::phonemes::LocalAligner),
            layout,
            meta,
            guide,
            channel,
            text: router,
            media: MediaRuntime {
                registry,
                store,
                ledger,
                cancel: CancellationToken::new(),
                allow_paid: false,
            },
            skip_reviews: true,
        })
    }

    #[tokio::test]
    async fn non_recap_pipeline_runs_to_ready() {
        let tmp = tempfile::tempdir().unwrap();
        let context = setup_test_context(&tmp);

        let opts = Options {
            fonts: Some(Arc::new(EstimatedFonts)),
            ..Options::default()
        };

        // Run full pipeline to Ready
        workflow::run(Arc::clone(&context), Until::Ready, &opts)
            .await
            .unwrap();

        // 1. Deliverables manifest exists
        let manifest: workflow::Manifest = serde_json::from_slice(
            &std::fs::read(context.layout.deliverables().join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.editions.len(), 1);

        // 2. Character sheets are completely empty (0 characters)
        let cast_dir = context.layout.cast_dir();
        let sheets_path = cast_dir.join("characters.json");
        if sheets_path.exists() {
            let sheets: BTreeMap<String, Value> =
                serde_json::from_slice(&std::fs::read(&sheets_path).unwrap()).unwrap();
            assert_eq!(
                sheets.len(),
                0,
                "Non-recap guide with max_named_characters=0 should produce 0 character sheets"
            );
        }

        // 3. Post packaging checks
        let post_file = context
            .layout
            .post_dir(&"en".parse().unwrap())
            .join("youtube.json");
        assert!(post_file.exists(), "youtube post file should exist");
        let post: retold_engine::languages::Post =
            serde_json::from_slice(&std::fs::read(&post_file).unwrap()).unwrap();
        assert_eq!(post.platform, "youtube");
        assert!(
            post.title.chars().count() <= 40,
            "Title must respect title_max_chars=40"
        );
        assert_eq!(post.hashtags.len(), 2, "Must have exactly 2 hashtags");
        assert!(
            post.description.starts_with("Briefing: Rust Ownership."),
            "Description must include prefix notice"
        );
        assert!(
            post.description.ends_with("Ref: Test Studio."),
            "Description must include suffix notice"
        );
    }
}
