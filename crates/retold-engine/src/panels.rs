//! Multi-panel frames: several supporting shots of one scene drawn as one storyboard sheet,
//! then split along its white gutters into full frames. One generation instead of two or
//! four, and the panels share faces, outfits and light because they were drawn together.

use image::RgbImage;

use crate::story::Shot;
use retold_core::num;

/// Columns, rows and the sheet's aspect for `panels` frames of `aspect` ("W:H"), chosen so
/// each panel is close to the frame's own shape.
#[must_use]
pub fn grid(panels: u32, aspect: &str) -> (u32, u32, String) {
    let (w, h) = ratio(aspect);
    match panels {
        4 => (2, 2, aspect.to_owned()),
        // Two portrait frames side by side, or two landscape frames stacked, fill a square.
        2 if w < h => (2, 1, "1:1".to_owned()),
        2 => (1, 2, "1:1".to_owned()),
        _ => (1, 1, aspect.to_owned()),
    }
}

fn ratio(aspect: &str) -> (f64, f64) {
    let mut parts = aspect
        .split(':')
        .map(|p| p.trim().parse::<f64>().unwrap_or(1.0));
    (parts.next().unwrap_or(1.0), parts.next().unwrap_or(1.0))
}

/// Consecutive fresh shots that may share a sheet: same setting and characters, not a hero
/// shot, not reused, not `excluded` (e.g. being redone). Only full groups are returned.
#[must_use]
pub fn groups(shots: &[Shot], panels: usize, excluded: &dyn Fn(usize) -> bool) -> Vec<Vec<usize>> {
    if panels < 2 {
        return vec![];
    }
    let eligible = |s: &Shot| !s.hero && s.reuse.is_none() && !excluded(s.unit);
    let key = |s: &Shot| {
        let mut c = s.characters.clone();
        c.sort();
        (s.setting.clone(), c)
    };
    let mut out = Vec::new();
    let mut run: Vec<&Shot> = Vec::new();
    for s in shots {
        if !eligible(s) {
            // A reused shot does not break a run: it is not drawn.
            if s.reuse.is_none() {
                run.clear();
            }
            continue;
        }
        if run.last().is_some_and(|l| key(l) != key(s)) {
            run.clear();
        }
        run.push(s);
        if run.len() == panels {
            out.push(run.iter().map(|s| s.unit).collect());
            run.clear();
        }
    }
    out
}

/// Channel value at or above which a pixel counts as gutter white.
const WHITE: u8 = 232;
/// A gutter is searched within this fraction of the sheet either side of where it should be.
const GUTTER_SEARCH: f64 = 0.1;
/// Share of white a line needs to be a gutter's core, and to extend the gutter outwards.
const GUTTER_CORE: f64 = 0.97;
const GUTTER_EDGE: f64 = 0.9;
/// A panel smaller than this fraction of its grid cell means the cuts went wrong.
const MIN_PANEL_FRACTION: u32 = 3;

/// Which way a gutter line runs through the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    /// A column: a vertical gutter between side-by-side panels.
    Column,
    /// A row: a horizontal gutter between stacked panels.
    Row,
}

impl Axis {
    /// Lines across the sheet in this direction, and pixels along each line.
    fn lines_and_length(self, img: &RgbImage) -> (u32, u32) {
        let (width, height) = img.dimensions();
        match self {
            Self::Column => (width, height),
            Self::Row => (height, width),
        }
    }

    /// Share of near-white pixels on line `at`.
    fn white_share(self, img: &RgbImage, at: u32) -> f64 {
        let (_, length) = self.lines_and_length(img);
        let white = (0..length)
            .filter(|&along| {
                let pixel = match self {
                    Self::Column => img.get_pixel(at, along),
                    Self::Row => img.get_pixel(along, at),
                };
                pixel.0.iter().all(|channel| *channel >= WHITE)
            })
            .count();
        num::count_f64(white) / f64::from(length.max(1))
    }

    /// The gutter near fraction `at` of the sheet: its first and last line, when one line
    /// there is almost entirely white.
    fn gutter(self, img: &RgbImage, at: f64) -> Option<(u32, u32)> {
        let (lines, _) = self.lines_and_length(img);
        let low = num::trunc_u32(f64::from(lines) * (at - GUTTER_SEARCH));
        let high = num::trunc_u32(f64::from(lines) * (at + GUTTER_SEARCH));
        let (best, share) = (low..high)
            .map(|line| (line, self.white_share(img, line)))
            .max_by(|(_, a), (_, b)| a.total_cmp(b))?;
        if share < GUTTER_CORE {
            return None;
        }
        let mut start = best;
        while start > 0 && self.white_share(img, start - 1) >= GUTTER_EDGE {
            start -= 1;
        }
        let mut end = best;
        while end + 1 < lines && self.white_share(img, end + 1) >= GUTTER_EDGE {
            end += 1;
        }
        Some((start, end))
    }

    /// `count` panel spans along this axis, separated by gutters.
    fn spans(self, img: &RgbImage, count: u32) -> Result<Vec<(u32, u32)>, NoGutters> {
        let (lines, _) = self.lines_and_length(img);
        let mut spans = Vec::new();
        let mut from = 0;
        for cut in 1..count {
            let (gutter_start, gutter_end) = self
                .gutter(img, f64::from(cut) / f64::from(count))
                .ok_or(NoGutters)?;
            spans.push((from, gutter_start));
            from = gutter_end + 1;
        }
        spans.push((from, lines));
        Ok(spans)
    }
}

