//! Mouth sheets: a 4×3 grid of mouth shapes and eye states on a flat magenta key.
//!
//! Row 1: rest, A/I, E, O. Row 2: U, M/B/P, F/V, L. Row 3: eyes open, half, closed;
//! the last cell is empty. Each group is cropped to one shared rectangle so every shape
//! keeps the same placement relative to the face.
use image::{Rgba, RgbaImage};

use crate::{Error, Eyes, Viseme};

pub const COLUMNS: u32 = 4;
pub const ROWS: u32 = 3;
/// The key colour the sheet prompt asks for.
pub const KEY: [u8; 3] = [255, 0, 255];

/// Layout text for the sheet prompt.
#[must_use]
pub fn layout_prompt() -> String {
    let mut cells: Vec<String> = Viseme::ALL
        .iter()
        .enumerate()
        .map(|(i, v)| format!("cell {}: mouth {}", i + 1, v.description()))
        .collect();
    cells.push("cell 9: both eyes open, looking forward".into());
    cells.push("cell 10: both eyes half closed, mid-blink".into());
    cells.push("cell 11: both eyes fully closed".into());
    cells.push("cell 12: empty".into());
    format!(
        "A mouth and eye sheet: a strict grid of 4 columns by 3 rows of equal cells, read left to right, top to bottom. {}. Every mouth cell is a close-up crop of the same mouth with a soft oval patch of the character's own skin around it (chin and cheeks), centred at the same position and size in every cell. Every eye cell is a close-up of both eyes on a horizontal band of skin with eyebrows, centred at the same position and size. Fill everything outside the skin patches, and the gaps between cells, with flat pure magenta #FF00FF. No grid lines, text, labels, numbers, shadows or other colours in the background.",
        cells.join("; ")
    )
}

#[derive(Debug, Clone)]
pub struct Sliced {
    pub mouths: Vec<(Viseme, RgbaImage)>,
    pub eyes: Vec<(Eyes, RgbaImage)>,
}
impl Sliced {
    #[must_use]
    pub fn mouth(&self, viseme: Viseme) -> Option<&RgbaImage> {
        self.mouths
            .iter()
            .find(|(v, _)| *v == viseme)
            .map(|(_, i)| i)
    }
    #[must_use]
    pub fn eyes(&self, eyes: Eyes) -> Option<&RgbaImage> {
        self.eyes.iter().find(|(e, _)| *e == eyes).map(|(_, i)| i)
    }
}

/// Alpha from distance to the key colour, with a soft edge.
#[must_use]
pub fn key_out(cell: &RgbaImage) -> RgbaImage {
    let mut out = cell.clone();
    for p in out.pixels_mut() {
        let d = p.0[..3]
            .iter()
            .zip(KEY)
            .map(|(a, b)| (f64::from(*a) - f64::from(b)).powi(2))
            .sum::<f64>()
            .sqrt();
        let alpha = ((d - 70.0) / 70.0).clamp(0.0, 1.0);
        // Remove magenta spill from edge pixels.
        if alpha < 1.0 {
            let g = p[1];
            p[0] = p[0].min(g.saturating_add(40));
            p[2] = p[2].min(g.saturating_add(40));
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "alpha is clamped to 0..=1"
        )]
        let a = (f64::from(p[3]) * alpha).round() as u8;
        p[3] = a;
    }
    out
}

fn bounds(image: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in image.enumerate_pixels() {
        if p[3] > 128 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 <= x1).then_some((x0, y0, x1 + 1, y1 + 1))
}

fn coverage(image: &RgbaImage) -> f64 {
    let opaque = image.pixels().filter(|p| p[3] > 128).count();
    #[expect(
        clippy::cast_precision_loss,
        reason = "pixel counts are far below 2^52"
    )]
    let share = opaque as f64 / (f64::from(image.width()) * f64::from(image.height())).max(1.0);
    share
}

