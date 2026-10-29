//! Deterministic hole propagation favouring far/background pixels at occlusion boundaries.
use crate::dibr::Warped;
use image::Rgba;
use std::collections::VecDeque;

pub fn fill(warped: &mut Warped) {
    let width = warped.image.width();
    let height = warped.image.height();
    let mut queue = VecDeque::new();
    let mut queued = vec![false; warped.holes.len()];
    let mut distance = vec![0_u32; warped.holes.len()];
    for y in 0..height {
        for x in 0..width {
            let i = y as usize * width as usize + x as usize;
            if warped.holes[i]
                && neighbors(x, y, width, height)
                    .any(|(nx, ny)| !warped.holes[ny as usize * width as usize + nx as usize])
            {
                queue.push_back((x, y));
                queued[i] = true;
                distance[i] = 1;
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let i = y as usize * width as usize + x as usize;
        let radius = f64::from(distance[i]) / f64::from(width.min(height).max(500));
        warped.audit.fill_radius_ratio = warped.audit.fill_radius_ratio.max(radius);
        // Leave broad reveals unresolved so QA chooses a gentler move instead of smearing.
        if radius > 0.004 {
            continue;
        }
        let points = neighbors(x, y, width, height)
            .filter(|(nx, ny)| {
                !warped.holes[*ny as usize * width as usize + *nx as usize]
                    && warped.image.get_pixel(*nx, *ny)[3] > 0
            })
            .collect::<Vec<_>>();
        if points.is_empty() {
            continue;
        }
        let far = points
            .iter()
            .map(|(nx, ny)| warped.depths[*ny as usize * width as usize + *nx as usize])
            .fold(1.0_f32, f32::min);
        let pixel = background_pixel(warped, &points, far);
        warped.image.put_pixel(x, y, pixel);
        warped.depths[i] = far;
        warped.holes[i] = false;
        for (nx, ny) in neighbors(x, y, width, height) {
            let j = ny as usize * width as usize + nx as usize;
            if warped.holes[j] && !queued[j] {
                queue.push_back((nx, ny));
                queued[j] = true;
                distance[j] = distance[i] + 1;
            }
        }
    }
    warped.audit.unresolved_holes =
        u32::try_from(warped.holes.iter().filter(|h| **h).count()).unwrap_or(u32::MAX);
}
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "averages of u8 channels remain inside [0,255]"
)]
fn background_pixel(warped: &Warped, points: &[(u32, u32)], far: f32) -> Rgba<u8> {
    let mut sum = [0.0_f64; 4];
    let mut weight = 0.0;
    for (x, y) in points {
        let d = warped.depths[*y as usize * warped.image.width() as usize + *x as usize];
        if d > far + 0.05 {
            continue;
        }
        let p = warped.image.get_pixel(*x, *y);
        for c in 0..4 {
            sum[c] += f64::from(p[c]);
        }
        weight += 1.0;
    }
    Rgba(sum.map(|v| (v / weight).round() as u8))
}
fn neighbors(x: u32, y: u32, w: u32, h: u32) -> impl Iterator<Item = (u32, u32)> {
    let left = x.checked_sub(1).map(|x| (x, y));
    let up = y.checked_sub(1).map(|y| (x, y));
    [
        left,
        up,
        (x + 1 < w).then_some((x + 1, y)),
        (y + 1 < h).then_some((x, y + 1)),
    ]
    .into_iter()
    .flatten()
}
