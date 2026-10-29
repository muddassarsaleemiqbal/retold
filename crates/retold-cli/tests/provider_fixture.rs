//! Keep the offline HTTP fixture compatible with delivery's thumbnail review contract.
use std::process::Command;

#[test]
fn offline_provider_ranks_every_delivery_candidate() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/fake_openrouter.py"
    );
    let output = Command::new("python3")
        .args([
            "-c",
            "import importlib.util,json,sys; spec=importlib.util.spec_from_file_location('fixture',sys.argv[1]); module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module); print(json.dumps(module.answer('',{'properties':{'ranking':{}}})))",
            fixture,
        ])
        .output()
        .expect("load offline provider fixture without starting its server");
    assert!(output.status.success(), "{:?}", output.stderr);
    let review: retold_engine::delivery::ThumbnailReview =
        serde_json::from_slice(&output.stdout).expect("typed thumbnail review");
    assert_ranking(&review);
}

fn assert_ranking(review: &retold_engine::delivery::ThumbnailReview) {
    assert_eq!(
        review
            .ranking
            .iter()
            .map(|row| row.candidate)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(review.ranking.iter().all(|row| row.usable
        && (0.0..=10.0).contains(&row.score)
        && !row.reason.trim().is_empty()));
    assert!(
        review
            .ranking
            .windows(2)
            .all(|rows| rows[0].score >= rows[1].score)
    );
}
