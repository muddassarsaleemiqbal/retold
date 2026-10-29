//! Local image checks with small ONNX detectors: printed text regions (the `PaddleOCR`
//! differentiable-binarization text detector) and people (RT-DETR, COCO class 0). Both are free, run on the CPU, and are
//! used only to catch clear failures before a model is asked.

use std::path::Path;

use image::RgbImage;
use image::imageops::FilterType;
use tokio_util::sync::CancellationToken;

use crate::Error;
use crate::onnx::{Backend, Input, OnnxSession, TensorData};
use retold_core::num;

/// A detected region in normalized [x0, y0, x1, y1] coordinates, with its confidence.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Region {
    pub bbox: [f32; 4],
    pub score: f32,
}

fn load(path: &Path) -> Result<RgbImage, Error> {
    Ok(image::open(path)
        .map_err(|e| Error::Invalid(format!("{}: {e}", path.display())))?
        .to_rgb8())
}

/// Text regions found by the DB detector: probability map > 0.3, connected regions whose
/// mean probability is at least `min_score` and that are at least 0.4% of the shorter side tall.
pub async fn text_regions(
    session: &OnnxSession,
    model: &Path,
    image: &Path,
    min_score: f32,
    cancel: &CancellationToken,
) -> Result<Vec<Region>, Error> {
    let img = load(image)?;
    let (width, height) = text_map_size(img.width(), img.height());
    let small = image::imageops::resize(&img, width, height, FilterType::Triangle);
    let input = Input {
        name: "x".into(),
        shape: vec![1, 3, i64::from(height), i64::from(width)],
        data: TensorData::F32(text_input(&small)),
    };
    let outputs = session
        .run(model, Backend::Cpu, vec![input], cancel)
        .await?;
    let output = outputs
        .first()
        .ok_or_else(|| Error::Invalid("text detector returned no tensor".into()))?;
    let (width, height) = (width as usize, height as usize);
    if output.values.len() != width * height {
        return Err(Error::Invalid(format!(
            "text detector returned {} values for a {width}x{height} map",
            output.values.len()
        )));
    }
    let map = ProbabilityMap {
        values: &output.values,
        width,
        height,
    };
    Ok(map.regions(min_score))
}

/// Probability above which a map pixel counts as text.
const TEXT_THRESHOLD: f32 = 0.3;
/// The text detector's longest input side.
const TEXT_LONGEST_SIDE: f64 = 960.0;
/// Text detector input sides are multiples of this.
const TEXT_STRIDE: u32 = 32;
/// `ImageNet` channel means and standard deviations the text detector was trained with, in BGR.
const TEXT_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const TEXT_STD: [f32; 3] = [0.229, 0.224, 0.225];
/// 8-bit channel maximum.
const CHANNEL_MAX: f32 = 255.0;

/// Input size for the text detector: longest side 960, both sides multiples of 32.
fn text_map_size(width: u32, height: u32) -> (u32, u32) {
    let scale = TEXT_LONGEST_SIDE / f64::from(width.max(height));
    let fit = |side: u32| {
        let strides = (f64::from(side) * scale / f64::from(TEXT_STRIDE)).max(1.0);
        num::round_u32(strides) * TEXT_STRIDE
    };
    (fit(width), fit(height))
}

/// Normalized planar BGR floats, as the text detector expects.
fn text_input(image: &RgbImage) -> Vec<f32> {
    let plane = image.pixels().len();
    let mut values = vec![0.0_f32; plane * 3];
    for (index, pixel) in image.pixels().enumerate() {
        let [red, green, blue] = pixel.0;
        for (channel, value) in [blue, green, red].into_iter().enumerate() {
            let unit = f32::from(value) / CHANNEL_MAX;
            values[channel * plane + index] = (unit - TEXT_MEAN[channel]) / TEXT_STD[channel];
        }
    }
    values
}

/// A connected region of the probability map being grown: pixel bounds and mean probability.
struct Blob {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
    probability_sum: f32,
    pixels: usize,
}

impl Blob {
    const fn at(column: usize, row: usize) -> Self {
        Self {
            left: column,
            top: row,
            right: column,
            bottom: row,
            probability_sum: 0.0,
            pixels: 0,
        }
    }

    fn add(&mut self, column: usize, row: usize, probability: f32) {
        self.left = self.left.min(column);
        self.top = self.top.min(row);
        self.right = self.right.max(column);
        self.bottom = self.bottom.max(row);
        self.probability_sum += probability;
        self.pixels += 1;
    }

    fn score(&self) -> f32 {
        self.probability_sum / num::count_f32(self.pixels.max(1))
    }

    /// Normalized `[x0, y0, x1, y1]` within a `width` x `height` map.
    fn bbox(&self, width: usize, height: usize) -> [f32; 4] {
        let (width, height) = (num::count_f32(width), num::count_f32(height));
        [
            num::count_f32(self.left) / width,
            num::count_f32(self.top) / height,
            num::count_f32(self.right + 1) / width,
            num::count_f32(self.bottom + 1) / height,
        ]
    }

    fn shorter_side(&self) -> f32 {
        num::count_f32((self.right - self.left + 1).min(self.bottom - self.top + 1))
    }
}

