//! Measuring text with the real font, so captions and titles are fitted before they reach
//! the editor. Shaping uses harfrust, so Devanagari conjuncts and Nastaliq joins are measured
//! as they will render.

use std::sync::Arc;

use fontdb::{Database, Style, Weight};
use retold_core::num;

/// Something that can tell how wide a line of text is.
pub trait Measure: Send + Sync {
    /// Width in pixels of `text` set at `size_px`.
    fn width(&self, text: &str, size_px: f64) -> f64;
    /// Font ascent and descent in pixels, both positive, for safe baseline placement.
    fn vertical_bounds(&self, size_px: f64) -> (f64, f64) {
        (size_px, size_px * 0.3)
    }
}

/// Fonts installed on this machine.
#[derive(Clone)]
pub struct Fonts {
    db: Arc<Database>,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts")
            .field("faces", &self.db.len())
            .finish()
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("font `{family} {style}` is not installed{}", if available.is_empty() { String::new() } else { format!(" (installed styles: {})", available.join(", ")) })]
pub struct MissingFont {
    pub family: String,
    pub style: String,
    pub available: Vec<String>,
}

const WEIGHT_PATTERNS: &[(&[&str], u16)] = &[
    (&["thin", "hairline"], 100),
    (&["extralight", "ultralight"], 200),
    (&["light"], 300),
    (&["medium"], 500),
    (&["semibold", "demibold"], 600),
    (&["extrabold", "ultrabold"], 800),
    (&["black", "heavy"], 900),
    (&["bold"], 700),
];

/// Maps a style name ("Bold", "`SemiBold`", "Black") to a weight.
#[must_use]
pub fn weight_of(style: &str) -> Weight {
    let normalized = style.to_ascii_lowercase().replace([' ', '-'], "");
    for (keywords, weight_val) in WEIGHT_PATTERNS {
        if keywords.iter().any(|keyword| normalized.contains(keyword)) {
            return Weight(*weight_val);
        }
    }
    Weight(400)
}

impl Fonts {
    /// Loads the system font folders (a few hundred milliseconds; do it once).
    #[must_use]
    pub fn system() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        Self { db: Arc::new(db) }
    }

    /// Finds `family` in exactly the asked style; a different weight is reported, not used,
    /// because a silent fallback changes the look of every caption.
    pub fn face(&self, family: &str, style: &str) -> Result<FontFace, MissingFont> {
        let want = weight_of(style);
        let italic = style.to_ascii_lowercase().contains("italic");
        let faces: Vec<&fontdb::FaceInfo> = self
            .db
            .faces()
            .filter(|f| {
                f.families
                    .iter()
                    .any(|(n, _)| n.eq_ignore_ascii_case(family))
            })
            .collect();
        let found = faces
            .iter()
            .find(|f| f.weight == want && (f.style != Style::Normal) == italic);
        if let Some(f) = found {
            Ok(FontFace {
                db: Arc::clone(&self.db),
                id: f.id,
            })
        } else {
            let mut available: Vec<String> =
                faces.iter().map(|f| f.post_script_name.clone()).collect();
            available.sort();
            available.dedup();
            Err(MissingFont {
                family: family.to_owned(),
                style: style.to_owned(),
                available,
            })
        }
    }
}

/// One installed font face.
#[derive(Clone)]
pub struct FontFace {
    db: Arc<Database>,
    id: fontdb::ID,
}

impl std::fmt::Debug for FontFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontFace")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Measure for FontFace {
    fn vertical_bounds(&self, size_px: f64) -> (f64, f64) {
        use read_fonts::TableProvider;
        self.db
            .with_face_data(self.id, |data, index| {
                let font = read_fonts::FontRef::from_index(data, index).ok()?;
                let scale = size_px / f64::from(font.head().ok()?.units_per_em());
                let metrics = font.hhea().ok()?;
                Some((
                    f64::from(metrics.ascender().to_i16()).max(0.0) * scale,
                    -f64::from(metrics.descender().to_i16()).min(0.0) * scale,
                ))
            })
            .flatten()
            .unwrap_or((size_px, size_px * 0.3))
    }
    fn width(&self, text: &str, size_px: f64) -> f64 {
        self.db
            .with_face_data(self.id, |data, index| {
                let font = harfrust::FontRef::from_index(data, index).ok()?;
                let data = harfrust::ShaperData::new(&font);
                let face = data.shaper(&font).build();
                let upem = f64::from(face.units_per_em());
                let mut buf = harfrust::UnicodeBuffer::new();
                buf.push_str(text);
                buf.guess_segment_properties();
                let shaped = face.shape(buf, harfrust::ShapeOptions::default());
                let advance: i64 = shaped
                    .glyph_positions()
                    .iter()
                    .map(|p| i64::from(p.x_advance))
                    .sum();
                Some(num::i64_f64(advance) * size_px / upem)
            })
            .flatten()
            .unwrap_or_else(|| estimate(text, size_px))
    }
}

/// A rough width when no font is available: 0.6 em per grapheme.
#[must_use]
pub fn estimate(text: &str, size_px: f64) -> f64 {
    use unicode_segmentation::UnicodeSegmentation;
    num::count_f64(text.graphemes(true).count()) * size_px * 0.6
}

/// Measures with the fallback estimate (tests, or when a font is missing).
#[derive(Debug, Clone, Copy, Default)]
pub struct Estimate;

impl Measure for Estimate {
    fn width(&self, text: &str, size_px: f64) -> f64 {
        estimate(text, size_px)
    }
}

/// The largest size, at most `size`, at which `text` fits in `max_width` (never below
/// `min_size`).
#[must_use]
pub fn fit_size(m: &dyn Measure, text: &str, size: f64, max_width: f64, min_size: f64) -> f64 {
    let w = m.width(text, size);
    if w <= max_width || w <= 0.0 {
        size
    } else {
        (size * max_width / w).floor().max(min_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_map_to_weights() {
        assert_eq!(weight_of("Bold"), Weight(700));
        assert_eq!(weight_of("SemiBold"), Weight(600));
        assert_eq!(weight_of("Black"), Weight(900));
        assert_eq!(weight_of("Regular"), Weight(400));
    }

    #[test]
    fn fit_shrinks_only_when_needed() {
        assert_eq!(fit_size(&Estimate, "SHORT", 72.0, 900.0, 40.0), 72.0);
        let s = fit_size(
            &Estimate,
            "A VERY LONG CAPTION LINE HERE",
            72.0,
            900.0,
            40.0,
        );
        assert!(s < 72.0 && Estimate.width("A VERY LONG CAPTION LINE HERE", s) <= 900.0);
    }

    /// Uses fonts that ship with macOS; skipped elsewhere.
    #[test]
    fn shapes_real_fonts_when_present() {
        let fonts = Fonts::system();
        if let Ok(face) = fonts.face("Kohinoor Devanagari", "Bold") {
            let w = face.width("नोस्फ़ेरातु", 72.0);
            assert!(w > 100.0 && w < 800.0, "{w}");
        }
        if let Ok(face) = fonts.face("Noto Nastaliq Urdu", "Bold") {
            assert!(face.width("نوسفیراتو", 72.0) > 50.0);
        }
        assert!(fonts.face("Definitely Not A Font", "Bold").is_err());
    }
}
