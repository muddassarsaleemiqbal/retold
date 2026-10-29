//! Explicit local speech diagnostic; no paid calls.
use retold_providers::media::SpeechRequest;
use retold_voice::local::LocalVoice;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = std::env::args().nth(1).ok_or("pass request JSON file")?;
    let output = std::env::args().nth(2).ok_or("pass output WAV")?;
    let model = std::env::args().nth(3).unwrap_or_else(|| "kokoro".into());
    let req: SpeechRequest = serde_json::from_slice(&std::fs::read(request)?)?;
    let local = LocalVoice::new(
        "local",
        "espeak-ng".into(),
        std::time::Duration::from_secs(1800),
    )?;
    let start = std::time::Instant::now();
    let bytes = local
        .synthesize(&model, &req, &tokio_util::sync::CancellationToken::new())
        .await?;
    std::fs::write(output, &bytes)?;
    println!(
        "{{\"seconds\":{},\"audio_seconds\":{}}}",
        start.elapsed().as_secs_f64(),
        retold_voice::audio::Wave::decode(&bytes)?.duration()
    );
    Ok(())
}
