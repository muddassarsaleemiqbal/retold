//! Design tokens. Screens ask for a role (`accent`, `muted`, `danger`, ...) and never for a raw
//! colour, so every theme, and terminals without true colour, restyle the whole app at once.
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Default,
    Light,
    HighContrast,
    NoColor,
}

/// One colour per role; `None` keeps the terminal's own colour.
#[derive(Debug, Clone, Copy)]
struct Tokens {
    text: Option<Color>,
    muted: Option<Color>,
    faint: Option<Color>,
    accent: Option<Color>,
    accent_text: Option<Color>,
    surface: Option<Color>,
    border: Option<Color>,
    success: Option<Color>,
    warning: Option<Color>,
    danger: Option<Color>,
    notice: Option<Color>,
}

const DARK_BACKGROUND: Color = Color::Rgb(30, 30, 46);
const LIGHT_BACKGROUND: Color = Color::Rgb(239, 241, 245);

const DARK: Tokens = Tokens {
    text: Some(Color::Rgb(205, 214, 244)),
    muted: Some(Color::Rgb(147, 153, 178)),
    faint: Some(Color::Rgb(88, 91, 112)),
    accent: Some(Color::Rgb(180, 190, 254)),
    accent_text: Some(Color::Rgb(30, 30, 46)),
    surface: Some(Color::Rgb(49, 50, 68)),
    border: Some(Color::Rgb(69, 71, 90)),
    success: Some(Color::Rgb(166, 227, 161)),
    warning: Some(Color::Rgb(249, 226, 175)),
    danger: Some(Color::Rgb(243, 139, 168)),
    notice: Some(Color::Rgb(137, 220, 235)),
};

const LIGHT: Tokens = Tokens {
    text: Some(Color::Rgb(76, 79, 105)),
    muted: Some(Color::Rgb(108, 111, 133)),
    faint: Some(Color::Rgb(156, 160, 176)),
    accent: Some(Color::Rgb(114, 135, 253)),
    accent_text: Some(Color::Rgb(239, 241, 245)),
    surface: Some(Color::Rgb(220, 224, 232)),
    border: Some(Color::Rgb(188, 192, 204)),
    success: Some(Color::Rgb(64, 160, 43)),
    warning: Some(Color::Rgb(223, 142, 29)),
    danger: Some(Color::Rgb(210, 15, 57)),
    notice: Some(Color::Rgb(4, 165, 229)),
};

/// For terminals without 24-bit colour: the 16 named colours every terminal maps well.
const ANSI: Tokens = Tokens {
    text: Some(Color::Gray),
    muted: Some(Color::Gray),
    faint: Some(Color::DarkGray),
    accent: Some(Color::LightBlue),
    accent_text: Some(Color::Black),
    surface: Some(Color::DarkGray),
    border: Some(Color::DarkGray),
    success: Some(Color::Green),
    warning: Some(Color::Yellow),
    danger: Some(Color::Red),
    notice: Some(Color::Cyan),
};

const ANSI_LIGHT: Tokens = Tokens {
    text: Some(Color::Black),
    muted: Some(Color::DarkGray),
    faint: Some(Color::DarkGray),
    accent: Some(Color::Blue),
    accent_text: Some(Color::White),
    surface: Some(Color::Gray),
    ..ANSI
};

const CONTRAST: Tokens = Tokens {
    text: Some(Color::White),
    muted: Some(Color::White),
    faint: Some(Color::Gray),
    accent: Some(Color::Yellow),
    accent_text: Some(Color::Black),
    surface: Some(Color::Black),
    border: Some(Color::White),
    success: Some(Color::LightGreen),
    warning: Some(Color::LightYellow),
    danger: Some(Color::LightRed),
    notice: Some(Color::LightCyan),
};

const MONO: Tokens = Tokens {
    text: None,
    muted: None,
    faint: None,
    accent: None,
    accent_text: None,
    surface: None,
    border: None,
    success: None,
    warning: None,
    danger: None,
    notice: None,
};

