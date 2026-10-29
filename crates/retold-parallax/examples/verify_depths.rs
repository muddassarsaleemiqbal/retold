//! Re-run the real model against the five recorded goldens, offline with installed weights.
use retold_parallax::depth::{DepthEstimator, DepthMap, ModelDepth};
use tokio_util::sync::CancellationToken;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(std::env::args().nth(1).ok_or("pass fixture directory")?);
    let model = ModelDepth::default();
    let mut results = Vec::new();
    for i in 0..5 {
        let depth = model
            .estimate(
                &root.join(format!("scene-{i}.png")),
                &CancellationToken::new(),
            )
            .await?;
        let golden = DepthMap::load(&root.join(format!("scene-{i}.rgd")))?;
        assert_eq!((depth.width, depth.height), (golden.width, golden.height));
        let difference = depth
            .values
            .iter()
            .zip(golden.values)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(difference <= 0.002, "golden {i}: {difference}");
        results.push(
            serde_json::json!({"fixture":i,"max_absolute_difference":difference,"tolerance":0.002}),
        );
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
