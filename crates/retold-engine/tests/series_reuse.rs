//! Three linked series parts share one cast: only the first part draws character sheets.
#![expect(clippy::unwrap_used, reason = "isolated fixture setup and assertions")]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared support module"
)]
mod support {
    include!("support/mod.rs");
}

use std::sync::Arc;
use std::sync::atomic::Ordering;

use retold_core::{Tier, Usd};
use retold_engine::context::Context;
use retold_engine::ledger::Ledger;
use retold_engine::media_runtime::{MediaRegistry, MediaRuntime};
use retold_engine::store::Store;
use retold_engine::{
    NewRun, Quality, RunLayout, Stage, fsx,
    pipeline::Until,
    run,
    workflow::{self, Options},
};
use retold_providers::media::MediaProvider;
use retold_providers::{ProvidersConfig, Router, TextProvider};

/// The fakes every part shares, so request counts span the whole series.
struct Fakes {
    text: Arc<support::TextFake>,
    media: Arc<support::MediaFake>,
}

fn channel(root: &std::path::Path) -> retold_guide::channel::Channel {
    let directory = root.join("config/channels/test-studio");
    if directory.exists() {
        return retold_guide::channel::Channel::load(&directory).unwrap();
    }
    retold_guide::channel::init_from_template("test-studio", &directory).unwrap()
}

fn part_inputs(guide: &retold_guide::Guide, scope: &str) -> retold_guide::inputs::Inputs {
    let raw: indexmap::IndexMap<_, _> = [
        ("title".into(), "Nosferatu".into()),
        ("kind".into(), "series".into()),
        ("scope".into(), scope.into()),
    ]
    .into();
    retold_guide::inputs::resolve(guide.manifest(), &raw).unwrap()
}

/// Create one part `series/nosferatu/<scope>` whose cast lives at `series/nosferatu`.
fn seed_source(layout: &RunLayout) {
    let source = layout.stage(Stage::Research).join("source-file.txt");
    fsx::write_atomic(&source, include_bytes!("fixtures/full/source.txt")).unwrap();
}

fn create_part(
    root: &std::path::Path,
    scope: &str,
) -> (
    RunLayout,
    retold_engine::RunMeta,
    retold_guide::Guide,
    retold_guide::channel::Channel,
) {
    let guide = retold_guide::catalog::bundled().unwrap().remove(0);
    let channel = channel(root);
    let inputs = part_inputs(&guide, scope);
    let variants = vec!["en".parse().unwrap()];
    let new = NewRun {
        guide: &guide,
        channel: &channel,
        inputs,
        variants,
        quality: Quality::Standard,
        budget: Usd::ZERO,
        skip_reviews: true,
        source_file: None,
    };
    let (layout, meta) = run::create(&root.join("library"), new).unwrap();
    seed_source(&layout);
    (layout, meta, guide, channel)
}

fn routing() -> ProvidersConfig {
    let mut config = ProvidersConfig::suggested(retold_providers::config::Found {
        claude: false,
        codex: false,
    });
    for tier in Tier::ALL {
        let media = matches!(
            tier,
            Tier::Image | Tier::ImageNoRefs | Tier::Tts | Tier::Transcribe
        );
        let route = if media {
            "openrouter:model"
        } else {
            "fixture:model"
        };
        config.routing.insert(tier, vec![route.parse().unwrap()]);
    }
    config
}

fn runtime(layout: &RunLayout, fakes: &Fakes) -> (Arc<Router>, MediaRuntime) {
    let provider = Arc::clone(&fakes.media) as Arc<dyn MediaProvider>;
    let registry = Arc::new(MediaRegistry::from_providers(
        routing(),
        vec![(provider, 2)],
    ));
    let text = Arc::clone(&fakes.text) as Arc<dyn TextProvider>;
    let table = registry.config.routing.clone();
    let text_router = Router::from_providers(vec![(text, 2)], table)
        .with_retry_backoff(std::time::Duration::ZERO);
    let store = Arc::new(tokio::sync::Mutex::new(Store::open(layout).unwrap()));
    let ledger = Arc::new(Ledger::open(&layout.meta().join("ledger.json"), Usd::ZERO).unwrap());
    let cancel = tokio_util::sync::CancellationToken::new();
    let media = MediaRuntime {
        registry,
        store,
        ledger,
        cancel,
        allow_paid: false,
    };
    (Arc::new(text_router), media)
}

fn part(root: &std::path::Path, scope: &str, fakes: &Fakes) -> Arc<Context> {
    let (layout, meta, guide, channel) = create_part(root, scope);
    let (text, media) = runtime(&layout, fakes);
    let depth = Arc::new(retold_parallax::depth::ModelDepth::default());
    let phonemes = Arc::new(retold_engine::phonemes::LocalAligner);
    Arc::new(Context {
        layout,
        meta,
        guide,
        channel,
        text,
        media,
        skip_reviews: true,
        depth,
        phonemes,
    })
}

fn sheet_requests(media: &support::MediaFake) -> usize {
    media
        .image_prompts
        .lock()
        .unwrap()
        .iter()
        .filter(|prompt| prompt.contains("SHEET"))
        .count()
}

async fn characters(context: Arc<retold_engine::context::Context>) -> retold_engine::RunLayout {
    let layout = context.layout.clone();
    let options = Options {
        fonts: Some(Arc::new(support::EstimatedFonts)),
        ..Default::default()
    };
    workflow::run(context, Until::Characters, &options)
        .await
        .unwrap();
    layout
}

#[tokio::test]
async fn three_linked_parts_reuse_the_first_parts_character_sheets() {
    let root = tempfile::tempdir().unwrap();
    let fakes = Fakes {
        text: Arc::new(support::TextFake::default()),
        media: Arc::new(support::MediaFake::default()),
    };
    let first = characters(part(root.path(), "Part 1", &fakes)).await;
    let drawn = sheet_requests(&fakes.media);
    assert!(drawn > 0, "the first part must draw the shared cast");
    let mut cast_dirs = vec![first.cast_dir()];
    for scope in ["Part 2", "Part 3"] {
        let layout = characters(part(root.path(), scope, &fakes)).await;
        assert_ne!(layout.video_dir(), first.video_dir());
        cast_dirs.push(layout.cast_dir());
    }
    assert_eq!(
        sheet_requests(&fakes.media),
        drawn,
        "later parts must reuse the shared sheets"
    );
    assert!(cast_dirs.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(fakes.media.speech.load(Ordering::SeqCst), 0);
}
