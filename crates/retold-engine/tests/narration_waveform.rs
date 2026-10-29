//! Recorded edition narration is located, hash-checked and reduced to a bounded waveform.
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
use retold_engine::{
    pipeline::Until,
    waveform,
    workflow::{self, Options},
};

const COLUMNS: usize = 64;

async fn captioned() -> (tempfile::TempDir, retold_engine::RunLayout) {
    let (root, context, _, _) = support::setup(&["en"], Usd::ZERO);
    workflow::run(
        Arc::clone(&context),
        Until::Captions,
        &Options {
            fonts: Some(Arc::new(support::EstimatedFonts)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    (root, context.layout.clone())
}

fn tampered_copy(
    layout: &retold_engine::RunLayout,
    source: &std::path::Path,
) -> std::path::PathBuf {
    let copy = layout.meta().join("tampered.wav");
    let mut bytes = std::fs::read(source).unwrap();
    if let Some(last) = bytes.last_mut() {
        *last ^= 1;
    }
    std::fs::write(&copy, bytes).unwrap();
    copy
}

#[tokio::test]
async fn captioned_editions_expose_hash_checked_narration_and_timed_lines() {
    let (_root, layout) = captioned().await;
    let narrations = waveform::narrations(&layout).unwrap();
    let first = narrations.first().unwrap();
    assert!(!first.lines.is_empty() && first.duration > 0.0);
    let envelope = waveform::envelope(&first.audio, &first.audio.path, COLUMNS).unwrap();
    assert!(!envelope.peaks.is_empty() && envelope.peaks.len() <= COLUMNS);
    let copy = tampered_copy(&layout, &first.audio.path);
    assert!(waveform::envelope(&first.audio, &copy, COLUMNS).is_err());
}