/// Cut the grid into keyed cells and crop each group to its shared rectangle.
pub fn slice(sheet: &RgbaImage) -> Result<Sliced, Error> {
    let (w, h) = (sheet.width() / COLUMNS, sheet.height() / ROWS);
    if w < 32 || h < 32 {
        return Err(Error::Invalid("mouth sheet is too small".into()));
    }
    // Inset each cell slightly so neighbouring cells never bleed in.
    let (ix, iy) = (w / 25, h / 25);
    let cell = |i: u32| {
        let (c, r) = (i % COLUMNS, i / COLUMNS);
        key_out(
            &image::imageops::crop_imm(sheet, c * w + ix, r * h + iy, w - 2 * ix, h - 2 * iy)
                .to_image(),
        )
    };
    let group = |indices: &[u32], what: &str| -> Result<Vec<RgbaImage>, Error> {
        let cells: Vec<_> = indices.iter().map(|i| cell(*i)).collect();
        let mut union: Option<(u32, u32, u32, u32)> = None;
        let mut centres = Vec::new();
        for (i, c) in cells.iter().enumerate() {
            let share = coverage(c);
            let Some(b) = bounds(c) else {
                return Err(Error::Invalid(format!("{what} cell {} is empty", i + 1)));
            };
            if !(0.03..=0.85).contains(&share) {
                return Err(Error::Invalid(format!(
                    "{what} cell {} covers {:.0}% of its cell; expected a centred patch on magenta",
                    i + 1,
                    share * 100.0
                )));
            }
            centres.push((f64::from(b.0 + b.2) / 2.0, f64::from(b.1 + b.3) / 2.0));
            union = Some(union.map_or(b, |u| {
                (u.0.min(b.0), u.1.min(b.1), u.2.max(b.2), u.3.max(b.3))
            }));
        }
        let (cw, ch) = (f64::from(w), f64::from(h));
        let (mx, my) = (
            centres.iter().map(|c| c.0).sum::<f64>()
                / f64::from(u32::try_from(centres.len()).unwrap_or(1)),
            centres.iter().map(|c| c.1).sum::<f64>()
                / f64::from(u32::try_from(centres.len()).unwrap_or(1)),
        );
        if centres
            .iter()
            .any(|c| (c.0 - mx).abs() > cw * 0.12 || (c.1 - my).abs() > ch * 0.12)
        {
            return Err(Error::Invalid(format!(
                "{what} shapes are not centred consistently across cells"
            )));
        }
        let u = union.ok_or_else(|| Error::Invalid(format!("{what} group is empty")))?;
        Ok(cells
            .iter()
            .map(|c| image::imageops::crop_imm(c, u.0, u.1, u.2 - u.0, u.3 - u.1).to_image())
            .collect())
    };
    let mouths = group(&[0, 1, 2, 3, 4, 5, 6, 7], "mouth")?;
    let eyes = group(&[8, 9, 10], "eye")?;
    Ok(Sliced {
        mouths: Viseme::ALL.into_iter().zip(mouths).collect(),
        eyes: [Eyes::Open, Eyes::Half, Eyes::Closed]
            .into_iter()
            .zip(eyes)
            .collect(),
    })
}

/// A synthetic sheet for tests: ellipses of skin with a dark mouth/eye mark.
#[doc(hidden)]
#[must_use]
pub fn synthetic(cell: u32) -> RgbaImage {
    let mut sheet = RgbaImage::from_pixel(
        cell * COLUMNS,
        cell * ROWS,
        Rgba([KEY[0], KEY[1], KEY[2], 255]),
    );
    for i in 0..11 {
        let (cx, cy) = (
            f64::from((i % COLUMNS) * cell + cell / 2),
            f64::from((i / COLUMNS) * cell + cell / 2),
        );
        let (rx, ry) = (f64::from(cell) * 0.3, f64::from(cell) * 0.2);
        for y in 0..sheet.height() {
            for x in 0..sheet.width() {
                let (dx, dy) = ((f64::from(x) - cx) / rx, (f64::from(y) - cy) / ry);
                let r = dx * dx + dy * dy;
                if r < 1.0 {
                    let mark = r < 0.15 + f64::from(i % 8) * 0.03;
                    sheet.put_pixel(
                        x,
                        y,
                        if mark {
                            Rgba([90, 30, 30, 255])
                        } else {
                            Rgba([230, 190, 160, 255])
                        },
                    );
                }
            }
        }
    }
    sheet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_a_clean_sheet_into_aligned_groups() {
        let sliced = slice(&synthetic(120)).unwrap();
        assert_eq!(sliced.mouths.len(), 8);
        assert_eq!(sliced.eyes.len(), 3);
        let size = sliced.mouth(Viseme::Rest).unwrap().dimensions();
        assert!(sliced.mouths.iter().all(|(_, m)| m.dimensions() == size));
        // Background is transparent, skin is opaque.
        let rest = sliced.mouth(Viseme::Rest).unwrap();
        assert_eq!(rest.get_pixel(0, 0)[3], 0);
        assert_eq!(rest.get_pixel(size.0 / 2, size.1 / 2)[3], 255);
    }

    #[test]
    fn rejects_a_sheet_without_key_background() {
        let flat = RgbaImage::from_pixel(480, 360, Rgba([230, 190, 160, 255]));
        assert!(slice(&flat).is_err());
    }
}
