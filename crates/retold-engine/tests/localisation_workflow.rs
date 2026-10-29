#[cfg(test)]
mod tests {
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
        pipeline::Until,
        workflow::{self, Options},
    };
    use std::sync::{Arc, atomic::Ordering};
    #[tokio::test]
    async fn an_idiom_issue_rejects_the_edition_and_repair_must_pass_before_speech() {
        let (_tmp, ctx, text, _) = support::setup(&["hi"], Usd::ZERO);
        text.reject_first_idiom.store(true, Ordering::SeqCst);
        workflow::run(
            Arc::clone(&ctx),
            Until::Voice,
            &Options {
                fonts: Some(Arc::new(support::EstimatedFonts)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let folder = ctx.layout.stage(Stage::Script).join("hi");
        let failed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.join("gates-0.json")).unwrap()).unwrap();
        assert!(
            failed["problems"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p.as_str().unwrap().contains("idiom"))
        );
        let repaired: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.join("gates-1.json")).unwrap()).unwrap();
        assert_eq!(repaired["problems"], serde_json::json!([]));
        assert_eq!(ctx.media.ledger.total().unwrap(), Usd::ZERO);
    }
}
