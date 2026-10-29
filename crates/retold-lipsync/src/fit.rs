//! Fitting a sheet sprite onto a face: similarity transform, skin colour match and a
//! feathered oval edge so the patch blends into the artwork.
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

/// A feature's position, size and roll in pixels (angle in radians).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub angle: f64,
}

/// A sprite ready to paste into the artwork at `x`, `y`.
#[derive(Debug, Clone)]
pub struct Placed {
    pub image: RgbaImage,
    pub x: i32,
    pub y: i32,
}

fn sample(image: &RgbaImage, x: f64, y: f64) -> [f64; 4] {
    if x < 0.0 || y < 0.0 || x > f64::from(image.width() - 1) || y > f64::from(image.height() - 1) {
        return [0.0; 4];
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bounds checked above"
    )]
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = (
        (x0 + 1).min(image.width() - 1),
        (y0 + 1).min(image.height() - 1),
    );
    let (fx, fy) = (x - f64::from(x0), y - f64::from(y0));
    let mut out = [0.0; 4];
    for (px, py, w) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x1, y0, fx * (1.0 - fy)),
        (x0, y1, (1.0 - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        let p = image.get_pixel(px, py);
        let a = f64::from(p[3]) / 255.0;
        for c in 0..3 {
            out[c] += f64::from(p[c]) * a * w;
        }
        out[3] += a * w;
    }
    let alpha = out[3];
    if alpha > 0.0 {
        for value in out.iter_mut().take(3) {
            *value /= alpha;
        }
    }
    out
}

/// Keep eyelids inside two feathered eye openings, preserving the artwork's eyebrows
/// and the outer frame of glasses rather than pasting a whole new facial band.
#[must_use]
pub fn eye_patch(
    sprite: &RgbaImage,
    eyes: Anchor,
    frame: &RgbaImage,
    origin: (i32, i32),
) -> RgbaImage {
    let mut out = sprite.clone();
    if !eyes.width.is_finite() || eyes.width <= 0.0 {
        return RgbaImage::new(sprite.width(), sprite.height());
    }
    let source = median_colour(
        &sprite
            .pixels()
            .filter(|p| p[3] > 200)
            .map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])])
            .collect::<Vec<_>>(),
    );
    let planes = [-0.5, 0.5].map(|side| eye_skin(frame, origin, eyes, side));
    let (sin, cos) = eyes.angle.sin_cos();
    for (x, y, pixel) in out.enumerate_pixels_mut() {
        let (dx, dy) = (f64::from(x) - eyes.x, f64::from(y) - eyes.y);
        let (along, across) = (dx * cos + dy * sin, -dx * sin + dy * cos);
        let radius = [-0.5, 0.5]
            .into_iter()
            .map(|side| ((along / eyes.width - side) / 0.48).hypot(across / (eyes.width * 0.28)))
            .fold(f64::INFINITY, f64::min);
        let opacity = ((1.0 - radius) / 0.15).clamp(0.0, 1.0);
        if let Some(source) = source {
            let under = sample(
                frame,
                f64::from(origin.0) + eyes.x + along * cos
                    - (eyes.width * 0.35 + across * 0.25) * sin,
                f64::from(origin.1)
                    + eyes.y
                    + along * sin
                    + (eyes.width * 0.35 + across * 0.25) * cos,
            );
            let side = usize::from(along >= 0.0);
            let target = if let Some(plane) = planes[side] {
                let nx = (along / eyes.width - if side == 0 { -0.5 } else { 0.5 }) / 0.33;
                let ny = across / (eyes.width * 0.23);
                let background = plane.at(nx, ny);
                let below = plane.at(nx, (0.35 + across / eyes.width * 0.25) / 0.23);
                std::array::from_fn(|c| {
                    background[c]
                        + if (under[c] - below[c]).abs() < 15.0 {
                            (under[c] - below[c]) * 0.35
                        } else {
                            0.0
                        }
                })
            } else {
                source
            };
            let distance = (luminance([
                f64::from(pixel[0]),
                f64::from(pixel[1]),
                f64::from(pixel[2]),
            ]) - luminance(source))
            .abs();
            let ink =
                ((distance - 28.0) / 35.0).clamp(0.0, 1.0) * 0.8 * (f64::from(pixel[3]) / 255.0);
            for c in 0..3 {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "colours are clamped to u8"
                )]
                {
                    pixel[c] = (f64::from(pixel[c]) * ink + target[c] * (1.0 - ink))
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
            }
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "opacity is clamped to 0-1"
        )]
        {
            pixel[3] = (255.0 * opacity).round() as u8;
        }
    }
    out
}

