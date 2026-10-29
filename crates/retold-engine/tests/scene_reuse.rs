//! Real workflow image production consumes manual direction and reuses unaffected local caches.
#![expect(clippy::unwrap_used, reason = "isolated fixture construction")]
#[expect(
    dead_code,
    reason = "the shared fake provider supports other workflow suites"
)]
mod support {
    include!("support/mod.rs");
}

use std::sync::{Arc, atomic::Ordering};

use retold_core::Usd;
use retold_engine::context::Context;
use retold_engine::editing::scenes::{self, ShotEdit};
use retold_engine::media_runtime::{MediaRegistry, MediaRuntime};
use retold_engine::{RunLayout, RunMeta, ledger::Ledger, pipeline::Until, store::Store, workflow};
use retold_guide::{Channel, Guide};
use retold_providers::Router;
use tokio_util::sync::CancellationToken;
const VISUAL_EDIT: &str = "The original character turns away from the window.";

struct SavedContext {
    layout: RunLayout,
    meta: RunMeta,
    guide: Guide,
    channel: Channel,
    text: Arc<Router>,
    registry: Arc<MediaRegistry>,
    ledger: Arc<Ledger>,
}

fn saved_context(context: &Context) -> SavedContext {
    SavedContext {
        layout: context.layout.clone(),
        meta: context.meta.clone(),
        guide: context.guide.clone(),
        channel: context.channel.clone(),
        text: Arc::clone(&context.text),
        registry: Arc::clone(&context.media.registry),
        ledger: Arc::clone(&context.media.ledger),
    }
}

impl SavedContext {
    fn open(&self) -> Arc<Context> {
        let store = Arc::new(tokio::sync::Mutex::new(Store::open(&self.layout).unwrap()));
        Arc::new(Context {
            layout: self.layout.clone(),
            meta: self.meta.clone(),
            guide: self.guide.clone(),
            channel: self.channel.clone(),
            text: Arc::clone(&self.text),
            media: MediaRuntime {
                registry: Arc::clone(&self.registry),
                store,
                ledger: Arc::clone(&self.ledger),
                cancel: CancellationToken::new(),
                allow_paid: false,
            },
            skip_reviews: true,
            depth: Arc::new(retold_parallax::depth::ModelDepth::default()),
            phonemes: Arc::new(retold_engine::phonemes::LocalAligner),
        })
    }
    fn edit(&self) {
        let original = scenes::snapshot(&self.layout).unwrap().unwrap();
        let shot = &original.board.direction.shots[1];
        let changed = scenes::save_shot(
            &self.layout,
            ShotEdit {
                expected_version: &original.version,
                unit: shot.unit,
                action: VISUAL_EDIT,
                location: &shot.location,
                mood: &shot.mood,
            },
        )
        .unwrap();
        scenes::approve(&self.layout, &changed.version).unwrap();
    }
}

#[tokio::test]
async fn a_visual_edit_draws_its_frame_and_keeps_unaffected_provider_results() {
    let (_root, context, text, media) = support::setup(&["en"], Usd::ZERO);
    let saved = saved_context(&context);
    workflow::run(
        Arc::clone(&context),
        Until::Images,
        &workflow::Options::default(),
    )
    .await
    .unwrap();
    drop(context);
    let images = media.images.load(Ordering::SeqCst);
    let calls = text.calls.load(Ordering::SeqCst);
    let qa = text.qa.load(Ordering::SeqCst);
    saved.edit();
    workflow::run(saved.open(), Until::Images, &workflow::Options::default())
        .await
        .unwrap();
    assert_eq!(media.images.load(Ordering::SeqCst), images + 1);
    assert_used_direction(&media);
    let extra_calls = text.calls.load(Ordering::SeqCst) - calls;
    let extra_qa = text.qa.load(Ordering::SeqCst) - qa;
    assert_eq!(
        extra_calls, extra_qa,
        "Only the changed frame needs new text QA; research, writing, direction, style and casting stay cached"
    );
    assert_eq!(media.speech.load(Ordering::SeqCst), 0);
    assert_eq!(saved.ledger.total().unwrap(), Usd::ZERO);
}

fn assert_used_direction(media: &support::MediaFake) {
    let prompts = media.image_prompts.lock().unwrap();
    assert!(
        prompts.last().unwrap().contains(VISUAL_EDIT),
        "The actual image request did not consume manual direction"
    );
}
