//! Lossless contain/pad plates let native reframing own the single crop transform.
use crate::{
    EngineError, assets::Images, context::Context, fsx, languages::Edition,
    media_runtime::MediaError,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plate {
    pub path: PathBuf,
    pub canvas: [u32; 2],
    pub zoom: f64,
    pub offset: [f64; 2],
    pub moving: bool,
    pub content_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlateKind {
    Still,
    Video,
}

impl PlateKind {
    const fn extension(self) -> &'static str {
        match self {
            Self::Still => "png",
            Self::Video => "mov",
        }
    }
}

struct EncodeJob<'a> {
    source: &'a Path,
    output: &'a Path,
    canvas: [u32; 2],
    fit: [u32; 2],
    kind: PlateKind,
}

#[must_use]
pub fn file(context: &Context, name: &str) -> PathBuf {
    context
        .layout
        .video_dir()
        .join("work/delivery-plates")
        .join(format!("{name}.json"))
}

fn load_bindings(
    context: &Context,
    edition: &Edition,
) -> Result<BTreeMap<usize, crate::local_stages::ShotBinding>, MediaError> {
    let original = crate::local_stages::bindings_file(&context.layout, edition);
    match std::fs::read(&original) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(setup),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(setup(e)),
    }
}

pub async fn prepare(
    context: &Context,
    images: &Images,
    edition: &Edition,
    name: &str,
) -> Result<(), MediaError> {
    let bindings = load_bindings(context, edition)?;
    let mut plates = BTreeMap::new();
    for frame in &images.frames {
        frame.image.verify()?;
        let (path, size, moving) = if let Some(b) = bindings.get(&frame.unit) {
            (b.path.clone(), b.dimensions, !b.camera)
        } else {
            let bytes = std::fs::read(&frame.image.path).map_err(setup)?;
            (
                frame.image.path.clone(),
                retold_providers::media::image_dimensions(&bytes)?,
                false,
            )
        };
        let canvas = context.guide.manifest().format.resolution;
        let (fitted, zoom, offset) = geometry(size, canvas, frame.qa.focus)?;
        let key = crate::store::hash(&(
            blake3::hash(&std::fs::read(&path).map_err(setup)?)
                .to_hex()
                .to_string(),
            canvas,
            fitted,
            "contain-blur-v2",
        ))?;
        let kind = if bindings.contains_key(&frame.unit) {
            PlateKind::Video
        } else {
            PlateKind::Still
        };
        let output = context
            .layout
            .video_dir()
            .join("work/delivery-plates")
            .join(format!("{key}.{}", kind.extension()));
        let step = format!("deliver-plate-{}", &key[..16]);
        let saved: Plate = crate::local_stages::run_stage_step(
            &context.media.store,
            &step,
            &serde_json::json!({"key":key}),
            &context.media.cancel,
            || async {
                encode(
                    context,
                    &EncodeJob {
                        source: &path,
                        output: &output,
                        canvas,
                        fit: fitted,
                        kind,
                    },
                )
                .await
                .map_err(|e| EngineError::Setup(e.to_string()))?;
                Ok(Plate {
                    path: output.clone(),
                    canvas,
                    zoom,
                    offset,
                    moving,
                    content_hash: blake3::hash(
                        &std::fs::read(&output).map_err(|e| EngineError::io(&output, e))?,
                    )
                    .to_hex()
                    .to_string(),
                })
            },
        )
        .await?;
        let actual = blake3::hash(&std::fs::read(&saved.path).map_err(setup)?)
            .to_hex()
            .to_string();
        if actual != saved.content_hash {
            return Err(setup("cached delivery plate changed"));
        }
        plates.insert(
            frame.unit,
            Plate {
                zoom,
                offset,
                moving,
                ..saved
            },
        );
    }
    fsx::write_json_atomic(&file(context, name), &plates)?;
    Ok(())
}

