use retold_infer::onnx::{Backend, Input, OnnxSession, TensorData};
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn cpu_and_coreml_fixture_outputs_match_and_cancel_is_bounded()
-> Result<(), Box<dyn std::error::Error>> {
    let model = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scale.onnx");
    let session = OnnxSession::default();
    let cancel = CancellationToken::new();
    let input = vec![Input {
        name: "input".into(),
        shape: vec![1, 4],
        data: TensorData::F32(vec![1.0, -2.0, 0.25, 10.0]),
    }];
    let cpu = session
        .run(&model, Backend::Cpu, input.clone(), &cancel)
        .await?;
    let coreml = session
        .run(&model, Backend::CoreMl, input.clone(), &cancel)
        .await?;
    assert_eq!(cpu[0].values, vec![2.0, -4.0, 0.5, 20.0]);
    for (a, b) in cpu[0].values.iter().zip(&coreml[0].values) {
        assert!((a - b).abs() < 1e-5);
    }
    cancel.cancel();
    let start = std::time::Instant::now();
    assert!(matches!(
        session.run(&model, Backend::Cpu, input, &cancel).await,
        Err(retold_infer::Error::Cancelled)
    ));
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    Ok(())
}
