#[cfg(test)]
mod tests {
    mod support {
        include!("support/mod.rs");
    }
    use retold_core::Usd;
    use retold_engine::{
        Stage,
        context::Context,
        pipeline::Until,
        workflow::{self, Options},
    };
    use retold_guide::{Guide, Origin};
    use retold_parallax::depth::{DepthEstimator, DepthMap};
    use std::fmt::Write;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio_util::sync::CancellationToken;
    #[derive(Debug, Default)]
    struct RecordedDepth {
        calls: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl DepthEstimator for RecordedDepth {
        fn identity(&self) -> Result<String, retold_parallax::Error> {
            Ok("recorded-depth-anything-small-v1".into())
        }
        async fn estimate(
            &self,
            _: &std::path::Path,
            _: &CancellationToken,
        ) -> Result<DepthMap, retold_parallax::Error> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            DepthMap::load(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../retold-parallax/tests/fixtures/scene-0.rgd"),
            )
        }
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum HeroFill {
        Off,
        On,
    }
    fn enabled(context: &Context, depth: Arc<RecordedDepth>, hero: HeroFill) -> Context {
        let mut result = context.clone();
        let mut files = result.guide.files().clone();
        let manifest = files.get_mut("guide.toml").unwrap();
        let fill = hero == HeroFill::On;
        write!(
            manifest,
            "\n[visuals.parallax]\nenabled=true\nmax_share=0.35\nstrength=0.35\nhero_fill={fill}\n"
        )
        .unwrap();
        result.guide = Guide::from_files(Origin::Inline, files).unwrap();
        result.depth = depth;
        result
    }
    fn options() -> Options {
        Options {
            fonts: Some(Arc::new(support::EstimatedFonts)),
            ..Default::default()
        }
    }
    #[tokio::test]
    async fn enabled_pipeline_uses_recorded_real_depth_and_reuses_calls_on_resume_and_redo() {
        let (_tmp, ctx, text, media) = support::setup(&["en"], Usd::ZERO);
        media.small_images.store(true, Ordering::SeqCst);
        let depth = Arc::new(RecordedDepth::default());
        let ctx = Arc::new(enabled(&ctx, Arc::clone(&depth), HeroFill::Off));
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        assert_eq!(
            depth.calls.load(Ordering::SeqCst),
            1,
            "identical images share the content-addressed depth result"
        );
        let counts = (
            text.calls.load(Ordering::SeqCst),
            media.images.load(Ordering::SeqCst),
            media.speech.load(Ordering::SeqCst),
        );
        let first: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                ctx.layout
                    .video_dir()
                    .join("work/parallax-shots/shots-0-en.audit.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let second: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                ctx.layout
                    .video_dir()
                    .join("work/parallax-shots/shots-1-en.audit.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_ne!(first["fallback"], "flat");
        let second_path = std::path::PathBuf::from(second["path"].as_str().unwrap());
        let second_modified = std::fs::metadata(&second_path).unwrap().modified().unwrap();
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        assert_eq!(depth.calls.load(Ordering::SeqCst), 1);
        ctx.media
            .store
            .lock()
            .await
            .bump_revision("shots-0")
            .unwrap();
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        assert_eq!(
            second_modified,
            std::fs::metadata(second_path).unwrap().modified().unwrap()
        );
        assert_eq!(
            counts,
            (
                text.calls.load(Ordering::SeqCst),
                media.images.load(Ordering::SeqCst),
                media.speech.load(Ordering::SeqCst)
            )
        );
        ctx.media
            .store
            .lock()
            .await
            .bump_revision("depth-0")
            .unwrap();
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        assert_eq!(depth.calls.load(Ordering::SeqCst), 2);
        let timeline: retold_filmcraft::timeline::Timeline = serde_json::from_slice(
            &std::fs::read(ctx.layout.stage(Stage::Edit).join("en.timeline.json")).unwrap(),
        )
        .unwrap();
        assert!(timeline.clips.iter().any(|c|matches!(&c.source,retold_filmcraft::timeline::Source::Media{path} if path.extension().is_some_and(|e|e=="mov"))));
        assert_eq!(ctx.media.ledger.total().unwrap(), Usd::ZERO);
    }
    #[tokio::test]
    async fn hero_fill_requires_permission_and_uses_durable_ledger_receipts() {
        let (_tmp, ctx, _, media) = support::setup(&["en"], Usd::from_micros(2_000_000));
        media.small_images.store(true, Ordering::SeqCst);
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        let previous_cost = ctx.media.ledger.total().unwrap();
        let depth = Arc::new(RecordedDepth::default());
        let mut configured = enabled(&ctx, depth, HeroFill::On);
        configured.media.allow_paid = false;
        let ctx = Arc::new(configured);
        let result = workflow::run(Arc::clone(&ctx), Until::Ready, &options()).await;
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("hero_fill requires --allow-paid")
        );
        // Existing image permission is also enforced before an opt-in background can be requested.
        let mut allowed = (*ctx).clone();
        allowed.media.allow_paid = true;
        let ctx = Arc::new(allowed);
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        assert!(
            ctx.media
                .store
                .lock()
                .await
                .state
                .completed
                .contains_key("depth-fill-0")
        );
        let cost = ctx.media.ledger.total().unwrap();
        assert_eq!(cost, previous_cost + Usd::from_micros(5_000));
        let images = media.images.load(Ordering::SeqCst);
        workflow::run(Arc::clone(&ctx), Until::Ready, &options())
            .await
            .unwrap();
        assert_eq!(cost, ctx.media.ledger.total().unwrap());
        assert_eq!(images, media.images.load(Ordering::SeqCst));
    }
}
