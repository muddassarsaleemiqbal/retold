//! Upscales an image 2× with the local model: `upscale_probe MODEL IN OUT`.
use std::error::Error;
use std::path::PathBuf;

use retold_infer::onnx::OnnxSession;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let [_, model, input, output] = &args[..] else {
        return Err("usage: upscale_probe MODEL IN OUT".into());
    };
    let image = image::open(input)?.to_rgb8();
    let start = std::time::Instant::now();
    let session = OnnxSession::default();
    let model = PathBuf::from(model);
    let upscaled =
        retold_infer::upscale::upscale_x2(&session, &model, &image, &CancellationToken::new())
            .await?;
    upscaled.save(output)?;
    println!(
        "{:?} -> {:?} in {:.1}s",
        image.dimensions(),
        upscaled.dimensions(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
