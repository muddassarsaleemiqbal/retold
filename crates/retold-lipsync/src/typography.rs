//! Visual typography localization for RTL, CJK vertical, and Latin scripts.

use serde::{Deserialize, Serialize};

/// Text layout writing direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextOrientation {
    /// Standard left-to-right horizontal (English, Spanish, etc.).
    HorizontalLtr,
    /// Right-to-left horizontal (Arabic, Urdu, Hebrew, Persian).
    HorizontalRtl,
    /// Top-to-bottom vertical column layout (Traditional CJK Japanese/Chinese).
    VerticalTtb,
}

/// Localized typographic rendering specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypographyLayout {
    pub orientation: TextOrientation,
    pub font_size_scale: f32,
    pub line_spacing_scale: f32,
    pub glyph_padding_px: u32,
}

impl TypographyLayout {
    /// Informs appropriate typographic layout parameters for a given ISO language tag.
    #[must_use]
    pub fn for_language(lang: &str) -> Self {
        let clean = lang.trim().to_lowercase();
        let base = clean.split('-').next().unwrap_or(&clean);

        match base {
            "ar" | "ur" | "fa" | "he" => Self {
                orientation: TextOrientation::HorizontalRtl,
                font_size_scale: 1.25, // Complex Arabic ligatures require larger point size
                line_spacing_scale: 1.35,
                glyph_padding_px: 6,
            },
            "hi" | "mr" | "ne" => Self {
                orientation: TextOrientation::HorizontalLtr,
                font_size_scale: 1.1,
                line_spacing_scale: 1.25,
                glyph_padding_px: 5,
            },
            "ja" | "zh" => Self {
                orientation: TextOrientation::HorizontalLtr,
                font_size_scale: 1.1,
                line_spacing_scale: 1.25,
                glyph_padding_px: 4,
            },
            _ => Self {
                orientation: TextOrientation::HorizontalLtr,
                font_size_scale: 1.0,
                line_spacing_scale: 1.0,
                glyph_padding_px: 2,
            },
        }
    }

    /// Formats a CJK vertical layout when specifically requested.
    #[must_use]
    pub const fn cjk_vertical() -> Self {
        Self {
            orientation: TextOrientation::VerticalTtb,
            font_size_scale: 1.15,
            line_spacing_scale: 1.4,
            glyph_padding_px: 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtl_layout_selection() {
        let ar = TypographyLayout::for_language("ar");
        assert_eq!(ar.orientation, TextOrientation::HorizontalRtl);
        assert!(ar.font_size_scale > 1.2);

        let ur = TypographyLayout::for_language("ur-PK");
        assert_eq!(ur.orientation, TextOrientation::HorizontalRtl);
    }

    #[test]
    fn test_latin_and_cjk_layouts() {
        let en = TypographyLayout::for_language("en");
        assert_eq!(en.orientation, TextOrientation::HorizontalLtr);
        assert_eq!(en.font_size_scale, 1.0);

        let cjk_vert = TypographyLayout::cjk_vertical();
        assert_eq!(cjk_vert.orientation, TextOrientation::VerticalTtb);
    }
}
