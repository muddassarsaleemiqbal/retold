//! Explicit fixture/evidence capture with the real pinned local model; never a test fallback.
use retold_parallax::depth::{DepthEstimator, ModelDepth};
use tokio_util::sync::CancellationToken;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let directory = std::path::Path::new(
        args.first()
            .ok_or("pass output directory and source illustrations")?,
    );
    std::fs::create_dir_all(directory)?;
    let estimator = ModelDepth::default();
    let mut records = Vec::new();
    for (i, source) in args.iter().skip(1).enumerate() {
        let image = retold_parallax::load_image(std::path::Path::new(source))?
            .thumbnail(160, 160)
            .into_rgb8();
        let path = directory.join(format!("scene-{i}.png"));
        image.save(&path)?;
        let start = std::time::Instant::now();
        let depth = estimator.estimate(&path, &CancellationToken::new()).await?;
        let seconds = start.elapsed().as_secs_f64();
        let output = directory.join(format!("scene-{i}.rgd"));
        depth.save(&output)?;
        records.push(serde_json::json!({"source":std::path::Path::new(source).file_name().map(|n|n.to_string_lossy()),"image":path.file_name().map(|n|n.to_string_lossy()),"depth":output.file_name().map(|n|n.to_string_lossy()),"model":estimator.identity()?,"width":depth.width,"height":depth.height,"seconds":seconds,"depth_hash":blake3::hash(&std::fs::read(output)?).to_hex().to_string()}));
    }
    std::fs::write(
        directory.join("goldens.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    println!("{}", directory.join("goldens.json").display());
    Ok(())
}