impl Theme {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Default => Self::Light,
            Self::Light => Self::HighContrast,
            Self::HighContrast => Self::NoColor,
            Self::NoColor => Self::Default,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "Dark",
            Self::Light => "Light",
            Self::HighContrast => "High contrast",
            Self::NoColor => "No colour",
        }
    }

    fn tokens(self) -> Tokens {
        match self {
            Self::Default => full_colour_or_ansi(DARK),
            Self::Light => {
                if true_colour() {
                    LIGHT
                } else {
                    ANSI_LIGHT
                }
            }
            Self::HighContrast => CONTRAST,
            Self::NoColor => MONO,
        }
    }

    const fn monochrome(self) -> bool {
        matches!(self, Self::NoColor)
    }

    /// Colour themes own their canvas; monochrome preserves terminal preferences.
    #[must_use]
    pub fn canvas(self) -> Style {
        let background = match self {
            Self::Default => {
                if true_colour() {
                    DARK_BACKGROUND
                } else {
                    Color::Black
                }
            }
            Self::Light => {
                if true_colour() {
                    LIGHT_BACKGROUND
                } else {
                    Color::White
                }
            }
            Self::HighContrast => Color::Black,
            Self::NoColor => return Style::default(),
        };
        self.text().bg(background)
    }

    #[must_use]
    pub fn text(self) -> Style {
        paint(self.tokens().text)
    }

    #[must_use]
    pub fn muted(self) -> Style {
        let style = paint(self.tokens().muted);
        if self.monochrome() {
            style.add_modifier(Modifier::DIM)
        } else {
            style
        }
    }

    #[must_use]
    pub fn faint(self) -> Style {
        let style = paint(self.tokens().faint);
        if self.monochrome() {
            style.add_modifier(Modifier::DIM)
        } else {
            style
        }
    }

    #[must_use]
    pub fn accent(self) -> Style {
        paint(self.tokens().accent)
    }

    #[must_use]
    pub fn strong(self) -> Style {
        self.text().add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn title(self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn border(self) -> Style {
        paint(self.tokens().border)
    }

    #[must_use]
    pub fn border_focus(self) -> Style {
        self.accent()
    }

    /// The selected row: a filled accent bar, or reversed text without colour.
    #[must_use]
    pub fn selected(self) -> Style {
        let tokens = self.tokens();
        if self.monochrome() {
            return Style::default().add_modifier(Modifier::REVERSED);
        }
        let style = Style::default().add_modifier(Modifier::BOLD);
        let style = tokens.accent.map_or(style, |colour| style.bg(colour));
        tokens.accent_text.map_or(style, |colour| style.fg(colour))
    }

    /// A selected row in a pane that does not have focus.
    #[must_use]
    pub fn selected_idle(self) -> Style {
        if self.monochrome() {
            return Style::default().add_modifier(Modifier::UNDERLINED);
        }
        let style = self.text();
        self.tokens()
            .surface
            .map_or(style, |colour| style.bg(colour))
    }

    #[must_use]
    pub fn surface(self) -> Style {
        self.tokens()
            .surface
            .map_or_else(Style::default, |colour| Style::default().bg(colour))
    }

    #[must_use]
    pub fn success(self) -> Style {
        paint(self.tokens().success)
    }

    #[must_use]
    pub fn warning(self) -> Style {
        paint(self.tokens().warning)
    }

    #[must_use]
    pub fn danger(self) -> Style {
        let style = paint(self.tokens().danger);
        if self.monochrome() {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        }
    }

    #[must_use]
    pub fn notice(self) -> Style {
        paint(self.tokens().notice)
    }

    /// A key hint such as the `⏎` in "⏎ open".
    #[must_use]
    pub fn key(self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }

    /// A filled badge such as a status chip or a button.
    #[must_use]
    pub fn chip(self, role: Style) -> Style {
        if self.monochrome() {
            return Style::default().add_modifier(Modifier::REVERSED);
        }
        let tokens = self.tokens();
        let colour = role.fg.or(tokens.accent);
        let style = Style::default().add_modifier(Modifier::BOLD);
        let style = colour.map_or(style, |fill| style.bg(fill));
        tokens.accent_text.map_or(style, |text| style.fg(text))
    }
}

/// Terminals without true colour get the named-colour palette instead.
fn full_colour_or_ansi(tokens: Tokens) -> Tokens {
    if true_colour() { tokens } else { ANSI }
}

fn paint(colour: Option<Color>) -> Style {
    colour.map_or_else(Style::default, |fg| Style::default().fg(fg))
}

/// `COLORTERM=truecolor` (or `24bit`) is the common signal; without it, named colours are safer.
/// Read once: styles are built many times per frame.
fn true_colour() -> bool {
    static TRUE_COLOUR: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRUE_COLOUR.get_or_init(|| {
        std::env::var("COLORTERM")
            .is_ok_and(|value| value.eq_ignore_ascii_case("truecolor") || value == "24bit")
    })
}