/// Splits a sheet into `cols × rows` panels (row-major), each centre-cropped to the frame
/// aspect (the caller enlarges them to frame size). Fails when a gutter cannot be found, so
/// the caller draws those shots one by one instead.
///
/// # Errors
/// [`NoGutters`] when the sheet has no clean gutter between panels or a panel is too small.
pub fn split(
    sheet: &RgbImage,
    cols: u32,
    rows: u32,
    aspect: &str,
) -> Result<Vec<RgbImage>, NoGutters> {
    let (width, height) = sheet.dimensions();
    let columns = Axis::Column.spans(sheet, cols)?;
    let rows_spans = Axis::Row.spans(sheet, rows)?;
    let (min_width, min_height) = (
        width / (cols * MIN_PANEL_FRACTION),
        height / (rows * MIN_PANEL_FRACTION),
    );
    let mut panels = Vec::new();
    for &(top, bottom) in &rows_spans {
        for &(left, right) in &columns {
            let (panel_width, panel_height) =
                (right.saturating_sub(left), bottom.saturating_sub(top));
            if panel_width < min_width || panel_height < min_height {
                return Err(NoGutters);
            }
            let (crop_width, crop_height) = centre_crop(panel_width, panel_height, aspect);
            let x = left + (panel_width - crop_width) / 2;
            let y = top + (panel_height - crop_height) / 2;
            panels.push(image::imageops::crop_imm(sheet, x, y, crop_width, crop_height).to_image());
        }
    }
    Ok(panels)
}

/// The largest `aspect` rectangle inside a `width` x `height` panel.
fn centre_crop(width: u32, height: u32, aspect: &str) -> (u32, u32) {
    let (frame_width, frame_height) = ratio(aspect);
    if f64::from(width) / f64::from(height) > frame_width / frame_height {
        (
            num::trunc_u32(f64::from(height) * frame_width / frame_height),
            height,
        )
    } else {
        (
            width,
            num::trunc_u32(f64::from(width) * frame_height / frame_width),
        )
    }
}

/// A panel sheet without clean gutters between its panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoGutters;

/// Frame size for a resolution label ("1K"/"2K") and aspect: the long side is 1024 or 2048.
#[must_use]
pub fn frame_size(resolution: &str, aspect: &str) -> (u32, u32) {
    let long = if resolution == "1K" { 1024.0 } else { 2048.0 };
    let (w, h) = ratio(aspect);
    let short = |a: f64, b: f64| num::round_u32(long * a / b);
    if w >= h {
        (num::trunc_u32(long), short(h, w))
    } else {
        (short(w, h), num::trunc_u32(long))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(unit: usize, setting: &str) -> Shot {
        serde_json::from_value(serde_json::json!({"unit":unit,"shot":"medium","location":"l","action":"a","mood":"m","characters":["Vera"],"focus":[0.5,0.5],"setting":setting,"hero":false})).unwrap()
    }

    fn hero_shot(unit: usize, setting: &str) -> Shot {
        let mut s = shot(unit, setting);
        s.hero = true;
        s
    }

    #[test]
    fn groups_take_full_runs_of_one_scene_and_skip_heroes() {
        let shots = [
            hero_shot(0, "Hall"),
            shot(1, "Hall"),
            shot(2, "Hall"),
            shot(3, "Beach"),
            shot(4, "Beach"),
            shot(5, "Beach"),
        ];
        assert_eq!(groups(&shots, 2, &|_| false), [vec![1, 2], vec![3, 4]]);
        assert_eq!(groups(&shots, 2, &|u| u == 2), [vec![3, 4]]);
        assert_eq!(groups(&shots, 1, &|_| false), Vec::<Vec<usize>>::new());
    }

    #[test]
    fn grids_keep_panels_close_to_the_frame_shape() {
        assert_eq!(grid(2, "9:16"), (2, 1, "1:1".into()));
        assert_eq!(grid(2, "16:9"), (1, 2, "1:1".into()));
        assert_eq!(grid(4, "9:16"), (2, 2, "9:16".into()));
        assert_eq!(frame_size("2K", "9:16"), (1152, 2048));
    }

    #[test]
    fn splits_on_white_gutters_and_refuses_sheets_without_them() {
        // Two dark panels separated by a white gutter in the middle.
        let sheet = RgbImage::from_fn(400, 400, |x, _| {
            if (195..205).contains(&x) {
                image::Rgb([255, 255, 255])
            } else if x < 195 {
                image::Rgb([20, 20, 20])
            } else {
                image::Rgb([200, 30, 30])
            }
        });
        let panels = split(&sheet, 2, 1, "9:16").unwrap();
        assert_eq!(panels.len(), 2);
        let (w, h) = panels[0].dimensions();
        assert!(
            (f64::from(w) / f64::from(h) - 9.0 / 16.0).abs() < 0.01,
            "{w}x{h}"
        );
        assert!(
            panels[0].get_pixel(w / 2, h / 2).0[0] < 50
                && panels[1].get_pixel(w / 2, h / 2).0[0] > 150
        );
        let seamless = RgbImage::from_pixel(400, 400, image::Rgb([20, 20, 20]));
        assert_eq!(split(&seamless, 2, 1, "9:16"), Err(NoGutters));
    }
}
