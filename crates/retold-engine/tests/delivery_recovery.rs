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
use retold_core::Usd;
use retold_engine::{
    assets::{Frame, ImageQa, Images},
    delivery_plates,
    languages::{Edition, Packaging},
    media_runtime::Asset,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
fn asset(path: &Path) -> Asset {
    Asset {
        path: path.into(),
        content_hash: blake3::hash(&std::fs::read(path).unwrap())
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
fn completed(folder: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().is_some_and(|e| e == "png")
                        && !p.to_string_lossy().contains("partial")
                })
                .collect()
        })
        .unwrap_or_default()
}
#[tokio::test]
async fn cancelled_plate_batch_resumes_without_reencoding_finished_items_or_provider_calls() {
    let (tmp, ctx, text, media) = support::setup(&["en"], Usd::ZERO);
    let mut frames = Vec::new();
    for i in 0..2 {
        let path = tmp.path().join(format!("source-{i}.png"));
        image::RgbaImage::from_pixel(2048, 1024, image::Rgba([180 + i * 20, 140, 90, 255]))
            .save(&path)
            .unwrap();
        frames.push(Frame {
            unit: usize::from(i),
            image: asset(&path),
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
        });
    }
    let first = frames[0].image.clone();
    let images = Images {
        cover: first.clone(),
        style: first.clone(),
        frames,
        sheets: BTreeMap::new(),
    };
    let edition = Edition {
        variant: "en".parse().unwrap(),
        units: vec![],
        audio: first,
        alignment: retold_lang::Alignment {
            duration: 1.0,
            matched: 1.0,
            units: vec![],
        },
        captions: vec![],
        sidecars: vec![],
        packaging: Packaging { posts: vec![] },
    };
    let folder = ctx.layout.video_dir().join("work/delivery-plates");
    let task = tokio::spawn({
        let ctx = Arc::clone(&ctx);
        let images = images.clone();
        let edition = edition.clone();
        async move { delivery_plates::prepare(&ctx, &images, &edition, "en-9x16").await }
    });
    let path = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(path) = completed(&folder).first() {
                break path.clone();
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    ctx.media.cancel.cancel();
    assert!(task.await.unwrap().is_err());
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let mut resumed = (*ctx).clone();
    resumed.media.cancel = tokio_util::sync::CancellationToken::new();
    delivery_plates::prepare(&resumed, &images, &edition, "en-9x16")
        .await
        .unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(completed(&folder).len(), 2);
    delivery_plates::prepare(&resumed, &images, &edition, "en-9x16")
        .await
        .unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(text.calls.load(Ordering::SeqCst), 0);
    assert_eq!(media.images.load(Ordering::SeqCst), 0);
    assert_eq!(resumed.media.ledger.total().unwrap(), Usd::ZERO);
}