fn median_colour(values: &[[f64; 3]]) -> Option<[f64; 3]> {
    if values.is_empty() {
        return None;
    }
    Some(std::array::from_fn(|c| {
        let mut channel: Vec<_> = values.iter().map(|p| p[c]).collect();
        channel.sort_by(f64::total_cmp);
        channel[channel.len() / 2]
    }))
}

fn luminance(rgb: [f64; 3]) -> f64 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}

#[derive(Clone, Copy)]
struct SkinPlane {
    base: [f64; 3],
    horizontal: [f64; 3],
    vertical: [f64; 3],
}
impl SkinPlane {
    fn at(self, x: f64, y: f64) -> [f64; 3] {
        std::array::from_fn(|c| self.base[c] + self.horizontal[c] * x + self.vertical[c] * y)
    }
}

fn eye_skin(frame: &RgbaImage, origin: (i32, i32), eyes: Anchor, side: f64) -> Option<SkinPlane> {
    let (sin, cos) = eyes.angle.sin_cos();
    let samples: Vec<_> = (0..24)
        .filter_map(|i| {
            let angle = f64::from(i) * std::f64::consts::TAU / 24.0;
            let (x, y) = (angle.cos(), angle.sin());
            let (along, across) = ((side + x * 0.33) * eyes.width, y * eyes.width * 0.23);
            let p = sample(
                frame,
                f64::from(origin.0) + eyes.x + along * cos - across * sin,
                f64::from(origin.1) + eyes.y + along * sin + across * cos,
            );
            (p[3] > 0.5).then_some((x, y, [p[0], p[1], p[2]]))
        })
        .collect();
    let median = median_colour(&samples.iter().map(|s| s.2).collect::<Vec<_>>())?;
    let clean: Vec<_> = samples
        .into_iter()
        .filter(|s| (0..3).map(|c| (s.2[c] - median[c]).powi(2)).sum::<f64>() < 900.0)
        .collect();
    let count = f64::from(u32::try_from(clean.len()).ok()?);
    if count < 4.0 {
        return None;
    }
    let (mx, my) = (
        clean.iter().map(|s| s.0).sum::<f64>() / count,
        clean.iter().map(|s| s.1).sum::<f64>() / count,
    );
    let mean: [f64; 3] = std::array::from_fn(|c| clean.iter().map(|s| s.2[c]).sum::<f64>() / count);
    let variance_x = clean
        .iter()
        .map(|s| (s.0 - mx).powi(2))
        .sum::<f64>()
        .max(0.01);
    let variance_y = clean
        .iter()
        .map(|s| (s.1 - my).powi(2))
        .sum::<f64>()
        .max(0.01);
    let horizontal: [f64; 3] = std::array::from_fn(|c| {
        (clean
            .iter()
            .map(|s| (s.0 - mx) * (s.2[c] - mean[c]))
            .sum::<f64>()
            / variance_x)
            .clamp(-15.0, 15.0)
    });
    let vertical: [f64; 3] = std::array::from_fn(|c| {
        (clean
            .iter()
            .map(|s| (s.1 - my) * (s.2[c] - mean[c]))
            .sum::<f64>()
            / variance_y)
            .clamp(-15.0, 15.0)
    });
    Some(SkinPlane {
        base: std::array::from_fn(|c| mean[c] - horizontal[c] * mx - vertical[c] * my),
        horizontal,
        vertical,
    })
}

fn mouth_skin(
    frame: &RgbaImage,
    origin: (i32, i32),
    mouth: Anchor,
    along: f64,
    across: f64,
) -> [f64; 3] {
    let (sin, cos) = mouth.angle.sin_cos();
    let under = |horizontal: f64| {
        sample(
            frame,
            f64::from(origin.0) + mouth.x + horizontal * cos - across * sin,
            f64::from(origin.1) + mouth.y + horizontal * sin + across * cos,
        )
    };
    let (left, right) = (under(-mouth.width * 0.62), under(mouth.width * 0.62));
    let t = (along / (mouth.width * 1.24) + 0.5).clamp(0.0, 1.0);
    std::array::from_fn(|c| left[c] * (1.0 - t) + right[c] * t)
}

