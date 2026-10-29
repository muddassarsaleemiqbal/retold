//! Portable half-block image previews. Decode and resize on the worker; draw bounded pixels.
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
};
use serde_json::{Value, json};
use std::path::Path;
/// Images are shrunk to at most this many pixels before they reach the screen.
const PREVIEW_WIDTH: u32 = 160;
const PREVIEW_HEIGHT: u32 = 120;
const IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
const DECODE_BYTES: u64 = 128 * 1024 * 1024;
const PIXEL_BYTES: usize = 3;

pub fn decode(path: &Path) -> std::io::Result<Option<Value>> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let previewable = IMAGE_EXTENSIONS.contains(&extension.as_str());
    if !previewable {
        return Ok(Option::default());
    }
    let mut reader = image::ImageReader::open(path)?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(DECODE_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().map_err(std::io::Error::other)?;
    let pixels = decoded.thumbnail(PREVIEW_WIDTH, PREVIEW_HEIGHT).to_rgb8();
    Ok(Some(
        json!({"width": pixels.width(), "height": pixels.height(), "pixels": pixels.into_raw()}),
    ))
}

/// A decoded image's size and its RGB bytes as JSON numbers.
struct Image<'a> {
    width: usize,
    height: usize,
    pixels: &'a [Value],
}

fn dimension(value: &Value) -> usize {
    value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_default()
}

fn image(value: &Value) -> Option<Image<'_>> {
    let picture = Image {
        width: dimension(&value["width"]),
        height: dimension(&value["height"]),
        pixels: value["pixels"].as_array()?,
    };
    let empty = picture.width == 0 || picture.height == 0;
    (!empty).then_some(picture)
}

/// The image in half-block cells: each cell shows two pixels, top as text and bottom as fill.
pub fn lines(value: &Value, area: Rect) -> Vec<Line<'static>> {
    let Some(picture) = image(value) else {
        return Vec::new();
    };
    let columns = usize::from(area.width).min(picture.width);
    let rows = usize::from(area.height).min(picture.height.div_ceil(2));
    let grid = Grid { columns, rows };
    (0..rows).map(|row| cell_row(&picture, grid, row)).collect()
}

#[derive(Clone, Copy)]
struct Grid {
    columns: usize,
    rows: usize,
}

fn cell_row(picture: &Image<'_>, grid: Grid, row: usize) -> Line<'static> {
    let Image {
        width,
        height,
        pixels,
    } = *picture;
    let half_rows = grid.rows.saturating_mul(2);
    let top = row.saturating_mul(2).saturating_mul(height) / half_rows;
    let lower = row
        .saturating_mul(2)
        .saturating_add(1)
        .saturating_mul(height)
        / half_rows;
    let bottom = lower.min(height.saturating_sub(1));
    let spans: Vec<Span<'static>> = (0..grid.columns)
        .map(|column| {
            let x = column.saturating_mul(width) / grid.columns;
            let style = Style::default()
                .fg(pixel(pixels, top * width + x))
                .bg(pixel(pixels, bottom * width + x));
            Span::styled("▀", style)
        })
        .collect();
    Line::from(spans)
}

fn pixel(pixels: &[Value], index: usize) -> Color {
    let component = |offset| {
        pixels
            .get(index * PIXEL_BYTES + offset)
            .and_then(Value::as_u64)
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or_default()
    };
    Color::Rgb(component(0), component(1), component(2))
}
