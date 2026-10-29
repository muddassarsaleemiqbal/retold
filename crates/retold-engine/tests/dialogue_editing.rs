//! Edition line overrides change spoken input only after approval and keep generated originals.
#![expect(clippy::unwrap_used, reason = "isolated fixture setup and assertions")]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared support module"
)]
mod support {
    include!("support/mod.rs");
}

use std::sync::Arc;

use retold_core::Usd;
use retold_engine::editing::{
    self, EditError,
    dialogue::{self, LineEdit},
};
use retold_engine::{
    Stage,
    pipeline::Until,
    workflow::{self, Options},
};

const LINE: &str = "Ek naya, haath se likha vakya.";

async fn voiced() -> (tempfile::TempDir, retold_engine::RunLayout) {
    let (root, context, _, _) = support::setup(&["hi"], Usd::ZERO);
    workflow::run(
        Arc::clone(&context),
        Until::Voice,
        &Options {
            fonts: Some(Arc::new(support::EstimatedFonts)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let layout = context.layout.clone();
    drop(context);
    (root, layout)
}

const fn edit<'a>(version: &'a str, text: &'a str) -> LineEdit<'a> {
    LineEdit {
        expected_version: version,
        language: "hi",
        unit: 0,
        text,
    }
}

fn save_unapproved(layout: &retold_engine::RunLayout) -> dialogue::DialogueSnapshot {
    let units = layout.stage(Stage::Script).join("hi/units.json");
    let generated = std::fs::read(&units).unwrap();
    let digest = editing::digest(layout).unwrap();
    let original = dialogue::snapshot(layout, "hi").unwrap().unwrap();
    assert!(original.approved && !original.edited);
    let saved = dialogue::save_line(layout, edit(&original.version, LINE)).unwrap();
    assert_eq!(saved.lines[0].text, LINE);
    assert!(saved.lines[0].edited && !saved.approved);
    assert_eq!(std::fs::read(&units).unwrap(), generated);
    assert_ne!(editing::digest(layout).unwrap(), digest);
    let stale = dialogue::save_line(layout, edit(&original.version, "Another line."));
    assert!(matches!(stale, Err(EditError::SceneChanged)));
    saved
}

#[tokio::test]
async fn line_impact_is_read_only_and_limited_to_speech_descendants() {
    let (_root, layout) = voiced().await;
    let original = dialogue::snapshot(&layout, "hi").unwrap().unwrap();
    let impact = dialogue::preview(&layout, edit(&original.version, LINE)).unwrap();
    assert_eq!(impact.changed_units, vec![0]);
    assert!(impact.affected_stages.contains(&"voice".into()));
    assert!(!impact.affected_stages.iter().any(|stage| stage == "images"));
    assert!(!layout.meta().join("edits/dialogue/hi.json").exists());
}

#[tokio::test]
async fn an_edited_line_is_held_until_approved_and_restoring_the_original_removes_it() {
    let (_root, layout) = voiced().await;
    let saved = save_unapproved(&layout);
    assert!(matches!(
        editing::ready(&layout),
        Err(EditError::Unapproved)
    ));
    let approved = dialogue::approve(&layout, "hi", &saved.version).unwrap();
    assert!(approved.approved);
    editing::ready(&layout).unwrap();
    let generated = approved.lines[0].generated.clone();
    let restored = dialogue::save_line(&layout, edit(&approved.version, &generated)).unwrap();
    assert!(!restored.edited && restored.approved);
}

#[tokio::test]
async fn invalid_lines_and_languages_are_rejected_without_writing() {
    let (_root, layout) = voiced().await;
    let layout = &layout;
    let original = dialogue::snapshot(layout, "hi").unwrap().unwrap();
    let long = "a".repeat(dialogue::MAX_LINE_BYTES + 1);
    for text in ["  ", long.as_str()] {
        let rejected = dialogue::save_line(layout, edit(&original.version, text));
        assert!(matches!(rejected, Err(EditError::Invalid(_))));
    }
    let traversal = dialogue::snapshot(layout, "../hi");
    assert!(matches!(traversal, Err(EditError::Invalid(_))));
    assert!(!layout.meta().join("edits/dialogue/hi.json").exists());
}

/// Everything a fresh context needs except the run lock, which is acquired only when built.
#[derive(Clone)]
struct Parts {
    layout: retold_engine::RunLayout,
    meta: retold_engine::RunMeta,
    guide: retold_guide::Guide,
    channel: retold_guide::channel::Channel,
    text: Arc<retold_providers::Router>,
    registry: Arc<retold_engine::media_runtime::MediaRegistry>,
    depth: Arc<dyn retold_parallax::depth::DepthEstimator>,
    phonemes: Arc<dyn retold_engine::phonemes::AcousticAligner>,
}

impl Parts {
    fn context(&self) -> Arc<retold_engine::context::Context> {
        use retold_engine::{ledger::Ledger, media_runtime::MediaRuntime, store::Store};
        let ledger = Ledger::open(&self.layout.meta().join("ledger.json"), Usd::ZERO).unwrap();
        Arc::new(retold_engine::context::Context {
            layout: self.layout.clone(),
            meta: self.meta.clone(),
            guide: self.guide.clone(),
            channel: self.channel.clone(),
            text: Arc::clone(&self.text),
            media: MediaRuntime {
                registry: Arc::clone(&self.registry),
                store: Arc::new(tokio::sync::Mutex::new(Store::open(&self.layout).unwrap())),
                ledger: Arc::new(ledger),
                cancel: tokio_util::sync::CancellationToken::new(),
                allow_paid: false,
            },
            skip_reviews: true,
            depth: Arc::clone(&self.depth),
            phonemes: Arc::clone(&self.phonemes),
        })
    }
}

fn parts(context: &retold_engine::context::Context) -> Parts {
    Parts {
        layout: context.layout.clone(),
        meta: context.meta.clone(),
        guide: context.guide.clone(),
        channel: context.channel.clone(),
        text: Arc::clone(&context.text),
        registry: Arc::clone(&context.media.registry),
        depth: Arc::clone(&context.depth),
        phonemes: Arc::clone(&context.phonemes),
    }
}

async fn voice(context: Arc<retold_engine::context::Context>) {
    let options = Options {
        fonts: Some(Arc::new(support::EstimatedFonts)),
        ..Default::default()
    };
    workflow::run(context, Until::Voice, &options)
        .await
        .unwrap();
}

#[tokio::test]
async fn an_approved_line_is_what_the_next_voice_run_speaks() {
    let (_root, context, _, media) = support::setup(&["hi"], Usd::ZERO);
    voice(Arc::clone(&context)).await;
    let parts = parts(&context);
    drop(context);
    let saved = save_unapproved(&parts.layout);
    dialogue::approve(&parts.layout, "hi", &saved.version).unwrap();
    voice(parts.context()).await;
    let texts = media.texts.lock().unwrap();
    let spoken = texts.get("hi").unwrap();
    assert!(
        spoken.contains(LINE),
        "speech did not use the approved line: {spoken}"
    );
}