/// Keep the connected articulation, excluding detached chin marks and the sheet's
/// skin border. Reconstruct only the old mouth area from neighbouring cheek skin.
#[must_use]
pub fn mouth_patch(
    sprite: &RgbaImage,
    mouth: Anchor,
    frame: &RgbaImage,
    origin: (i32, i32),
) -> RgbaImage {
    use std::collections::{BTreeSet, VecDeque};
    let (w, h) = (f64::from(sprite.width()), f64::from(sprite.height()));
    let colours: Vec<_> = sprite
        .enumerate_pixels()
        .filter(|(x, y, p)| {
            let r = ((f64::from(*x) - w / 2.0) / (w / 2.0))
                .hypot((f64::from(*y) - h / 2.0) / (h / 2.0));
            p[3] > 200 && (0.6..0.9).contains(&r)
        })
        .map(|(_, _, p)| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])])
        .collect();
    let Some(complexion) = median_colour(&colours) else {
        return RgbaImage::new(sprite.width(), sprite.height());
    };
    let ink = |x: u32, y: u32| {
        let p = sprite.get_pixel(x, y);
        let value = luminance([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
        p[3] > 128
            && (value - luminance(complexion)).abs() > 28.0
            && (f64::from(x) - mouth.x).abs() < mouth.width * 0.7
            && (mouth.y - mouth.width * 0.4..mouth.y + mouth.width * 0.9).contains(&f64::from(y))
    };
    let seed = sprite
        .enumerate_pixels()
        .filter(|(x, y, _)| ink(*x, *y))
        .filter(|(x, y, _)| {
            (f64::from(*x) - mouth.x).abs() < mouth.width * 0.25
                && (f64::from(*y) - mouth.y).abs() < mouth.width * 0.25
        })
        .min_by(|a, b| {
            let distance = |x: u32, y: u32| (f64::from(x) - mouth.x).hypot(f64::from(y) - mouth.y);
            distance(a.0, a.1).total_cmp(&distance(b.0, b.1))
        })
        .map(|(x, y, _)| (x, y));
    let Some(seed) = seed else {
        return RgbaImage::new(sprite.width(), sprite.height());
    };
    let mut queue = VecDeque::from([seed]);
    let mut visited = BTreeSet::from([seed]);
    let (mut top, mut bottom) = (mouth.y, mouth.y);
    let mut mask = RgbaImage::new(sprite.width(), sprite.height());
    while let Some((x, y)) = queue.pop_front() {
        top = top.min(f64::from(y));
        bottom = bottom.max(f64::from(y));
        mask.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        for nx in x.saturating_sub(1)..=(x + 1).min(sprite.width() - 1) {
            for ny in y.saturating_sub(1)..=(y + 1).min(sprite.height() - 1) {
                if !visited.contains(&(nx, ny)) && ink(nx, ny) {
                    visited.insert((nx, ny));
                    queue.push_back((nx, ny));
                }
            }
        }
    }
    let mask = image::imageops::blur(&mask, 0.6);
    // Avoid a sheet's exaggerated opening swallowing the chin in a fixed-jaw illustration.
    let stretch = ((bottom - top) / (mouth.width * 0.7)).max(1.0);
    blend_mouth(sprite, &mask, mouth, frame, origin, stretch)
}

fn blend_mouth(
    sprite: &RgbaImage,
    mask: &RgbaImage,
    mouth: Anchor,
    frame: &RgbaImage,
    origin: (i32, i32),
    stretch: f64,
) -> RgbaImage {
    let mut out = RgbaImage::new(sprite.width(), sprite.height());
    let (sin, cos) = mouth.angle.sin_cos();
    for (x, y, p) in out.enumerate_pixels_mut() {
        let (dx, dy) = (f64::from(x) - mouth.x, f64::from(y) - mouth.y);
        let (along, across) = (dx * cos + dy * sin, -dx * sin + dy * cos);
        let (sx, sy) = (
            mouth.x + along * cos - across * stretch * sin,
            mouth.y + along * sin + across * stretch * cos,
        );
        let original = sample(sprite, sx, sy);
        let coverage = sample(mask, sx, sy)[3];
        let rx = (along.abs() - mouth.width * 0.58).max(0.0);
        let ry = (-mouth.width * 0.22 - across)
            .max(0.0)
            .max(across - mouth.width * 0.34);
        let erase = (1.0 - rx.hypot(ry) / (mouth.width * 0.12).max(1.0)).clamp(0.0, 1.0);
        let target = mouth_skin(
            frame,
            origin,
            mouth,
            along,
            across.clamp(-mouth.width * 0.25, mouth.width * 0.3),
        );
        let strength = coverage * 0.75;
        for c in 0..3 {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "colours are clamped to u8"
            )]
            {
                p[c] = (original[c] * strength + target[c] * (1.0 - strength))
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "opacity is clamped to 0-1"
        )]
        {
            p[3] = (original[3] * coverage.max(erase) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Map `sprite` (whose feature is at `from`, in sprite pixels) onto `to` in `frame`.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one resampling pass: transform, feather and skin match"
)]
pub fn place(sprite: &RgbaImage, from: Anchor, to: Anchor, frame: &RgbaImage) -> Option<Placed> {
    if from.width <= 0.0 || to.width <= 0.0 || sprite.width() < 2 || sprite.height() < 2 {
        return None;
    }
    let scale = to.width / from.width;
    // Filter before reducing: point/bilinear sampling alone skips thin contours
    // on large generated sheets and leaves jagged eyelids and tooth edges.
    if scale.is_finite() && scale > 0.0 && scale < 0.75 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "positive dimensions, shrinking an existing u32 image"
        )]
        let (width, height) = (
            (f64::from(sprite.width()) * scale).round().max(2.0) as u32,
            (f64::from(sprite.height()) * scale).round().max(2.0) as u32,
        );
        if width < sprite.width() && height < sprite.height() {
            let filtered = image::imageops::resize(
                sprite,
                width,
                height,
                image::imageops::FilterType::Lanczos3,
            );
            let sx = f64::from(width) / f64::from(sprite.width());
            let sy = f64::from(height) / f64::from(sprite.height());
            return place(
                &filtered,
                Anchor {
                    x: from.x * sx,
                    y: from.y * sy,
                    width: from.width * sx,
                    ..from
                },
                to,
                frame,
            );
        }
    }
    let (sin, cos) = (to.angle - from.angle).sin_cos();
    let (sw, sh) = (f64::from(sprite.width()), f64::from(sprite.height()));
    // Bounding box of the transformed sprite corners.
    let corner = |x: f64, y: f64| {
        let (dx, dy) = ((x - from.x) * scale, (y - from.y) * scale);
        (to.x + dx * cos - dy * sin, to.y + dx * sin + dy * cos)
    };
    let corners = [
        corner(0.0, 0.0),
        corner(sw, 0.0),
        corner(0.0, sh),
        corner(sw, sh),
    ];
    let min_x = corners
        .iter()
        .map(|c| c.0)
        .fold(f64::INFINITY, f64::min)
        .floor();
    let min_y = corners
        .iter()
        .map(|c| c.1)
        .fold(f64::INFINITY, f64::min)
        .floor();
    let max_x = corners
        .iter()
        .map(|c| c.0)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil();
    let max_y = corners
        .iter()
        .map(|c| c.1)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil();
    let (w, h) = (max_x - min_x, max_y - min_y);
    if !(1.0..=4096.0).contains(&w) || !(1.0..=4096.0).contains(&h) {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "size bounded above"
    )]
    let mut out = RgbaImage::new(w as u32, h as u32);
    // Ellipse inscribed in the sprite: feather its rim, and use the outer ring as skin.
    let (cx, cy, rx, ry) = (sw / 2.0, sh / 2.0, sw / 2.0, sh / 2.0);
    let mut ring: Vec<([f64; 3], [f64; 3])> = Vec::new();
    let mut pixels = Vec::with_capacity((out.width() * out.height()) as usize);
    for y in 0..out.height() {
        for x in 0..out.width() {
            let (fx, fy) = (min_x + f64::from(x) + 0.5, min_y + f64::from(y) + 0.5);
            let (dx, dy) = (fx - to.x, fy - to.y);
            // Inverse rotation and scale back into sprite space.
            let (sx, sy) = (
                from.x + (dx * cos + dy * sin) / scale,
                from.y + (-dx * sin + dy * cos) / scale,
            );
            let mut p = sample(sprite, sx - 0.5, sy - 0.5);
            let r = ((sx - cx) / rx).hypot((sy - cy) / ry);
            p[3] *= ((1.0 - r) / 0.35).clamp(0.0, 1.0);
            if p[3] > 0.6 && r > 0.6 {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "checked against frame size"
                )]
                let under = (fx >= 0.0
                    && fy >= 0.0
                    && fx < f64::from(frame.width())
                    && fy < f64::from(frame.height()))
                .then(|| frame.get_pixel(fx as u32, fy as u32));
                if let Some(under) = under {
                    ring.push((
                        [p[0], p[1], p[2]],
                        [
                            f64::from(under[0]),
                            f64::from(under[1]),
                            f64::from(under[2]),
                        ],
                    ));
                }
            }
            pixels.push(p);
        }
    }
    // Shift the sprite so its skin ring matches the artwork beneath it.
    let shift: [f64; 3] = std::array::from_fn(|channel| {
        if ring.is_empty() {
            return 0.0;
        }
        let mut source: Vec<_> = ring.iter().map(|p| p.0[channel]).collect();
        let mut target: Vec<_> = ring.iter().map(|p| p.1[channel]).collect();
        source.sort_by(f64::total_cmp);
        target.sort_by(f64::total_cmp);
        (target[target.len() / 2] - source[source.len() / 2]).clamp(-60.0, 60.0)
    });
    for (p, out) in pixels.iter().zip(out.pixels_mut()) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "values are clamped to u8"
        )]
        let px = Rgba([
            (p[0] + shift[0]).round().clamp(0.0, 255.0) as u8,
            (p[1] + shift[1]).round().clamp(0.0, 255.0) as u8,
            (p[2] + shift[2]).round().clamp(0.0, 255.0) as u8,
            (p[3] * 255.0).round().clamp(0.0, 255.0) as u8,
        ]);
        *out = px;
    }
    #[expect(clippy::cast_possible_truncation, reason = "frame coordinates fit i32")]
    Some(Placed {
        image: out,
        x: min_x as i32,
        y: min_y as i32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reducing_a_sheet_preserves_thin_contours_between_sample_centres() {
        let mut sprite = RgbaImage::from_pixel(100, 100, Rgba([200, 150, 100, 255]));
        for x in 20..80 {
            sprite.put_pixel(x, 48, Rgba([10, 5, 0, 255]));
        }
        let frame = RgbaImage::from_pixel(100, 100, Rgba([200, 150, 100, 255]));
        let placed = place(
            &sprite,
            Anchor {
                x: 50.0,
                y: 50.0,
                width: 100.0,
                angle: 0.0,
            },
            Anchor {
                x: 50.0,
                y: 50.0,
                width: 20.0,
                angle: 0.0,
            },
            &frame,
        )
        .unwrap();
        assert!(
            placed.image.pixels().any(|p| p[0] < 190),
            "thin contour must survive 5x reduction"
        );
    }

    #[test]
    fn mouth_mask_removes_detached_chin_marks_and_erases_the_old_lip_corners() {
        let mut sprite = RgbaImage::from_pixel(100, 80, Rgba([210, 140, 90, 255]));
        for y in 25..45 {
            for x in 35..65 {
                sprite.put_pixel(x, y, Rgba([35, 15, 10, 255]));
            }
        }
        for x in 40..60 {
            sprite.put_pixel(x, 60, Rgba([35, 15, 10, 255]));
        }
        let mut frame = RgbaImage::from_pixel(200, 200, Rgba([190, 120, 70, 255]));
        for x in 75..125 {
            for y in 78..84 {
                frame.put_pixel(x, y, Rgba([80, 30, 20, 255]));
            }
        }
        let mouth = Anchor {
            x: 50.0,
            y: 30.0,
            width: 50.0,
            angle: 0.0,
        };
        let patch = mouth_patch(&sprite, mouth, &frame, (50, 50));
        assert_eq!(
            patch.get_pixel(50, 60)[3],
            0,
            "detached chin line must not be pasted"
        );
        image::imageops::overlay(&mut frame, &patch, 50, 50);
        assert_eq!(
            frame.get_pixel(77, 80).0,
            [190, 120, 70, 255],
            "old closed-mouth corner must be erased with matching skin"
        );
        assert!(
            frame.get_pixel(100, 80)[0] < 80,
            "open-mouth ink must survive"
        );
    }

    #[test]
    fn blink_erases_the_original_outer_lashes_inside_the_feature_region() {
        let sprite = RgbaImage::from_pixel(200, 120, Rgba([150, 100, 70, 255]));
        let mut frame = sprite.clone();
        frame.put_pixel(32, 52, Rgba([20, 10, 5, 255]));
        let patch = eye_patch(
            &sprite,
            Anchor {
                x: 100.0,
                y: 60.0,
                width: 80.0,
                angle: 0.0,
            },
            &frame,
            (0, 0),
        );
        image::imageops::overlay(&mut frame, &patch, 0, 0);
        assert_eq!(frame.get_pixel(32, 52).0, [150, 100, 70, 255]);
    }

    #[test]
    fn blink_background_does_not_copy_a_dark_glasses_rim_into_the_lens() {
        let mut sprite = RgbaImage::from_pixel(200, 100, Rgba([150, 100, 70, 255]));
        sprite.get_pixel_mut(60, 60)[3] = 80;
        let mut frame = RgbaImage::from_pixel(200, 100, Rgba([150, 100, 70, 255]));
        for x in 30..170 {
            frame.put_pixel(x, 88, Rgba([20, 10, 5, 255]));
        }
        let mask = eye_patch(
            &sprite,
            Anchor {
                x: 100.0,
                y: 60.0,
                width: 80.0,
                angle: 0.0,
            },
            &frame,
            (0, 0),
        );
        assert_eq!(mask.get_pixel(60, 60).0, [150, 100, 70, 255]);
        assert_eq!(mask.get_pixel(140, 60).0, [150, 100, 70, 255]);
    }

    #[test]
    fn blink_mask_preserves_brows_and_glasses_bridge() {
        let sprite = RgbaImage::from_pixel(200, 100, Rgba([150, 100, 70, 255]));
        let mask = eye_patch(
            &sprite,
            Anchor {
                x: 100.0,
                y: 60.0,
                width: 80.0,
                angle: 0.0,
            },
            &sprite,
            (0, 0),
        );
        assert_eq!(
            mask.get_pixel(60, 30)[3],
            0,
            "eyebrow must remain the original artwork"
        );
        assert_eq!(
            mask.get_pixel(100, 60)[3],
            0,
            "glasses bridge must remain the original artwork"
        );
        assert_eq!(mask.get_pixel(60, 60)[3], 255);
        assert_eq!(mask.get_pixel(140, 60)[3], 255);
    }

    #[test]
    fn scales_rotates_and_matches_skin() {
        let sprite = RgbaImage::from_pixel(40, 20, Rgba([200, 150, 120, 255]));
        let frame = RgbaImage::from_pixel(200, 200, Rgba([220, 170, 140, 255]));
        let from = Anchor {
            x: 20.0,
            y: 10.0,
            width: 40.0,
            angle: 0.0,
        };
        let to = Anchor {
            x: 100.0,
            y: 120.0,
            width: 80.0,
            angle: 0.0,
        };
        let placed = place(&sprite, from, to, &frame).unwrap();
        assert_eq!((placed.x, placed.y), (60, 100));
        assert_eq!(placed.image.dimensions(), (80, 40));
        let centre = placed.image.get_pixel(40, 20);
        assert_eq!(centre[3], 255);
        // Skin shifted +20 per channel to match the frame.
        assert_eq!(&centre.0[..3], &[220, 170, 140]);
        // Feathered corners.
        assert_eq!(placed.image.get_pixel(0, 0)[3], 0);
        let rotated = place(
            &sprite,
            from,
            Anchor {
                angle: std::f64::consts::FRAC_PI_2,
                ..to
            },
            &frame,
        )
        .unwrap();
        assert_eq!(rotated.image.dimensions(), (40, 80));
    }
}
