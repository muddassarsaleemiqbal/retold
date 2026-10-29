//! Deterministic visual quality inspection algorithms.
//!
//! Provides:
//! - CIEDE2000 perceptual color difference ($\Delta E_{00}$) for frozen/duplicate frame detection.
//! - Sobel spatial edge density and Laplacian variance for sharpness/blur detection.
//! - Rec. 709 luminance clipping analysis for blown highlights and crushed shadows.

use image::RgbImage;
use retold_core::num;
use serde::{Deserialize, Serialize};

/// Perceptual color representation in CIE $L^*a^*b^*$ color space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

/// Comparison result between two consecutive video keyframes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameComparison {
    /// Average CIEDE2000 perceptual color difference across sample regions.
    pub mean_delta_e: f64,
    /// Maximum CIEDE2000 perceptual color difference found in any region.
    pub max_delta_e: f64,
    /// `true` if $\Delta E_{00} < 1.0$, indicating imperceptible change (duplicate or frozen shot).
    pub is_duplicate: bool,
}

/// Blur and spatial sharpness metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlurReport {
    /// Variance of the Laplacian operator (higher = sharper, lower = blurry).
    pub variance_of_laplacian: f64,
    /// Fraction of pixels with significant high-frequency edge gradients.
    pub edge_density: f64,
    /// `true` if the image fails sharpness thresholds (e.g. out-of-focus or AI smearing).
    pub is_blurry: bool,
}

/// Exposure and luminance clipping metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LuminanceReport {
    /// Average image luminance normalized to [0.0, 1.0].
    pub mean_luminance: f64,
    /// Fraction of pixels with blown highlights ($Y > 0.98$).
    pub blown_highlights_fraction: f64,
    /// Fraction of pixels with crushed shadows ($Y < 0.02$).
    pub crushed_shadows_fraction: f64,
    /// `true` if severe highlight blowout or shadow crushing covers more than 50% of the image.
    pub is_clipped: bool,
}

