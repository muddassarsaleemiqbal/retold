//! Prints text regions and people found in images: `detect_probe TEXT_MODEL PERSON_MODEL IMAGE...`
use std::error::Error;
use std::path::Path;

use retold_infer::detect;
use retold_infer::onnx::OnnxSession;
use tokio_util::sync::CancellationToken;

const TEXT_MIN_SCORE: f32 = 0.6;
const PERSON_MIN_SCORE: f32 = 0.5;
const TEXT_REGIONS_SHOWN: usize = 8;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let [_, text_model, person_model, images @ ..] = &args[..] else {
        return Err("usage: detect_probe TEXT_MODEL PERSON_MODEL IMAGE...".into());
    };
    let session = OnnxSession::default();
    let cancel = CancellationToken::new();
    for image in images {
        let path = Path::new(image);
        let text = detect::text_regions(
            &session,
            Path::new(text_model),
            path,
            TEXT_MIN_SCORE,
            &cancel,
        )
        .await?;
        let people = detect::people(
            &session,
            Path::new(person_model),
            path,
            PERSON_MIN_SCORE,
            &cancel,
        )
        .await?;
        let scores: Vec<f32> = people.iter().map(|person| person.score).collect();
        println!(
            "{image}\n  text regions: {}\n  people: {} {scores:?}",
            text.len(),
            people.len()
        );
        for region in text.iter().take(TEXT_REGIONS_SHOWN) {
            println!("    text {:?} {:.2}", region.bbox, region.score);
        }
    }
    Ok(())
}
