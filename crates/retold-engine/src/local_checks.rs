//! Free local detectors run before any model is asked about an image: printed text on every
//! generated image, and people on location and prop references (which must have none).
//! Each runs only when its model is installed (`retold models install text-detect` /
//! `person-detect`); otherwise it is skipped and the vision check covers it.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use retold_infer::detect;
use retold_infer::onnx::OnnxSession;
use tokio_util::sync::CancellationToken;

use crate::media_runtime::MediaError;

fn session() -> &'static OnnxSession {
    static SESSION: OnceLock<OnnxSession> = OnceLock::new();
    SESSION.get_or_init(OnnxSession::default)
}

/// The verified model file, or `None` when the bundle is not installed here.
fn model(name: &str, file: &str) -> Option<PathBuf> {
    let store = retold_infer::models::ModelStore::discover().ok()?;
    let dir = store.verify(&store.manifest(name).ok()?).ok()?;
    Some(dir.join(file))
}

fn failed(e: &retold_infer::Error) -> MediaError {
    MediaError::Quality(format!("local image check failed: {e}"))
}

/// Text-line shapes only: confident, at least 0.8% of the image height tall and at least
/// 2.5 times wider than tall in pixels. Folds, buttons and hatching fail these.
fn is_text_line(r: &detect::Region, width: u32, height: u32) -> bool {
    let w = f64::from(r.bbox[2] - r.bbox[0]) * f64::from(width);
    let h = f64::from(r.bbox[3] - r.bbox[1]) * f64::from(height);
    r.score >= 0.9 && h >= f64::from(height) * 0.008 && w >= h * 2.5
}

/// Boxes (normalized x0, y0, x1, y1) of clear printed text lines; empty when there is none
/// or the detector is not installed.
pub async fn text_boxes(
    path: &Path,
    cancel: &CancellationToken,
) -> Result<Vec<[f32; 4]>, MediaError> {
    let Some(model) = model("text-detect", "detection/v3/det.onnx") else {
        return Ok(vec![]);
    };
    let (width, height) = image::image_dimensions(path).map_err(|e| {
        MediaError::Quality(format!("{} is not a readable image: {e}", path.display()))
    })?;
    Ok(detect::text_regions(session(), &model, path, 0.6, cancel)
        .await
        .map_err(|e| failed(&e))?
        .into_iter()
        .filter(|r| is_text_line(r, width, height))
        .map(|r| r.bbox)
        .collect())
}

/// A problem when the image clearly contains printed words.
pub async fn printed_text(
    path: &Path,
    cancel: &CancellationToken,
) -> Result<Option<String>, MediaError> {
    let lines = text_boxes(path, cancel).await?;
    Ok((!lines.is_empty()).then(|| {
        let at: Vec<_> = lines.iter().take(3).map(|b| format!("x {:.2}–{:.2}, y {:.2}–{:.2}", b[0], b[2], b[1], b[3])).collect();
        format!("printed text or labels detected ({}); draw no letters, words, numbers or labels anywhere", at.join("; "))
    }))
}

/// A white-on-black edit mask covering `boxes` (expanded by half their height) at `size`.
#[must_use]
pub fn text_mask(boxes: &[[f32; 4]], size: (u32, u32)) -> image::RgbImage {
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    let rects: Vec<_> = boxes
        .iter()
        .map(|b| {
            let pad = f64::from(b[3] - b[1]) * h * 0.5;
            (
                f64::from(b[0]) * w - pad,
                f64::from(b[1]) * h - pad,
                f64::from(b[2]) * w + pad,
                f64::from(b[3]) * h + pad,
            )
        })
        .collect();
    image::RgbImage::from_fn(size.0, size.1, |x, y| {
        let (x, y) = (f64::from(x), f64::from(y));
        let inside = rects
            .iter()
            .any(|r| x >= r.0 && x <= r.2 && y >= r.1 && y <= r.3);
        image::Rgb(if inside { [255; 3] } else { [0; 3] })
    })
}

/// A problem when a reference that must be empty of people clearly shows one. The detector
/// misses many stylised figures, so it is trusted only for what it does find.
pub async fn people_in_empty_reference(
    path: &Path,
    cancel: &CancellationToken,
) -> Result<Option<String>, MediaError> {
    let Some(model) = model("person-detect", "onnx/model.onnx") else {
        return Ok(None);
    };
    let found = detect::people(session(), &model, path, 0.85, cancel)
        .await
        .map_err(|e| failed(&e))?;
    Ok((!found.is_empty()).then(|| {
        format!("{} person figure(s) detected; this reference must show no people, characters or figures at all", found.len())
    }))
}

/// `img` scaled to exactly `size`. Enlargements of 1.3× or more first go through the local
/// 2× super-resolution model when it is installed (`retold models install upscale`), which
/// keeps line art crisp; the rest is plain Lanczos resampling.
pub async fn enlarge(
    img: image::RgbImage,
    size: (u32, u32),
    cancel: &CancellationToken,
) -> Result<image::RgbImage, MediaError> {
    let grows = f64::from(size.0) >= f64::from(img.width()) * 1.3;
    let img = match model("upscale", "onnx/model.onnx").filter(|_| grows) {
        Some(model) => retold_infer::upscale::upscale_x2(session(), &model, &img, cancel)
            .await
            .map_err(|e| failed(&e))?,
        None => img,
    };
    Ok(image::imageops::resize(
        &img,
        size.0,
        size.1,
        image::imageops::FilterType::Lanczos3,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_cover_padded_text_boxes_only() {
        let mask = text_mask(&[[0.25, 0.5, 0.75, 0.6]], (100, 100));
        assert_eq!(mask.get_pixel(50, 55).0, [255; 3]);
        assert_eq!(mask.get_pixel(50, 46).0, [255; 3]);
        assert_eq!(mask.get_pixel(50, 80).0, [0; 3]);
        assert_eq!(mask.get_pixel(5, 55).0, [0; 3]);
    }

    #[test]
    fn only_wide_confident_lines_count_as_text() {
        let r = |bbox, score| detect::Region { bbox, score };
        // A word label on a 1254×1254 sheet (measured): 74×17 px.
        assert!(is_text_line(
            &r([0.12, 0.883, 0.178, 0.897], 1.0),
            1254,
            1254
        ));
        // A coat fold on a 1080×1920 frame (measured false hit): 173×102 px.
        assert!(!is_text_line(
            &r([0.772, 0.754, 0.932, 0.807], 0.96),
            1080,
            1920
        ));
        // A button speck.
        assert!(!is_text_line(
            &r([0.485, 0.692, 0.498, 0.696], 0.81),
            1080,
            1920
        ));
    }
}