/// Convert 8-bit sRGB color to CIE $L^*a^*b^*$ under D65 standard illuminant.
#[must_use]
#[expect(
    clippy::many_single_char_names,
    reason = "names follow the CIE colour formulas"
)]
pub fn srgb_to_lab(r: u8, g: u8, b: u8) -> Lab {
    // 1. Gamma expansion to linear sRGB
    let expand = |c: u8| -> f64 {
        let v = f64::from(c) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let rl = expand(r);
    let gl = expand(g);
    let bl = expand(b);

    // 2. Linear sRGB to XYZ (D65 illuminant, 2-degree observer)
    let x = (rl * 0.412_456_4) + (gl * 0.357_576_1) + (bl * 0.180_437_5);
    let y = (rl * 0.212_672_9) + (gl * 0.715_152_2) + (bl * 0.072_175_0);
    let z = (rl * 0.019_333_9) + (gl * 0.119_192_0) + (bl * 0.950_304_1);

    // 3. Normalize by D65 reference white
    let x_n = 0.950_47;
    let y_n = 1.000_00;
    let z_n = 1.088_83;

    let f = |t: f64| -> f64 {
        let delta = 6.0 / 29.0;
        if t > delta * delta * delta {
            t.cbrt()
        } else {
            (t / (3.0 * delta * delta)) + (4.0 / 29.0)
        }
    };

    let fx = f(x / x_n);
    let fy = f(y / y_n);
    let fz = f(z / z_n);

    Lab {
        l: (116.0 * fy) - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// Samples per side when comparing two frames on a grid.
const COMPARE_GRID: u32 = 16;

/// The pixel at grid cell (`gx`, `gy`) of a `COMPARE_GRID` x `COMPARE_GRID` grid over `image`.
fn grid_pixel(image: &RgbImage, gx: u32, gy: u32) -> image::Rgb<u8> {
    let (width, height) = image.dimensions();
    let column = (gx * (width / COMPARE_GRID).max(1)).min(width - 1);
    let row = (gy * (height / COMPARE_GRID).max(1)).min(height - 1);
    *image.get_pixel(column, row)
}

/// Calculate the exact CIEDE2000 color difference ($\Delta E_{00}$) between two colors.
///
/// Implements standard ISO/CIE 11664-6:2014 / Sharma et al. (2005).
#[must_use]
#[expect(
    clippy::similar_names,
    reason = "names follow the CIEDE2000 paper's notation"
)]
pub fn ciede2000(c1: Lab, c2: Lab) -> f64 {
    let c1_val = c1.a.hypot(c1.b);
    let c2_val = c2.a.hypot(c2.b);
    let c_bar = f64::midpoint(c1_val, c2_val);

    let c_bar_7 = c_bar.powi(7);
    let g = 0.5 * (1.0 - (c_bar_7 / (c_bar_7 + 25.0_f64.powi(7))).sqrt());

    let a1_prime = (1.0 + g) * c1.a;
    let a2_prime = (1.0 + g) * c2.a;

    let c1_prime = a1_prime.hypot(c1.b);
    let c2_prime = a2_prime.hypot(c2.b);
    let c_bar_prime = f64::midpoint(c1_prime, c2_prime);

    let h1_prime = compute_hue_angle(a1_prime, c1.b);
    let h2_prime = compute_hue_angle(a2_prime, c2.b);

    let (delta_h_prime, h_bar_prime) =
        compute_delta_and_mean_hue(c1_prime, c2_prime, h1_prime, h2_prime);
    let delta_cap_h_prime =
        2.0 * (c1_prime * c2_prime).sqrt() * (delta_h_prime.to_radians() / 2.0).sin();

    let l_bar = f64::midpoint(c1.l, c2.l);
    let s_l = 1.0 + ((0.015 * (l_bar - 50.0).powi(2)) / (20.0 + (l_bar - 50.0).powi(2)).sqrt());
    let s_c = 1.0 + (0.045 * c_bar_prime);
    let t = 1.0 - (0.17 * (h_bar_prime - 30.0).to_radians().cos())
        + (0.24 * (2.0 * h_bar_prime).to_radians().cos())
        + (0.32 * ((3.0 * h_bar_prime) + 6.0).to_radians().cos())
        - (0.20 * ((4.0 * h_bar_prime) - 63.0).to_radians().cos());
    let s_h = 1.0 + (0.015 * c_bar_prime * t);

    let delta_theta = 30.0 * (-((h_bar_prime - 275.0) / 25.0).powi(2)).exp();
    let c_bar_prime_7 = c_bar_prime.powi(7);
    let r_c = 2.0 * (c_bar_prime_7 / (c_bar_prime_7 + 25.0_f64.powi(7))).sqrt();
    let r_t = -r_c * (2.0 * delta_theta.to_radians()).sin();

    let delta_l_prime = c2.l - c1.l;
    let term_l = delta_l_prime / s_l;
    let delta_c_prime = c2_prime - c1_prime;
    let term_c = delta_c_prime / s_c;
    let term_h = delta_cap_h_prime / s_h;

    (term_l.powi(2) + term_c.powi(2) + term_h.powi(2) + (r_t * term_c * term_h)).sqrt()
}

fn compute_hue_angle(a: f64, b: f64) -> f64 {
    if a.abs() < 1e-6 && b.abs() < 1e-6 {
        return 0.0;
    }
    let deg = b.atan2(a).to_degrees();
    if deg < 0.0 { deg + 360.0 } else { deg }
}

fn compute_delta_and_mean_hue(c1: f64, c2: f64, h1: f64, h2: f64) -> (f64, f64) {
    if c1.abs() < 1e-6 || c2.abs() < 1e-6 {
        return (0.0, h1 + h2);
    }
    let diff = h2 - h1;
    let delta = if diff.abs() <= 180.0 {
        diff
    } else if diff > 180.0 {
        diff - 360.0
    } else {
        diff + 360.0
    };

    let mean = if (h1 - h2).abs() <= 180.0 {
        f64::midpoint(h1, h2)
    } else if h1 + h2 < 360.0 {
        (h1 + h2 + 360.0) / 2.0
    } else {
        (h1 + h2 - 360.0) / 2.0
    };

    (delta, mean)
}

/// Compare two images for frozen/duplicate frames using CIEDE2000 across a grid of samples.
#[must_use]
pub fn compare_frames(img1: &RgbImage, img2: &RgbImage) -> FrameComparison {
    let (w1, h1) = img1.dimensions();
    let (w2, h2) = img2.dimensions();
    if w1 == 0 || h1 == 0 || w2 == 0 || h2 == 0 {
        return FrameComparison {
            mean_delta_e: 0.0,
            max_delta_e: 0.0,
            is_duplicate: true,
        };
    }

    let mut total_delta = 0.0;
    let mut max_delta = 0.0_f64;
    let mut count = 0_usize;

    for gy in 0..COMPARE_GRID {
        for gx in 0..COMPARE_GRID {
            let p1 = grid_pixel(img1, gx, gy);
            let p2 = grid_pixel(img2, gx, gy);

            let lab1 = srgb_to_lab(p1[0], p1[1], p1[2]);
            let lab2 = srgb_to_lab(p2[0], p2[1], p2[2]);

            let de = ciede2000(lab1, lab2);
            total_delta += de;
            if de > max_delta {
                max_delta = de;
            }
            count += 1;
        }
    }

    let mean_delta = if count == 0 {
        0.0
    } else {
        total_delta / num::count_f64(count)
    };
    FrameComparison {
        mean_delta_e: mean_delta,
        max_delta_e: max_delta,
        is_duplicate: mean_delta < 1.0,
    }
}

/// Detect blur and measure spatial edge density using Laplacian variance and Sobel kernels.
#[must_use]
pub fn detect_blur(img: &RgbImage, blur_threshold: f64) -> BlurReport {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return BlurReport {
            variance_of_laplacian: 0.0,
            edge_density: 0.0,
            is_blurry: true,
        };
    }

    let mut gray = vec![0.0_f64; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            // Rec. 709 luma
            let luma = (0.2126 * f64::from(p[0]))
                + (0.7152 * f64::from(p[1]))
                + (0.0722 * f64::from(p[2]));
            gray[(y * w + x) as usize] = luma;
        }
    }

    let mut laplacians = Vec::with_capacity(((w - 2) * (h - 2)) as usize);
    let mut edge_count = 0_usize;

    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let idx = (y * w + x) as usize;
            let center = gray[idx];
            let top = gray[idx - w as usize];
            let bottom = gray[idx + w as usize];
            let left = gray[idx - 1];
            let right = gray[idx + 1];

            let lap = top + bottom + left + right - (4.0 * center);
            laplacians.push(lap);

            // Sobel 3x3 for edge density
            let top_left = gray[idx - w as usize - 1];
            let top_right = gray[idx - w as usize + 1];
            let bottom_left = gray[idx + w as usize - 1];
            let bottom_right = gray[idx + w as usize + 1];

            let gx =
                top_right + (2.0 * right) + bottom_right - top_left - (2.0 * left) - bottom_left;
            let gy =
                bottom_left + (2.0 * bottom) + bottom_right - top_left - (2.0 * top) - top_right;
            let grad_mag = gx.hypot(gy);
            if grad_mag > 30.0 {
                edge_count += 1;
            }
        }
    }

    let n = num::count_f64(laplacians.len());
    let mean: f64 = laplacians.iter().sum::<f64>() / n;
    let variance: f64 = laplacians.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / n;
    let edge_density = num::count_f64(edge_count) / n;

    BlurReport {
        variance_of_laplacian: variance,
        edge_density,
        is_blurry: variance < blur_threshold,
    }
}

