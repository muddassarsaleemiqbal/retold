//! Reproducible full-workflow soak with fixtures only. Real media acceptance is separate.
//! Acceptance suite: `cargo test -p retold-engine --features acceptance --test s8_soak`.
#![cfg(feature = "acceptance")]
#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared support module"
)]
mod support {
    include!("support/mod.rs");
}
use async_trait::async_trait;
use retold_core::{Tier, Usd};
use retold_engine::events::{Events, Status};
use retold_engine::media_runtime::{MediaError, MediaRegistry, MediaRuntime};
use retold_engine::phonemes::{AcousticAligner, Alignment};
use retold_engine::run::{self, NewRun};
use retold_engine::store::{State, Store};
use retold_engine::workflow::{self, Options};
use retold_engine::{context::Context, fsx, ledger::Ledger, pipeline::Until};
use retold_guide::{Guide, Origin};
use retold_lipsync::viseme::{Phoneme, SpokenWord};
use retold_providers::{
    Billing, ProviderError, ProvidersConfig, Router, TextProvider, TextRequest, TextResponse,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Debug, Default)]
struct Text {
    base: support::TextFake,
    calls: AtomicUsize,
}
#[async_trait]
impl TextProvider for Text {
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
    async fn generate(
        &self,
        req: &TextRequest,
        model: &str,
        cancel: &CancellationToken,
    ) -> std::result::Result<TextResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let p = &req.schema["properties"];
        let value = if p.get("faces").is_some() {
            Some(
                json!({"faces":(["Thomas Hutter","Count Orlok","Ellen"].map(|name|json!({"name":name,"left":0.2,"top":0.2,"right":0.8,"bottom":0.8})))}),
            )
        } else if p.get("mouth_visible").is_some() {
            Some(
                json!({"mouth_visible":true,"eyes_visible":true,"mouth_left":{"x":0.35,"y":0.6},"mouth_right":{"x":0.65,"y":0.6},"left_eye":{"x":0.3,"y":0.35},"right_eye":{"x":0.7,"y":0.35}}),
            )
        } else if p.get("anchors").is_some() {
            let tail = req.prompt.split_once("Labels: ").unwrap().1;
            let labels: Vec<String> = serde_json::from_str(tail).unwrap();
            Some(
                json!({"anchors":labels.into_iter().map(|label|json!({"label":label,"first":{"x":0.3,"y":0.5},"second":{"x":0.7,"y":0.5}})).collect::<Vec<_>>()}),
            )
        } else if p.get("ok").is_some() {
            Some(if p.get("bad_cells").is_some() {
                json!({"ok":true,"problems":[],"bad_cells":[]})
            } else {
                json!({"ok":true,"problems":[]})
            })
        } else {
            None
        };
        if let Some(value) = value {
            return Ok(TextResponse {
                value,
                usage: retold_providers::Usage::default(),
            });
        }
        let mut answer = self.base.generate(req, model, cancel).await?;
        for field in ["sections", "units"] {
            if let Some(items) = answer.value.get_mut(field).and_then(Value::as_array_mut) {
                for (i, item) in items.iter_mut().enumerate() {
                    let text = item["text"].as_str().unwrap();
                    item["text"] = format!(
                        "{}: {text}",
                        if i % 2 == 0 { "Thomas Hutter" } else { "Ellen" }
                    )
                    .into();
                }
            }
        }
        Ok(answer)
    }
}
#[derive(Debug)]
struct Depth;
#[async_trait]
impl retold_parallax::depth::DepthEstimator for Depth {
    fn identity(&self) -> std::result::Result<String, retold_parallax::Error> {
        Ok("s8-recorded-depth-v1".into())
    }
    async fn estimate(
        &self,
        _: &Path,
        _: &CancellationToken,
    ) -> std::result::Result<retold_parallax::depth::DepthMap, retold_parallax::Error> {
        retold_parallax::depth::DepthMap::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../retold-parallax/tests/fixtures/scene-0.rgd"),
        )
    }
}
#[derive(Debug)]
struct Phonemes;
#[async_trait]
impl AcousticAligner for Phonemes {
    fn identity(&self) -> std::result::Result<String, MediaError> {
        Ok("s8-fixture-phonemes-v1".into())
    }
    async fn phonemize(
        &self,
        _: &str,
        words: &[retold_lang::TimedWord],
    ) -> std::result::Result<Vec<SpokenWord>, MediaError> {
        Ok(words
            .iter()
            .map(|w| SpokenWord {
                start: w.start,
                end: w.end,
                ipa: "a".into(),
            })
            .collect())
    }
    async fn align(
        &self,
        _: &Path,
        words: &[SpokenWord],
        _: &CancellationToken,
    ) -> std::result::Result<Alignment, MediaError> {
        Ok(Alignment {
            phonemes: words
                .iter()
                .map(|w| Phoneme {
                    start: w.start,
                    end: w.end,
                    ipa: w.ipa.clone(),
                    confidence: 1.0,
                })
                .collect(),
            stride_seconds: 0.02,
            model_revision: "recorded-test-windows".into(),
        })
    }
}
#[expect(
    clippy::too_many_lines,
    reason = "one explicit fixture context with no production providers"
)]
fn context(
    root: &Path,
    cut: Option<String>,
) -> Result<(Arc<Context>, Arc<Text>, Arc<support::MediaFake>)> {
    std::fs::create_dir_all(root)?;
    let mut files = retold_guide::catalog::bundled()?.remove(0).files().clone();
    let mut m: toml::Value = toml::from_str(&files["guide.toml"])?;
    m["format"]["fps"] = 6.into();
    m["format"]["target_seconds"] = toml::Value::try_from([20, 60])?;
    m["narration"]["mode"] = "dialogue".into();
    m["writing"]["gates"] = toml::Value::Array(vec![]);
    m["writing"]["drafts"]["subscription"] = 1.into();
    for lang in ["en", "hi", "ur"] {
        m["length"][lang] = toml::from_str::<toml::Value>("words=[1,400]")?;
    }
    m["visuals"].as_table_mut().unwrap().insert(
        "parallax".into(),
        toml::from_str("enabled=true\nmax_share=1.0\nstrength=0.2\nhero_fill=false")?,
    );
    m["visuals"]
        .as_table_mut()
        .unwrap()
        .insert("lipsync".into(), toml::from_str("enabled=true")?);
    m["edit"]["opening"]["seconds"] = 0.48.into();
    m["edit"]["ending"]["seconds"] = 0.48.into();
    files.insert("guide.toml".into(), toml::to_string(&m)?);
    let guide = Guide::from_files(Origin::Inline, files)?;
    let channel = retold_guide::channel::init_from_template("test-studio", &root.join("channel"))?;
    let (layout, meta) = if let Some(r) = run::find_all(&root.join("library")).pop() {
        (r.layout, r.meta)
    } else {
        let raw = [
            ("title".into(), "S8 full workflow".into()),
            ("kind".into(), "film".into()),
        ]
        .into_iter()
        .collect();
        run::create(
            &root.join("library"),
            NewRun {
                guide: &guide,
                channel: &channel,
                inputs: retold_guide::inputs::resolve(guide.manifest(), &raw)
                    .map_err(|e| format!("{e:?}"))?,
                variants: ["en", "hi", "ur"]
                    .into_iter()
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()?,
                quality: retold_engine::Quality::default(),
                budget: Usd::ZERO,
                skip_reviews: true,
                source_file: None,
            },
        )?
    };
    fsx::write_atomic(
        &layout
            .stage(retold_engine::Stage::Research)
            .join("source-file.txt"),
        include_bytes!("fixtures/full/source.txt"),
    )?;
    let text = Arc::new(Text::default());
    let texts_path = root.join("fixture-texts.json");
    let texts = if texts_path.exists() {
        serde_json::from_slice(&std::fs::read(&texts_path)?)?
    } else {
        BTreeMap::new()
    };
    let media = Arc::new(support::MediaFake {
        texts_file: Some(texts_path),
        texts: std::sync::Mutex::new(texts),
        small_images: std::sync::atomic::AtomicBool::new(true),
        mouth_sheets: std::sync::atomic::AtomicBool::new(true),
        recorded_speech: std::sync::atomic::AtomicBool::new(true),
        ..support::MediaFake::default()
    });
    let mut config = ProvidersConfig::suggested(retold_providers::config::Found {
        claude: false,
        codex: false,
    });
    for tier in Tier::ALL {
        config.routing.insert(
            tier,
            vec![
                if matches!(
                    tier,
                    Tier::Image | Tier::ImageNoRefs | Tier::Tts | Tier::Transcribe
                ) {
                    "openrouter:model"
                } else {
                    "fixture:model"
                }
                .parse()?,
            ],
        );
    }
    let registry = Arc::new(MediaRegistry::from_providers(
        config,
        vec![(
            Arc::clone(&media) as Arc<dyn retold_providers::media::MediaProvider>,
            1,
        )],
    ));
    let router = Arc::new(
        Router::from_providers(
            vec![(Arc::clone(&text) as Arc<dyn TextProvider>, 1)],
            registry.config.routing.clone(),
        )
        .with_retry_backoff(Duration::ZERO),
    );
    let mut store = Store::open(&layout)?;
    let marker = root.join("kill-ready.json");
    store.set_events(Arc::new(Events::new(
        &layout.meta().join("events.log"),
        None,
        Some(Arc::new(move |event| {
            if cut.as_ref().is_some_and(|s| event.task.starts_with(s))
                && matches!(event.status, Status::Complete)
            {
                fsx::write_json_atomic(&marker, event).unwrap();
                std::thread::sleep(Duration::from_secs(120));
            }
        })),
    )));
    let ledger = Arc::new(Ledger::open(&layout.meta().join("ledger.json"), Usd::ZERO)?);
    Ok((
        Arc::new(Context {
            layout,
            meta,
            guide,
            channel,
            text: router,
            media: MediaRuntime {
                registry,
                store: Arc::new(tokio::sync::Mutex::new(store)),
                ledger,
                cancel: CancellationToken::new(),
                allow_paid: false,
            },
            skip_reviews: true,
            depth: Arc::new(Depth),
            phonemes: Arc::new(Phonemes),
        }),
        text,
        media,
    ))
}
fn counts(text: &Text, media: &support::MediaFake) -> [usize; 4] {
    [
        text.calls.load(Ordering::SeqCst),
        media.images.load(Ordering::SeqCst),
        media.speech.load(Ordering::SeqCst),
        media.transcripts.load(Ordering::SeqCst),
    ]
}
#[test]
fn worker() -> Result {
    let Ok(root) = std::env::var("RETOLD_S8_CASE") else {
        return Ok(());
    };
    tokio::runtime::Runtime::new()?.block_on(async {
 let root=Path::new(&root);let (ctx,text,media)=context(root,std::env::var("RETOLD_S8_CUT").ok())?;
 let options=Options {fonts:Some(Arc::new(support::EstimatedFonts)),..Options::default()};
 workflow::run(Arc::clone(&ctx),Until::Ready,&options).await?;
 let before=counts(&text,&media);let charges=serde_json::to_value(ctx.media.ledger.entries()?)?;
 let old=clips(root)?;workflow::run(Arc::clone(&ctx),Until::Ready,&options).await?;
 assert_eq!(before,counts(&text,&media));assert_eq!(charges,serde_json::to_value(ctx.media.ledger.entries()?)?);assert_eq!(old,clips(root)?);
 let state=&ctx.media.store.lock().await.state;assert!(state.active.is_none()&&state.active_tasks.is_empty()&&state.error.is_none());
 for lang in ["en","hi","ur"] {assert!(state.completed.keys().any(|s|s.starts_with(&format!("voices-0-{lang}"))));assert!(state.completed.keys().any(|s|s.starts_with(&format!("phonemes-{lang}-0"))));assert!(state.completed.contains_key(&format!("shots-0-{lang}")));}
 let sprites=ctx.layout.video_dir().join("work/mouths");assert!(std::fs::read_dir(&sprites)?.any(|e|e.is_ok_and(|e|e.file_name().to_string_lossy().starts_with("timing-"))));
 fsx::write_json_atomic(&root.join("result.json"),&json!({"schema":1,"languages":["en","hi","ur"],"scope":"full production workflow to Ready with fixture providers/depth/phonemes; real ffmpeg intermediates; no editor export","state":state,"warm_provider_repeats":0,"warm_rerenders":0,"ledger":ctx.media.ledger.entries()?,"paid_microusd":ctx.media.ledger.total()?.micros(),"clips":old.len()}))?;
 Ok(()) })
}
fn clips(root: &Path) -> Result<BTreeMap<PathBuf, (String, u128)>> {
    let mut result = BTreeMap::new();
    if !root.exists() {
        return Ok(result);
    }
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            result.extend(clips(&path)?);
        } else if path.extension().is_some_and(|e| e == "mov") {
            result.insert(
                path.clone(),
                (
                    blake3::hash(&std::fs::read(&path)?).to_hex().to_string(),
                    std::fs::metadata(path)?
                        .modified()?
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_nanos(),
                ),
            );
        }
    }
    Ok(result)
}
fn spawn(root: &Path, cut: Option<&str>) -> Result<std::process::Child> {
    let mut cmd = std::process::Command::new(std::env::current_exe()?);
    cmd.args(["--exact", "worker", "--nocapture"])
        .env("RETOLD_S8_CASE", root)
        .env_remove("RETOLD_S8_CUT");
    if let Some(cut) = cut {
        cmd.env("RETOLD_S8_CUT", cut);
    }
    let log = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(root.join("worker.log"))?;
    cmd.stdout(log.try_clone()?).stderr(log);
    Ok(cmd.spawn()?)
}
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one ordered ten-case process-kill, resume and evidence check"
)]
fn ten_full_multilingual_runs_survive_seeded_sigkills() -> Result {
    let tmp = tempfile::tempdir()?;
    let root =
        std::env::var_os("RETOLD_S8_OUTPUT").map_or_else(|| tmp.path().to_owned(), PathBuf::from);
    std::fs::create_dir_all(&root)?;
    let mut cuts = [
        "image-1",
        "voices-1-en",
        "voices-2-hi",
        "captions",
        "depth-0",
        "phonemes-en-0",
        "mouths-faces-0",
        "shots-0-en",
        "shots-1-hi",
        "package-ur",
    ];
    let mut seed = 0x5838_u64;
    for i in (1..cuts.len()).rev() {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let j = usize::try_from(seed % u64::try_from(i + 1)?)?;
        cuts.swap(i, j);
    }
    let mut records = vec![];
    for (i, cut) in cuts.into_iter().enumerate() {
        let case = root.join(format!("case-{i:02}"));
        std::fs::create_dir_all(&case)?;
        if case.join("result.json").exists() {
            return Err("use a fresh soak output directory".into());
        }
        let started = Instant::now();
        let mut child = spawn(&case, Some(cut))?;
        let deadline = started + Duration::from_secs(120);
        while !case.join("kill-ready.json").exists() {
            if let Some(status) = child.try_wait()? {
                return Err(format!(
                    "case {i} stopped before {cut}: {status}; see {}",
                    case.join("worker.log").display()
                )
                .into());
            }
            if Instant::now() > deadline {
                child.kill()?;
                child.wait()?;
                return Err(format!("case {i} never reached {cut}; see worker.log").into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill()?;
        let killed = child.wait()?;
        assert!(!killed.success());
        let found = run::find_all(&case.join("library"))
            .pop()
            .ok_or("missing killed run")?;
        let before: State = serde_json::from_slice(&std::fs::read(found.layout.state_file())?)?;
        let old = clips(&case)?;
        let cached_before = cache_hashes(&found.layout.meta().join("cache"))?;
        let mut resumed = spawn(&case, None)?;
        let deadline = Instant::now() + Duration::from_secs(180);
        let status = loop {
            if let Some(s) = resumed.try_wait()? {
                break s;
            }
            if Instant::now() > deadline {
                resumed.kill()?;
                resumed.wait()?;
                return Err(format!("case {i} resume timeout; see worker.log").into());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(
            status.success(),
            "case {i}: see {}",
            case.join("worker.log").display()
        );
        let after: State = serde_json::from_slice(&std::fs::read(found.layout.state_file())?)?;
        let cached_after = cache_hashes(&found.layout.meta().join("cache"))?;
        for (path, value) in &cached_before {
            assert_eq!(
                cached_after.get(path),
                Some(value),
                "finished cache changed: {}",
                path.display()
            );
        }
        assert!(after.active.is_none() && after.active_tasks.is_empty() && after.error.is_none());
        for (path, value) in old {
            assert_eq!(clips(&case)?.get(&path), Some(&value));
        }
        let summary: Value = serde_json::from_slice(&std::fs::read(case.join("result.json"))?)?;
        records.push(json!({"case":i,"cut":cut,"completed_cache_items_preserved":cached_before.len(),"state_items_at_kill":before.completed.len(),"elapsed_seconds":started.elapsed().as_secs_f64(),"result":summary}));
        println!("S8 case {i}: killed at {cut}, resumed successfully");
    }
    fsx::write_json_atomic(
        &root.join("soak.json"),
        &json!({"schema":1,"seed":"0x5838","runs":records,"consecutive_passes":10,"languages_per_run":["en","hi","ur"],"new_real_paid_spend_usd":0}),
    )?;
    Ok(())
}

fn cache_hashes(root: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") {
            out.insert(
                path.clone(),
                blake3::hash(&std::fs::read(path)?).to_hex().to_string(),
            );
        }
    }
    Ok(out)
}