pub fn geometry(
    source: [u32; 2],
    canvas: [u32; 2],
    focus: [f64; 2],
) -> Result<([u32; 2], f64, [f64; 2]), MediaError> {
    if source.contains(&0)
        || canvas.iter().any(|v| *v < 2 || v % 2 != 0)
        || focus
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(setup("invalid reframe geometry"));
    }
    let fit = (f64::from(canvas[0]) / f64::from(source[0]))
        .min(f64::from(canvas[1]) / f64::from(source[1]));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "positive fitted dimensions bounded by the u32 canvas"
    )]
    let fitted = source.map(|v| ((f64::from(v) * fit).floor() as u32 / 2 * 2).max(2));
    // Square feed editions keep the whole composition (including a second face).
    // Portrait/widescreen use the native crop; soft extension fills free space.
    let zoom = if canvas[0] == canvas[1] {
        1.0
    } else {
        (f64::from(canvas[0]) / f64::from(fitted[0]))
            .max(f64::from(canvas[1]) / f64::from(fitted[1]))
    };
    if zoom * 1.1 > 4.0 {
        return Err(setup("source aspect exceeds native reframe zoom range"));
    }
    let offset = std::array::from_fn(|i| {
        let available = (f64::from(fitted[i]) - f64::from(canvas[i]) / zoom).max(0.0) / 2.0;
        ((focus[i] - 0.5) * f64::from(fitted[i])).clamp(-available, available)
    });
    Ok((fitted, zoom * 100.0, offset))
}

async fn encode(context: &Context, job: &EncodeJob<'_>) -> Result<(), MediaError> {
    std::fs::create_dir_all(
        job.output
            .parent()
            .ok_or_else(|| setup("plate parent missing"))?,
    )
    .map_err(setup)?;
    let partial = job.output.with_file_name(format!(
        "{}.partial.{}",
        job.output
            .file_stem()
            .ok_or_else(|| setup("plate stem"))?
            .to_string_lossy(),
        job.kind.extension()
    ));
    let mut args = vec![
        "-y".into(),
        "-i".into(),
        job.source.display().to_string(),
        "-filter_complex".into(),
        format!(
            "[0:v]split=2[b][f];[b]scale={}:{}:force_original_aspect_ratio=increase,crop={}:{},gblur=sigma=32[bg];[f]scale={}:{}[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2",
            job.canvas[0], job.canvas[1], job.canvas[0], job.canvas[1], job.fit[0], job.fit[1]
        ),
        "-an".into(),
    ];
    if job.kind == PlateKind::Video {
        args.extend([
            "-c:v".into(),
            "prores_ks".into(),
            "-profile:v".into(),
            "4".into(),
            "-pix_fmt".into(),
            "yuva444p10le".into(),
        ]);
    } else {
        args.extend(["-frames:v".into(), "1".into()]);
    }
    args.push(partial.display().to_string());
    let program =
        std::env::var_os("RETOLD_FFMPEG").map_or_else(|| PathBuf::from("ffmpeg"), PathBuf::from);
    let result = retold_providers::process::run(
        &program,
        &args,
        "",
        context.layout.video_dir(),
        &retold_providers::process::Limits::default(),
        &context.media.cancel,
    )
    .await?;
    if result.status != Some(0) {
        return Err(setup(result.stderr));
    }
    std::fs::rename(&partial, job.output).map_err(setup)?;
    Ok(())
}
fn setup(e: impl std::fmt::Display) -> MediaError {
    EngineError::Setup(e.to_string()).into()
}

#[cfg(test)]
mod tests {
    use super::geometry;
    #[test]
    fn portrait_and_landscape_keep_the_source_and_bound_focus_to_covered_pixels() {
        let (fit, zoom, offset) = geometry([960, 720], [1080, 1920], [0.29, 0.4]).unwrap();
        assert_eq!(fit, [1080, 810]);
        assert!((zoom - 237.037_037).abs() < 0.0001);
        assert!(offset[0] < 0.0);
        assert_eq!(offset[1], 0.0);
        let (fit, _, offset) = geometry([960, 720], [1920, 1080], [1.0, 0.0]).unwrap();
        assert_eq!(fit, [1440, 1080]);
        assert_eq!(offset[0], 0.0);
        assert_eq!(offset[1], -135.0);
        let (_, zoom, offset) = geometry([960, 720], [1080, 1080], [0.29, 0.4]).unwrap();
        assert_eq!(zoom, 100.0);
        assert_eq!(offset, [0.0, 0.0]);
        assert!(geometry([0, 720], [1080, 1920], [0.5, 0.5]).is_err());
        assert!(geometry([960, 720], [1080, 1920], [f64::NAN, 0.5]).is_err());
    }
}