/// Share of pixels (0..=1) within `tolerance` (Euclidean RGB distance) of `color`. Samples
/// every fourth pixel in each direction, which is plenty for coverage of flat colour fills.
#[must_use]
pub fn color_coverage(img: &RgbImage, color: [u8; 3], tolerance: f64) -> f64 {
    let (w, h) = img.dimensions();
    let (mut hits, mut seen) = (0_u64, 0_u64);
    for y in (0..h).step_by(4) {
        for x in (0..w).step_by(4) {
            let p = img.get_pixel(x, y).0;
            let d: f64 = p
                .iter()
                .zip(color)
                .map(|(a, b)| (f64::from(*a) - f64::from(b)).powi(2))
                .sum();
            seen += 1;
            if d.sqrt() <= tolerance {
                hits += 1;
            }
        }
    }
    if seen == 0 {
        0.0
    } else {
        f64::from(u32::try_from(hits).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(seen).unwrap_or(u32::MAX))
    }
}

/// Analyze luminance distribution, highlight blowout, and shadow crushing.
#[must_use]
pub fn check_luminance(img: &RgbImage) -> LuminanceReport {
    let (w, h) = img.dimensions();
    let total_pixels = u64::from(w) * u64::from(h);
    if total_pixels == 0 {
        return LuminanceReport {
            mean_luminance: 0.0,
            blown_highlights_fraction: 0.0,
            crushed_shadows_fraction: 0.0,
            is_clipped: false,
        };
    }

    let mut sum_lum = 0.0;
    let mut blown = 0_u64;
    let mut crushed = 0_u64;

    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            let y_val = ((0.2126 * f64::from(p[0]))
                + (0.7152 * f64::from(p[1]))
                + (0.0722 * f64::from(p[2])))
                / 255.0;
            sum_lum += y_val;
            if y_val > 0.98 {
                blown += 1;
            } else if y_val < 0.02 {
                crushed += 1;
            }
        }
    }

    let total = num::u64_f64(total_pixels);
    let blown_fraction = num::u64_f64(blown) / total;
    let crushed_fraction = num::u64_f64(crushed) / total;

    LuminanceReport {
        mean_luminance: sum_lum / total,
        blown_highlights_fraction: blown_fraction,
        crushed_shadows_fraction: crushed_fraction,
        is_clipped: blown_fraction > 0.50 || crushed_fraction > 0.50,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn ciede2000_identical_colors_zero() {
        let lab = srgb_to_lab(128, 128, 128);
        let de = ciede2000(lab, lab);
        assert!(de.abs() < 1e-4);
    }

    #[test]
    fn identical_images_flagged_as_duplicate() {
        let img1 = RgbImage::from_pixel(64, 64, Rgb([100, 150, 200]));
        let img2 = RgbImage::from_pixel(64, 64, Rgb([100, 150, 200]));
        let result = compare_frames(&img1, &img2);
        assert!(result.is_duplicate);
        assert!(result.mean_delta_e < 1.0);
    }

    #[test]
    fn distinct_images_flagged_as_different() {
        let img1 = RgbImage::from_pixel(64, 64, Rgb([20, 20, 20]));
        let img2 = RgbImage::from_pixel(64, 64, Rgb([240, 240, 240]));
        let result = compare_frames(&img1, &img2);
        assert!(!result.is_duplicate);
        assert!(result.mean_delta_e > 50.0);
    }

    #[test]
    fn detect_blur_separates_sharp_and_flat() {
        // Flat image has zero variance (blurry / featureless)
        let flat = RgbImage::from_pixel(64, 64, Rgb([128, 128, 128]));
        let report_flat = detect_blur(&flat, 50.0);
        assert!(report_flat.is_blurry);
        assert_eq!(report_flat.variance_of_laplacian, 0.0);

        // High contrast checkerboard pattern has high variance (sharp)
        let mut sharp = RgbImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let color = if (x / 4 + y / 4) % 2 == 0 { 255 } else { 0 };
                sharp.put_pixel(x, y, Rgb([color, color, color]));
            }
        }
        let report_sharp = detect_blur(&sharp, 50.0);
        assert!(!report_sharp.is_blurry);
        assert!(report_sharp.variance_of_laplacian > 500.0);
    }

    #[test]
    fn detects_blown_and_crushed_luminance() {
        let blown_img = RgbImage::from_pixel(32, 32, Rgb([255, 255, 255]));
        let rep_blown = check_luminance(&blown_img);
        assert!(rep_blown.is_clipped);
        assert!(rep_blown.blown_highlights_fraction > 0.99);

        let crushed_img = RgbImage::from_pixel(32, 32, Rgb([0, 0, 0]));
        let rep_crushed = check_luminance(&crushed_img);
        assert!(rep_crushed.is_clipped);
        assert!(rep_crushed.crushed_shadows_fraction > 0.99);
    }
}
