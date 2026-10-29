//! Initial Retold tokens, kept synchronized with the Kit/Base control palette.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, px, rgb};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub canvas: Hsla,
    pub surface: Hsla,
    pub text: Hsla,
    pub secondary: Hsla,
    pub border: Hsla,
    pub accent: Hsla,
    pub attention: Hsla,
    pub error: Hsla,
    pub success: Hsla,
    pub control: Hsla,
    /// Source-list background, a step below the canvas.
    pub sidebar: Hsla,
    /// Saturated fill for primary buttons and selection; text on it is `on_accent`.
    pub accent_fill: Hsla,
    pub on_accent: Hsla,
}

/// The macOS body size; every rem in the app scales from it.
pub const BASE_FONT: f32 = 13.;

pub fn zoom(percent: u16, cx: &mut App) {
    Theme::update(cx, |theme| {
        theme.font_size = px(BASE_FONT * f32::from(percent) / 100.)
    });
}

impl Palette {
    pub fn new(light: bool) -> Self {
        Self::with_contrast(light, false)
    }

    pub fn with_contrast(light: bool, high_contrast: bool) -> Self {
        let colors = if high_contrast && light {
            [
                0xFF_FFFF, 0xFF_FFFF, 0x00_0000, 0x00_0000, 0x00_0000, 0x00_3B80, 0x57_3900,
            ]
        } else if high_contrast {
            [
                0x00_0000, 0x00_0000, 0xFF_FFFF, 0xFF_FFFF, 0xFF_FFFF, 0x9A_D0FF, 0xFF_E066,
            ]
        } else if light {
            [
                0xF4_F7FA, 0xFF_FFFF, 0x1C_2A36, 0x55_6676, 0xD0_D7DE, 0x24_5C91, 0x9A_6700,
            ]
        } else {
            [
                0x1E_2935, 0x2A_3949, 0xED_F3F7, 0xA4_B3C2, 0x36_4655, 0x83_B8E8, 0xE6_C482,
            ]
        };
        let [canvas, surface, text, secondary, border, accent, attention] =
            colors.map(|v| rgb(v).into());
        let (error, success) = if high_contrast {
            (text, text)
        } else if light {
            (rgb(0xB4_2318).into(), rgb(0x2F_6B4F).into())
        } else {
            (rgb(0xFF_B4AB).into(), rgb(0x83_D3A1).into())
        };
        let control = if high_contrast {
            rgb(if light { 0x94_9494 } else { 0x5A_5A5A }).into()
        } else {
            rgb(if light { 0xEC_EFF3 } else { 0x33_404F }).into()
        };
        let sidebar = match (high_contrast, light) {
            (true, _) => canvas,
            (false, true) => rgb(0xEC_EFF3).into(),
            (false, false) => rgb(0x18_212B).into(),
        };
        let accent_fill = match (high_contrast, light) {
            (true, _) => accent,
            (false, true) => rgb(0x24_5C91).into(),
            (false, false) => rgb(0x3B_82D6).into(),
        };
        let on_accent = if high_contrast && !light {
            canvas
        } else {
            rgb(0xFF_FFFF).into()
        };
        Self {
            canvas,
            surface,
            text,
            secondary,
            border,
            accent,
            attention,
            error,
            success,
            control,
            sidebar,
            accent_fill,
            on_accent,
        }
    }
}

pub fn apply(light: bool, cx: &mut App) {
    apply_contrast(light, false, cx);
    zoom(100, cx);
}

pub fn apply_contrast(light: bool, high_contrast: bool, cx: &mut App) {
    Theme::change(
        if light {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        },
        None,
        cx,
    );
    let p = Palette::with_contrast(light, high_contrast);
    Theme::update(cx, |theme| {
        theme.colors.background = p.canvas;
        theme.colors.foreground = p.text;
        theme.colors.muted_foreground = p.secondary;
        theme.colors.border = p.border;
        theme.colors.danger = p.error;
        theme.colors.success = p.success;
        theme.colors.warning = p.attention;
        // Gpui-component draws input borders with `input`; keep fields visibly framed.
        theme.colors.input = match (high_contrast, light) {
            (true, _) => p.text,
            (false, true) => rgb(0xB8_C3CF).into(),
            (false, false) => rgb(0x4A_5768).into(),
        };
        theme.colors.popover = p.surface;
        theme.colors.popover_foreground = p.text;
        theme.colors.accent = if high_contrast {
            p.surface
        } else {
            p.accent_fill.opacity(0.18)
        };
        theme.colors.accent_foreground = if high_contrast { p.text } else { p.accent };
        theme.colors.list = p.surface;
        theme.colors.list_active = if high_contrast { p.surface } else { p.border };
        theme.colors.list_active_border = p.accent;
        theme.colors.list_hover = if high_contrast { p.surface } else { p.border };
        theme.colors.primary = p.accent_fill;
        theme.colors.primary_foreground = p.on_accent;
        theme.colors.button_primary = p.accent_fill;
        theme.colors.button_primary_foreground = p.on_accent;
        theme.colors.button = p.control;
        theme.colors.button_foreground = p.text;
        theme.colors.button_hover = if high_contrast { p.surface } else { p.border };
        if !high_contrast {
            theme.colors.button_primary_hover = p.accent_fill.opacity(0.88);
            theme.colors.button_primary_active = p.accent_fill.opacity(0.76);
            theme.colors.secondary_active = p.accent_fill.opacity(0.22);
            theme.colors.button_active = p.accent_fill.opacity(0.22);
        }
        theme.radius = px(6.);
        theme.radius_lg = px(8.);
        if high_contrast {
            theme.colors.button = p.control;
            theme.colors.button_active = p.control;
            theme.colors.button_hover = p.control;
            theme.colors.secondary = p.control;
            theme.colors.secondary_active = p.control;
            theme.colors.secondary_foreground = p.text;
            theme.colors.button_primary_hover = p.accent;
            theme.colors.button_primary_active = p.accent;
        }
        theme.colors.ring = p.accent;
        theme.colors.switch_thumb = rgb(0xFF_FFFF).into();
        // Dialogs dim the window behind them so the sheet reads as in front.
        theme.colors.overlay = gpui_kit::hsla(0., 0., 0., if light { 0.22 } else { 0.42 });
        theme.colors.switch = if light {
            rgb(0x8A_97A8).into()
        } else {
            rgb(0x4A_5363).into()
        };
    });
}
