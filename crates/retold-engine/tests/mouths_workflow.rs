#[cfg(test)]
mod tests {
    // Offline fallback contracts. All model responses and media come from fixtures.
    #[expect(
        dead_code,
        reason = "each test binary uses a different part of the shared support module"
    )]
    mod support {
        include!("support/mod.rs");
    }
    use retold_core::Usd;
    use retold_engine::{
        Stage,
        assets::Images,
        languages::Edition,
        mouths,
        pipeline::Until,
        workflow::{self, Options},
    };
    use retold_guide::{Guide, Origin};
    use std::sync::{Arc, atomic::Ordering};

    #[tokio::test]
    async fn disabled_invalid_dialogue_unknown_speaker_and_no_visible_faces_keep_stills() {
        let (_tmp, ctx, text, media) = support::setup(&["en"], Usd::ZERO);
        media.small_images.store(true, Ordering::SeqCst);
        workflow::run(
            Arc::clone(&ctx),
            Until::Ready,
            &Options {
                fonts: Some(Arc::new(support::EstimatedFonts)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let images: Images = serde_json::from_slice(
            &std::fs::read(ctx.layout.stage(Stage::Images).join("images.json")).unwrap(),
        )
        .unwrap();
        let mut editions: Vec<Edition> = serde_json::from_slice(
            &std::fs::read(ctx.layout.stage(Stage::Captions).join("editions.json")).unwrap(),
        )
        .unwrap();
        let edition = &mut editions[0];
        let before = text.calls.load(Ordering::SeqCst);
        assert!(
            mouths::attachments(&ctx, &images, edition, 0, 24, 0.0)
                .await
                .unwrap()
                .is_empty()
        );
        let mut enabled = (*ctx).clone();
        let mut files = enabled.guide.files().clone();
        let mut manifest: toml::Value = toml::from_str(&files["guide.toml"]).unwrap();
        manifest["narration"]["mode"] = "dialogue".into();
        manifest["visuals"].as_table_mut().unwrap().insert(
            "lipsync".into(),
            toml::Value::Table(std::iter::once(("enabled".into(), true.into())).collect()),
        );
        files.insert("guide.toml".into(), toml::to_string(&manifest).unwrap());
        enabled.guide = Guide::from_files(Origin::Inline, files).unwrap();
        edition.units[0].text = "not a dialogue cue".into();
        assert!(
            mouths::attachments(&enabled, &images, edition, 0, 24, 0.0)
                .await
                .unwrap()
                .is_empty()
        );
        edition.units[0].text = "Unknown: Hello.".into();
        assert!(
            mouths::attachments(&enabled, &images, edition, 0, 24, 0.0)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(before, text.calls.load(Ordering::SeqCst));
        edition.alignment.units[0].words.truncate(1);
        let speaker = images.sheets.keys().next().expect("fixture cast");
        edition.units[0].text = format!("{speaker}: Hello.");
        assert!(
            mouths::attachments(&enabled, &images, edition, 0, 24, 0.0)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(before + 1, text.calls.load(Ordering::SeqCst));
        assert_eq!(ctx.media.ledger.total().unwrap(), Usd::ZERO);
    }
}
