#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]
mod support {
    include!("support/mod.rs");
}
use retold_core::Usd;
use retold_engine::{
    pipeline::Until,
    workflow::{self, Options},
};
use std::{
    collections::BTreeSet,
    fmt::Write as _,
    sync::{Arc, atomic::Ordering},
};
#[tokio::test]
async fn three_aspects_have_golden_metadata_captions_centered_native_motion_and_cached_resume() {
    let (_tmp, ctx, text, media) = support::setup(&["en:en.srt"], Usd::ZERO);
    let mut ctx = (*ctx).clone();
    let mut files = ctx.guide.files().clone();
    files
        .get_mut("guide.toml")
        .unwrap()
        .push_str("\n[delivery]\naspects=[\"9:16\",\"16:9\",\"1:1\"]\n");
    ctx.guide = retold_guide::Guide::from_files(retold_guide::Origin::Inline, files).unwrap();
    let ctx = Arc::new(ctx);
    let options = Options {
        fonts: Some(Arc::new(support::EstimatedFonts)),
        ..Default::default()
    };
    workflow::run(Arc::clone(&ctx), Until::Ready, &options)
        .await
        .unwrap();
    let manifest: workflow::Manifest = serde_json::from_slice(
        &std::fs::read(ctx.layout.deliverables().join("manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest.editions.len(), 3);
    assert!(!manifest.video_approved);
    for delivery in &manifest.editions {
        let names: BTreeSet<_> = std::fs::read_dir(&delivery.directory)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let expected: BTreeSet<_> = include_str!("fixtures/delivery-files.txt")
            .replace(
                "{caption}",
                delivery.subtitles[0].file_name().unwrap().to_str().unwrap(),
            )
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(names, expected);
        assert!(!delivery.video.exists());
        let timeline: retold_filmcraft::timeline::Timeline =
            serde_json::from_slice(&std::fs::read(&delivery.timeline).unwrap()).unwrap();
        let aspect = retold_deliver::Aspect::parse(&delivery.aspect).unwrap();
        assert_eq!([timeline.width, timeline.height], aspect.dimensions());
        assert_centered_timeline(&timeline);
    }
    let checklist = manifest.editions[0].directory.join("post.md");
    std::fs::write(&checklist, "- [x] My review is complete\n").unwrap();
    let counts = (
        text.calls.load(Ordering::SeqCst),
        media.images.load(Ordering::SeqCst),
        media.speech.load(Ordering::SeqCst),
    );
    workflow::run(Arc::clone(&ctx), Until::Ready, &options)
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
    assert_eq!(
        std::fs::read_to_string(&checklist).unwrap(),
        "- [x] My review is complete\n"
    );
    let path = manifest.editions[0]
        .directory
        .join("description.youtube.txt");
    std::fs::write(path, "tampered").unwrap();
    assert!(
        workflow::run(Arc::clone(&ctx), Until::Ready, &options)
            .await
            .unwrap_err()
            .to_string()
            .contains("delivery file changed")
    );
    assert_eq!(ctx.media.ledger.total().unwrap(), Usd::ZERO);
}

#[test]
fn invalid_or_duplicate_delivery_aspects_fail_at_guide_load() {
    let guide = retold_guide::catalog::bundled().unwrap().remove(0);
    for aspects in ["[\"9:16\",\"9:16\"]", "[\"4:3\"]", "[\"../../escape\"]"] {
        let mut files = guide.files().clone();
        writeln!(
            files.get_mut("guide.toml").unwrap(),
            "\n[delivery]\naspects={aspects}"
        )
        .unwrap();
        let error =
            retold_guide::Guide::from_files(retold_guide::Origin::Inline, files).unwrap_err();
        assert!(format!("{error:?}").contains("delivery.aspects"));
    }
}

fn assert_centered_timeline(timeline: &retold_filmcraft::timeline::Timeline) {
    assert!(
        timeline
            .clips
            .iter()
            .any(|c| c.effects.iter().any(|e| e.id == "motion"))
    );
    assert!(
        timeline
            .clips
            .iter()
            .all(|c| c.effects.iter().all(|e| e.id != "auto_reframe"))
    );
    for c in &timeline.clips {
        if let retold_filmcraft::timeline::Source::Text { position, .. } = &c.source {
            assert!(position[0] > 0.0 && position[0] < f64::from(timeline.width));
            assert!(position[1] > 0.0 && position[1] < f64::from(timeline.height));
        }
    }
}