/// Smallest text region side, as a fraction of the map's shorter side.
const MIN_SIDE_FRACTION: f32 = 0.004;
/// Smallest text region side in map pixels, whatever the map size.
const MIN_SIDE_PIXELS: f32 = 3.0;

/// A text-probability map from the detector, row-major.
struct ProbabilityMap<'a> {
    values: &'a [f32],
    width: usize,
    height: usize,
}

impl ProbabilityMap<'_> {
    /// Connected regions (4-neighbour) above [`TEXT_THRESHOLD`], kept when their mean
    /// probability reaches `min_score` and they are not specks.
    fn regions(&self, min_score: f32) -> Vec<Region> {
        let mut seen = vec![false; self.values.len()];
        let shorter_side = num::count_f32(self.width.min(self.height));
        let min_side = (shorter_side * MIN_SIDE_FRACTION).max(MIN_SIDE_PIXELS);
        let mut regions = Vec::new();
        for start in 0..self.values.len() {
            if seen[start] || self.values[start] <= TEXT_THRESHOLD {
                continue;
            }
            let blob = self.grow(start, &mut seen);
            let score = blob.score();
            if score >= min_score && blob.shorter_side() >= min_side {
                regions.push(Region {
                    bbox: blob.bbox(self.width, self.height),
                    score,
                });
            }
        }
        regions
    }

    /// Flood-fills the region containing `start`, marking its pixels in `seen`.
    fn grow(&self, start: usize, seen: &mut [bool]) -> Blob {
        let mut blob = Blob::at(start % self.width, start / self.width);
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(index) = stack.pop() {
            blob.add(index % self.width, index / self.width, self.values[index]);
            for next in self.neighbours(index).into_iter().flatten() {
                if !seen[next] && self.values[next] > TEXT_THRESHOLD {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        blob
    }

    /// The 4-neighbours of `index` that lie inside the map.
    fn neighbours(&self, index: usize) -> [Option<usize>; 4] {
        let (column, row) = (index % self.width, index / self.width);
        [
            (column > 0).then(|| index - 1),
            (column + 1 < self.width).then(|| index + 1),
            (row > 0).then(|| index - self.width),
            (row + 1 < self.height).then(|| index + self.width),
        ]
    }
}

/// People found by RT-DETR (640×640 input, scores from sigmoid logits, COCO class 0),
/// at or above `min_score`, after merging boxes that overlap by more than half.
pub async fn people(
    session: &OnnxSession,
    model: &Path,
    image: &Path,
    min_score: f32,
    cancel: &CancellationToken,
) -> Result<Vec<Region>, Error> {
    let img = load(image)?;
    let small = image::imageops::resize(&img, 640, 640, FilterType::Triangle);
    let plane = 640 * 640;
    let mut values = vec![0.0_f32; plane * 3];
    for (i, p) in small.pixels().enumerate() {
        for c in 0..3 {
            values[c * plane + i] = f32::from(p.0[c]) / 255.0;
        }
    }
    let input = Input {
        name: "pixel_values".into(),
        shape: vec![1, 3, 640, 640],
        data: TensorData::F32(values),
    };
    let outputs = session
        .run(model, Backend::Cpu, vec![input], cancel)
        .await?;
    let logits = outputs
        .iter()
        .find(|o| o.name == "logits")
        .ok_or_else(|| Error::Invalid("person detector returned no logits".into()))?;
    let boxes = outputs
        .iter()
        .find(|o| o.name == "pred_boxes")
        .ok_or_else(|| Error::Invalid("person detector returned no boxes".into()))?;
    let classes = usize::try_from(*logits.shape.last().unwrap_or(&1)).unwrap_or(1);
    let mut found: Vec<Region> = Vec::new();
    for (q, chunk) in boxes.values.as_chunks::<4>().0.iter().enumerate() {
        let score = 1.0 / (1.0 + (-logits.values[q * classes]).exp());
        if score < min_score {
            continue;
        }
        let [cx, cy, bw, bh] = [chunk[0], chunk[1], chunk[2], chunk[3]];
        found.push(Region {
            bbox: [cx - bw / 2.0, cy - bh / 2.0, cx + bw / 2.0, cy + bh / 2.0],
            score,
        });
    }
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Region> = Vec::new();
    for r in found {
        if kept.iter().all(|k| iou(k.bbox, r.bbox) <= 0.5) {
            kept.push(r);
        }
    }
    Ok(kept)
}

fn iou(a: [f32; 4], b: [f32; 4]) -> f32 {
    let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let inter = w * h;
    let area = |r: [f32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
    inter / (area(a) + area(b) - inter).max(f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_keep_confident_blocks_and_drop_specks() {
        let (w, h) = (100, 100);
        let mut prob = vec![0.0_f32; w * h];
        for y in 10..20 {
            for x in 10..60 {
                prob[y * w + x] = 0.9;
            }
        }
        prob[90 * w + 90] = 0.95;
        let found = ProbabilityMap {
            values: &prob,
            width: w,
            height: h,
        }
        .regions(0.6);
        assert_eq!(found.len(), 1);
        assert!((found[0].bbox[0] - 0.1).abs() < 1e-6 && (found[0].bbox[2] - 0.6).abs() < 1e-6);
    }
}
