//! A reviewed frame retake changes exactly one revision and clears only visual completion.
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
    EditError,
    retake::{self, Retake},
};
use retold_engine::{
    pipeline::Until,
    store,
    workflow::{self, Options},
};

async fn framed() -> (tempfile::TempDir, retold_engine::RunLayout) {
    let (root, context, _, _) = support::setup(&["en"], Usd::ZERO);
    workflow::run(
        Arc::clone(&context),
        Until::Images,
        &Options {
            fonts: Some(Arc::new(support::EstimatedFonts)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    (root, context.layout.clone())
}

#[tokio::test]
async fn a_frame_retake_bumps_one_revision_once_and_rejects_stale_reviews() {
    let (_root, layout) = framed().await;
    let before = store::read_state(&layout).unwrap();
    let reviewed = retake::plan(&layout, &Retake::Frame(1)).unwrap();
    assert_eq!(reviewed.scope, "image-1-0");
    assert!(reviewed.affected_stages.contains(&"images".into()));
    assert_eq!(
        store::read_state(&layout).unwrap().revisions,
        before.revisions
    );
    let applied = retake::request(&layout, &reviewed, &Retake::Frame(1)).unwrap();
    assert_eq!(applied.revision, reviewed.revision + 1);
    let repeated = retake::request(&layout, &reviewed, &Retake::Frame(1)).unwrap();
    assert_eq!(repeated, applied);
    let after = store::read_state(&layout).unwrap();
    assert!(after.completed.contains_key("storyboard"));
    assert!(!after.completed.contains_key("images"));
    assert_eq!(after.revision("image-0-0"), before.revision("image-0-0"));
    retake::request(&layout, &applied, &Retake::Frame(1)).unwrap();
    let stale = retake::request(&layout, &reviewed, &Retake::Frame(1));
    assert!(matches!(stale, Err(EditError::SceneChanged)));
}

#[tokio::test]
async fn unknown_scenes_and_mismatched_reviews_are_rejected() {
    let (_root, layout) = framed().await;
    assert!(matches!(
        retake::plan(&layout, &Retake::Frame(999)),
        Err(EditError::Invalid(_))
    ));
    let narration = retake::plan(&layout, &Retake::Narration).unwrap();
    let mismatched = retake::request(&layout, &narration, &Retake::Frame(0));
    assert!(matches!(mismatched, Err(EditError::Invalid(_))));
}
