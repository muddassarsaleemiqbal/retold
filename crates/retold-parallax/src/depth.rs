//! Pinned Depth-Anything inference. No heuristic or implicit-download fallback.
use crate::Error;
use async_trait::async_trait;
use image::RgbImage;
use retold_infer::onnx::{Backend, Input, OnnxSession, TensorData};
use std::io::{Read, Write};
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub const VERSION: &str = "depth-anything-preprocess-v1";
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DepthMap {
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}
impl DepthMap {
    pub fn new(width: u32, height: u32, values: Vec<f32>) -> Result<Self, Error> {
        let result = Self {
            width,
            height,
            values,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.width == 0
            || self.height == 0
            || u64::from(self.width) * u64::from(self.height) > 16_777_216
            || self.values.len()
                != usize::try_from(u64::from(self.width) * u64::from(self.height))
                    .map_err(|e| Error::Invalid(e.to_string()))?
            || self
                .values
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(Error::Invalid(
                "invalid normalized depth dimensions or values".into(),
            ));
        }
        Ok(())
    }
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> f32 {
        self.values[y as usize * self.width as usize + x as usize]
    }
    /// Binary float storage avoids quantizing depth to a preview's eight bits.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        self.validate()?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        let mut writer = std::io::BufWriter::new(file.as_file_mut());
        writer.write_all(b"RGD1")?;
        writer.write_all(&self.width.to_le_bytes())?;
        writer.write_all(&self.height.to_le_bytes())?;
        for v in &self.values {
            writer.write_all(&v.to_le_bytes())?;
        }
        writer.flush()?;
        drop(writer);
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e| e.error)?;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, Error> {
        let mut file = std::fs::File::open(path)?;
        let mut header = [0_u8; 12];
        file.read_exact(&mut header)?;
        if &header[..4] != b"RGD1" {
            return Err(Error::Invalid("unsupported depth cache format".into()));
        }
        let width = u32::from_le_bytes(
            header[4..8]
                .try_into()
                .map_err(|e: std::array::TryFromSliceError| Error::Invalid(e.to_string()))?,
        );
        let height = u32::from_le_bytes(
            header[8..12]
                .try_into()
                .map_err(|e: std::array::TryFromSliceError| Error::Invalid(e.to_string()))?,
        );
        let n = u64::from(width) * u64::from(height);
        if n > 16_777_216 || file.metadata()?.len() != 12 + 4 * n {
            return Err(Error::Invalid("depth cache size mismatch".into()));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let values = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        Self::new(width, height, values)
    }
}
#[async_trait]
pub trait DepthEstimator: Send + Sync + std::fmt::Debug {
    fn identity(&self) -> Result<String, Error>;
    async fn estimate(&self, image: &Path, cancel: &CancellationToken) -> Result<DepthMap, Error>;
}
#[derive(Debug, Default)]
pub struct ModelDepth {
    session: OnnxSession,
}
#[async_trait]
impl DepthEstimator for ModelDepth {
    fn identity(&self) -> Result<String, Error> {
        let m = retold_infer::models::ModelStore::discover()?.manifest("depth")?;
        Ok(format!("{}:{}:{VERSION}", m.version, m.files[0].sha256))
    }
    async fn estimate(&self, image: &Path, cancel: &CancellationToken) -> Result<DepthMap, Error> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let store = retold_infer::models::ModelStore::discover()?;
        let model = store
            .verify(&store.manifest("depth")?)?
            .join("onnx/model.onnx");
        let path = image.to_owned();
        let (input, size) = tokio::task::spawn_blocking(move || preprocess(&path))
            .await
            .map_err(|e| Error::Invalid(e.to_string()))??;
        // CPU is the predictable default for this graph; S1 measured its first call faster.
        let outputs = self
            .session
            .run(&model, Backend::Cpu, vec![input], cancel)
            .await?;
        let output = outputs
            .first()
            .ok_or_else(|| Error::Invalid("depth model returned no tensor".into()))?;
        if output.shape.len() != 3 || output.shape[0] != 1 {
            return Err(Error::Invalid(
                "depth model output must be [1,height,width]".into(),
            ));
        }
        let width = u32::try_from(output.shape[2]).map_err(|e| Error::Invalid(e.to_string()))?;
        let height = u32::try_from(output.shape[1]).map_err(|e| Error::Invalid(e.to_string()))?;
        normalize_resize(width, height, &output.values, size)
    }
}
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "resized dimensions are rounded, clamped and bounded before conversion"
)]
fn input_size(width: u32, height: u32) -> Result<(u32, u32), Error> {
    if width == 0 || height == 0 {
        return Err(Error::Invalid("empty source image".into()));
    }
    let sw = 518.0 / f64::from(width);
    let sh = 518.0 / f64::from(height);
    let scale = if (1.0 - sw).abs() < (1.0 - sh).abs() {
        sw
    } else {
        sh
    };
    let w = ((f64::from(width) * scale / 14.0).round_ties_even() * 14.0).max(14.0);
    let h = ((f64::from(height) * scale / 14.0).round_ties_even() * 14.0).max(14.0);
    if w > 4096.0 || h > 4096.0 {
        return Err(Error::Invalid(
            "image aspect is too extreme for local depth inference".into(),
        ));
    }
    Ok((w as u32, h as u32))
}
fn preprocess(path: &Path) -> Result<(Input, (u32, u32)), Error> {
    let image = crate::load_image(path)?.into_rgb8();
    let size = image.dimensions();
    let (w, h) = input_size(size.0, size.1)?;
    let resized: image::ImageBuffer<image::Rgb<u8>, Vec<u8>> =
        image::imageops::resize(&image, w, h, image::imageops::FilterType::CatmullRom);
    Ok((
        Input {
            name: "pixel_values".into(),
            shape: vec![1, 3, i64::from(h), i64::from(w)],
            data: TensorData::F32(channels(&resized)),
        },
        size,
    ))
}
fn channels(image: &RgbImage) -> Vec<f32> {
    let mut values = Vec::with_capacity(image.as_raw().len());
    for (c, mean, std) in [(0, 0.485, 0.229), (1, 0.456, 0.224), (2, 0.406, 0.225)] {
        values.extend(
            image
                .pixels()
                .map(|p| (f32::from(p[c]) / 255.0 - mean) / std),
        );
    }
    values
}
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bilinear coordinates are clamped to tensor bounds and output depth to [0,1]"
)]
fn normalize_resize(
    width: u32,
    height: u32,
    values: &[f32],
    size: (u32, u32),
) -> Result<DepthMap, Error> {
    if width == 0
        || height == 0
        || values.len() != width as usize * height as usize
        || values.iter().any(|v| !v.is_finite())
    {
        return Err(Error::Invalid("malformed real depth output".into()));
    }
    let min = values.iter().copied().fold(f32::INFINITY, f32::min);
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let range = max - min;
    let mut result = Vec::with_capacity(size.0 as usize * size.1 as usize);
    for y in 0..size.1 {
        for x in 0..size.0 {
            let sx = ((f64::from(x) + 0.5) * f64::from(width) / f64::from(size.0) - 0.5)
                .clamp(0.0, f64::from(width - 1));
            let sy = ((f64::from(y) + 0.5) * f64::from(height) / f64::from(size.1) - 0.5)
                .clamp(0.0, f64::from(height - 1));
            let x0 = sx.floor() as u32;
            let y0 = sy.floor() as u32;
            let x1 = (x0 + 1).min(width - 1);
            let y1 = (y0 + 1).min(height - 1);
            let fx = sx - f64::from(x0);
            let fy = sy - f64::from(y0);
            let value = f64::from(values[(y0 * width + x0) as usize]) * (1.0 - fx) * (1.0 - fy)
                + f64::from(values[(y0 * width + x1) as usize]) * fx * (1.0 - fy)
                + f64::from(values[(y1 * width + x0) as usize]) * (1.0 - fx) * fy
                + f64::from(values[(y1 * width + x1) as usize]) * fx * fy;
            result.push(if range > f32::EPSILON {
                ((value - f64::from(min)) / f64::from(range)).clamp(0.0, 1.0) as f32
            } else {
                0.5
            });
        }
    }
    DepthMap::new(size.0, size.1, result)
}
