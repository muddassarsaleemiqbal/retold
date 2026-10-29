//! Local 2× super-resolution (`Swin2SR` lightweight) for crops and panels, run in overlapping
//! tiles so memory stays small on any image size.

use std::path::Path;

use image::RgbImage;
use tokio_util::sync::CancellationToken;

use crate::Error;
use crate::onnx::{Backend, Input, OnnxSession, TensorData};
use retold_core::num;

const TILE_SIDE: u32 = 192;
const OVERLAP: u32 = 16;

/// `img` at twice its width and height.
pub async fn upscale_x2(
    session: &OnnxSession,
    model: &Path,
    img: &RgbImage,
    cancel: &CancellationToken,
) -> Result<RgbImage, Error> {
    let (w, h) = img.dimensions();
    let mut out = RgbImage::new(w * 2, h * 2);
    let step = TILE_SIDE - 2 * OVERLAP;
    let mut y = 0;
    while y < h {
        let mut x = 0;
        while x < w {
            // Tile with an overlap margin on every side, clamped to the image.
            let (x0, y0) = (x.saturating_sub(OVERLAP), y.saturating_sub(OVERLAP));
            let (x1, y1) = ((x + step + OVERLAP).min(w), (y + step + OVERLAP).min(h));
            let tile = image::imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image();
            let big = run_tile(session, model, &tile, cancel).await?;
            // Keep only the tile's own area, dropping the overlap the model saw as context.
            let (kx, ky) = (x - x0, y - y0);
            let (kw, kh) = ((x + step).min(w) - x, (y + step).min(h) - y);
            for dy in 0..kh * 2 {
                for dx in 0..kw * 2 {
                    out.put_pixel(
                        x * 2 + dx,
                        y * 2 + dy,
                        *big.get_pixel(kx * 2 + dx, ky * 2 + dy),
                    );
                }
            }
            x += step;
        }
        y += step;
    }
    Ok(out)
}

/// The model works on sizes that are a multiple of this.
const MODEL_STRIDE: u32 = 8;
/// Colour channels in and out of the model.
const CHANNELS: usize = 3;
/// 8-bit channel maximum, for converting to and from 0..=1.
const CHANNEL_MAX: f32 = 255.0;

/// One tile through the model: padded (edge pixels repeated) to a multiple of 8 as it was
/// trained, then cropped back to exactly twice the tile.
async fn run_tile(
    session: &OnnxSession,
    model: &Path,
    tile: &RgbImage,
    cancel: &CancellationToken,
) -> Result<RgbImage, Error> {
    let (width, height) = tile.dimensions();
    let padded_width = width.div_ceil(MODEL_STRIDE) * MODEL_STRIDE;
    let padded_height = height.div_ceil(MODEL_STRIDE) * MODEL_STRIDE;
    let input = Input {
        name: "pixel_values".into(),
        shape: vec![1, 3, i64::from(padded_height), i64::from(padded_width)],
        data: TensorData::F32(planar(tile, padded_width, padded_height)),
    };
    let outputs = session
        .run(model, Backend::Cpu, vec![input], cancel)
        .await?;
    let output = outputs
        .first()
        .ok_or_else(|| Error::Invalid("upscaler returned no tensor".into()))?;
    interleaved(&output.shape, &output.values, width * 2, height * 2)
}

/// `tile` as planar RGB floats in 0..=1, padded to the given size by repeating edge pixels.
fn planar(tile: &RgbImage, padded_width: u32, padded_height: u32) -> Vec<f32> {
    let (width, height) = tile.dimensions();
    let plane = usize::try_from(u64::from(padded_width) * u64::from(padded_height)).unwrap_or(0);
    let mut values = vec![0.0_f32; plane * CHANNELS];
    for row in 0..padded_height {
        for column in 0..padded_width {
            let pixel = tile.get_pixel(column.min(width - 1), row.min(height - 1));
            let offset = usize::try_from(row * padded_width + column).unwrap_or(0);
            for (channel, value) in pixel.0.iter().enumerate() {
                values[channel * plane + offset] = f32::from(*value) / CHANNEL_MAX;
            }
        }
    }
    values
}

/// The model's planar `[1, 3, height, width]` output as an image of `width` x `height`, the
/// top-left of the (padded) output.
fn interleaved(shape: &[i64], values: &[f32], width: u32, height: u32) -> Result<RgbImage, Error> {
    let bad = |detail: String| Error::Invalid(format!("upscaler output: {detail}"));
    let &[_, _, rows, columns] = shape else {
        return Err(bad(format!("expected 4 dimensions, got {shape:?}")));
    };
    let rows = usize::try_from(rows).map_err(|error| bad(error.to_string()))?;
    let columns = usize::try_from(columns).map_err(|error| bad(error.to_string()))?;
    let (wanted_columns, wanted_rows) = (width as usize, height as usize);
    if columns < wanted_columns || rows < wanted_rows || values.len() < rows * columns * CHANNELS {
        return Err(bad(format!(
            "{columns}x{rows} is smaller than {width}x{height}"
        )));
    }
    let plane = rows * columns;
    Ok(RgbImage::from_fn(width, height, |column, row| {
        let offset = row as usize * columns + column as usize;
        let channel = |index: usize| {
            let value = values[index * plane + offset].clamp(0.0, 1.0);
            num::round_u8(f64::from(value * CHANNEL_MAX))
        };
        image::Rgb([channel(0), channel(1), channel(2)])
    }))
}
