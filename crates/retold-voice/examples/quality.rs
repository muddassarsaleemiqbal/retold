//! Real offline ASR and speaker-embedding measurement for an explicit WAV.
use retold_providers::{local_whisper::LocalWhisper, media::MediaProvider};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::PathBuf::from(std::env::args().nth(1).ok_or("audio")?);
    let model = std::path::PathBuf::from(std::env::args().nth(2).ok_or("whisper model")?);
    let lang = std::env::args().nth(3).ok_or("language")?.parse()?;
    let expected = std::env::args().nth(4).ok_or("expected text")?;
    let reference = std::env::args().nth(5);
    let cancel = tokio_util::sync::CancellationToken::new();
    let whisper = LocalWhisper {
        id: "whisper".into(),
        binary: "whisper-cli".into(),
        model,
        ffmpeg: "ffmpeg".into(),
        limits: retold_providers::process::Limits {
            timeout: std::time::Duration::from_secs(600),
            ..Default::default()
        },
    };
    let result = whisper
        .transcribe("local", &std::fs::read(&path)?, "wav", &lang, &cancel)
        .await?
        .value;
    let score = retold_voice::audio::accuracy(&expected, &result.text);
    let local = retold_voice::local::LocalVoice::new(
        "local",
        "espeak-ng".into(),
        std::time::Duration::from_secs(600),
    )?;
    let emb = local.embedding(&path, &cancel).await?;
    let similarity = if let Some(reference) = reference {
        Some(retold_voice::audio::cosine(
            &emb,
            &local
                .embedding(std::path::Path::new(&reference), &cancel)
                .await?,
        )?)
    } else {
        None
    };
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"transcript":result,"accuracy":score,"embedding":emb,"similarity":similarity})
        )?
    );
    Ok(())
}
