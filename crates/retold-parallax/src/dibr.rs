//! Subpixel forward splatting with a z buffer and premultiplied bilinear colour weights.
use crate::{
    Error,
    camera::{Pose, project},
    depth::DepthMap,
};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Audit {
    pub hole_ratio: f64,
    pub stretched_fraction: f64,
    pub max_stretch: f64,
    pub unresolved_holes: u32,
    /// Border gaps before filling, including isolated subpixel splat gaps.
    #[serde(default)]
    pub edge_holes: u32,
    /// Constant focal multiplier required to cover the full camera move.
    #[serde(default)]
    pub framing_scale: f64,
    /// Largest attempted fill distance / max(shorter image side, 500 pixels).
    #[serde(default)]
    pub fill_radius_ratio: f64,
}
#[derive(Debug)]
pub struct Warped {
    pub image: RgbaImage,
    pub holes: Vec<bool>,
    pub depths: Vec<f32>,
    pub audit: Audit,
}
#[derive(Debug)]
struct Splat {
    size: (u32, u32),
    z: Vec<f64>,
    weight: Vec<f64>,
    color: Vec<[f64; 4]>,
    depths: Vec<f32>,
}
impl Splat {
    fn new(size: (u32, u32)) -> Self {
        let n = size.0 as usize * size.1 as usize;
        Self {
            size,
            z: vec![f64::INFINITY; n],
            weight: vec![0.0; n],
            color: vec![[0.0; 4]; n],
            depths: vec![0.0; n],
        }
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "splat coordinates are bounds checked before indexing"
    )]
    fn add(&mut self, point: (f64, f64, f64), depth: f32, pixel: Rgba<u8>) {
        let (x, y, z) = point;
        if !x.is_finite()
            || !y.is_finite()
            || z <= 0.0
            || x < -1.0
            || y < -1.0
            || x >= f64::from(self.size.0)
            || y >= f64::from(self.size.1)
        {
            return;
        }
        let x0 = x.floor();
        let y0 = y.floor();
        let fx = x - x0;
        let fy = y - y0;
        for (sx, sy, w) in [
            (x0, y0, (1.0 - fx) * (1.0 - fy)),
            (x0 + 1.0, y0, fx * (1.0 - fy)),
            (x0, y0 + 1.0, (1.0 - fx) * fy),
            (x0 + 1.0, y0 + 1.0, fx * fy),
        ] {
            if w <= 1e-9
                || sx < 0.0
                || sy < 0.0
                || sx >= f64::from(self.size.0)
                || sy >= f64::from(self.size.1)
            {
                continue;
            }
            let i = sy as usize * self.size.0 as usize + sx as usize;
            if z > self.z[i] + 0.03 {
                continue;
            }
            if z < self.z[i] - 0.03 {
                self.weight[i] = 0.0;
                self.color[i] = [0.0; 4];
            }
            self.z[i] = self.z[i].min(z);
            self.depths[i] = self.depths[i].max(depth);
            self.weight[i] += w;
            let alpha = f64::from(pixel[3]) / 255.0;
            for c in 0..3 {
                self.color[i][c] += f64::from(pixel[c]) * alpha * w;
            }
            self.color[i][3] += alpha * w;
        }
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "colour and alpha values are explicitly clamped to u8 bounds"
    )]
    fn finish(self, source: &RgbaImage, mut audit: Audit) -> Warped {
        let mut image = RgbaImage::new(self.size.0, self.size.1);
        let mut holes = vec![false; self.weight.len()];
        let mut count = 0;
        for (i, p) in image.pixels_mut().enumerate() {
            let alpha = self.color[i][3];
            if self.weight[i] <= 1e-9 || alpha <= 1e-9 {
                holes[i] = source.as_raw()[i * 4 + 3] > 0;
                count += u32::from(holes[i]);
                continue;
            }
            let color = self.color[i];
            *p = Rgba([
                (color[0] / alpha).round().clamp(0.0, 255.0) as u8,
                (color[1] / alpha).round().clamp(0.0, 255.0) as u8,
                (color[2] / alpha).round().clamp(0.0, 255.0) as u8,
                (alpha / self.weight[i] * 255.0).round().clamp(0.0, 255.0) as u8,
            ]);
        }
        audit.hole_ratio = f64::from(count) / (f64::from(self.size.0) * f64::from(self.size.1));
        audit.unresolved_holes = count;
        Warped {
            image,
            holes,
            depths: self.depths,
            audit,
        }
    }
}
pub fn warp(image: &RgbaImage, depth: &DepthMap, pose: Pose) -> Result<Warped, Error> {
    depth.validate()?;
    if image.dimensions() != (depth.width, depth.height) {
        return Err(Error::Invalid("depth and scene dimensions differ".into()));
    }
    let size = image.dimensions();
    let mut splat = Splat::new(size);
    let mut stretched = 0_u32;
    let mut edges = 0_u32;
    let mut max_stretch = 1.0_f64;
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] == 0 {
            continue;
        }
        let d = depth.at(x, y);
        let p = project(x, y, d, size, pose);
        splat.add(p, d, *pixel);
        for (nx, ny) in [(x + 1, y), (x, y + 1)] {
            if nx >= size.0 || ny >= size.1 || image.get_pixel(nx, ny)[3] == 0 {
                continue;
            }
            let neighbor = project(nx, ny, depth.at(nx, ny), size, pose);
            let stretch = (p.0 - neighbor.0).hypot(p.1 - neighbor.1);
            edges += 1;
            stretched += u32::from(stretch > 1.8);
            max_stretch = max_stretch.max(stretch);
        }
    }
    Ok(splat.finish(
        image,
        Audit {
            stretched_fraction: f64::from(stretched) / f64::from(edges.max(1)),
            max_stretch,
            ..Audit::default()
        },
    ))
}
