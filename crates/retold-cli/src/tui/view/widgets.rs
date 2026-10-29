//! Small drawing helpers every screen shares, so panels, chips and hints look the same
//! everywhere.
use unicode_segmentation::UnicodeSegmentation;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding};
use retold_app::protocol::{Diagnostic, Payload};

use super::super::catalog::Risk;
use super::super::input::Input;
use super::super::theme::Theme;

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_FRAME_MILLIS: u64 = 80;

/// Whether a panel or row has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Focused,
    Quiet,
}

/// How a list row is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Plain,
    /// Selected in the pane that has focus.
    Selected,
    /// Selected in a pane without focus.
    Remembered,
}

impl Row {
    /// How row `index` looks when `selected` is the chosen row.
    #[must_use]
    pub const fn at(index: usize, selected: usize, emphasis: Emphasis) -> Self {
        match (index == selected, emphasis) {
            (true, Emphasis::Focused) => Self::Selected,
            (true, Emphasis::Quiet) => Self::Remembered,
            (false, _) => Self::Plain,
        }
    }

    #[must_use]
    pub fn style(self, theme: Theme) -> Style {
        match self {
            Self::Plain => theme.text(),
            Self::Selected => theme.selected(),
            Self::Remembered => theme.selected_idle(),
        }
    }

    /// Secondary text: quiet unless the row is highlighted.
    #[must_use]
    pub fn secondary(self, theme: Theme) -> Style {
        match self {
            Self::Plain => theme.muted(),
            Self::Selected | Self::Remembered => self.style(theme),
        }
    }
}

/// A rounded panel; the focused one gets the accent border and a bright title.
#[must_use]
pub fn panel(title: &str, emphasis: Emphasis, theme: Theme) -> Block<'static> {
    let (border, heading) = match emphasis {
        Emphasis::Focused => (theme.border_focus(), theme.title()),
        Emphasis::Quiet => (theme.border(), theme.strong()),
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .padding(Padding::horizontal(1));
    if title.is_empty() {
        block
    } else {
        block.title(Span::styled(format!(" {title} "), heading))
    }
}

#[must_use]
pub fn spinner(now: u64) -> &'static str {
    let frames = u64::try_from(SPINNER_FRAMES.len()).unwrap_or(1);
    let frame = (now / SPINNER_FRAME_MILLIS) % frames;
    let index = usize::try_from(frame).unwrap_or_default();
    SPINNER_FRAMES.get(index).copied().unwrap_or("·")
}

/// ` read only ` and friends, coloured by how careful the person should be.
#[must_use]
pub fn risk_chip(risk: Risk, theme: Theme) -> Span<'static> {
    Span::styled(
        format!(" {} ", risk.label()),
        theme.chip(risk_role(risk, theme)),
    )
}

#[must_use]
pub fn risk_role(risk: Risk, theme: Theme) -> Style {
    match risk {
        Risk::Read => theme.notice(),
        Risk::Write => theme.accent(),
        Risk::Spend => theme.warning(),
        Risk::Destructive => theme.danger(),
    }
}

/// `⏎ open  / filter` with keys bright and words quiet.
#[must_use]
pub fn hints(pairs: &[(&str, &str)], theme: Theme) -> Line<'static> {
    let spans = pairs.iter().flat_map(|(key, label)| {
        [
            Span::styled(format!(" {key} "), theme.key()),
            Span::styled(format!("{label} "), theme.muted()),
        ]
    });
    Line::from(spans.collect::<Vec<_>>())
}

/// Text lines in the theme's text colour.
#[must_use]
pub fn paragraph_lines(text: &str, theme: Theme) -> Vec<Line<'static>> {
    text.lines()
        .map(|line| Line::from(Span::styled(clean(line), theme.text())))
        .collect()
}

/// Replaces control characters so saved text cannot move the cursor or change colours.
#[must_use]
pub fn clean(text: &str) -> String {
    text.chars()
        .map(|letter| if letter.is_control() { ' ' } else { letter })
        .collect()
}

/// A payload as lines: results in plain text, refusals as a card with the fix.
#[must_use]
pub fn payload_lines(payload: &Payload, theme: Theme) -> Vec<Line<'static>> {
    match payload {
        Payload::Error { diagnostic, .. } => error_lines(diagnostic, theme),
        Payload::Result { .. } | Payload::Event(_) => {
            let text = crate::output::plain(payload).unwrap_or_else(|error| format!("{error}"));
            paragraph_lines(&text, theme)
        }
    }
}

fn error_lines(diagnostic: &Diagnostic, theme: Theme) -> Vec<Line<'static>> {
    let code = diagnostic.code.as_str();
    let heading = Line::from(vec![
        Span::styled(format!(" {code} "), theme.chip(theme.danger())),
        Span::raw("  "),
        Span::styled(clean(&diagnostic.message), theme.strong()),
    ]);
    let fix = Line::from(vec![
        Span::styled("How to fix  ", theme.muted()),
        Span::styled(clean(diagnostic.code.explanation()), theme.text()),
    ]);
    let mut lines = vec![heading, Line::default(), fix];
    lines.extend(suggestion_lines(diagnostic, theme));
    lines
}

fn suggestion_lines(diagnostic: &Diagnostic, theme: Theme) -> Vec<Line<'static>> {
    let commands: Vec<String> = diagnostic
        .suggested_actions
        .iter()
        .map(|words| words.join(" "))
        .filter(|command| command != "retold --help")
        .collect();
    if commands.is_empty() {
        return Vec::new();
    }
    let listed = commands
        .into_iter()
        .map(|command| Line::from(Span::styled(format!("  $ {command}"), theme.accent())));
    [
        Line::default(),
        Line::from(Span::styled("Try", theme.muted())),
    ]
    .into_iter()
    .chain(listed)
    .collect()
}

/// A loading line with a spinner.
#[must_use]
pub fn loading(now: u64, what: &str, theme: Theme) -> Vec<Line<'static>> {
    vec![Line::from(vec![
        Span::styled(format!("{} ", spinner(now)), theme.accent()),
        Span::styled(String::from(what), theme.muted()),
    ])]
}

/// An empty state: one quiet, centred sentence.
#[must_use]
pub fn empty(text: &str, theme: Theme) -> Vec<Line<'static>> {
    vec![
        Line::default(),
        Line::from(Span::styled(String::from(text), theme.muted())).centered(),
    ]
}

/// The visible part of an input that fits `width`, and where the cursor sits in it.
#[must_use]
pub fn input_view(input: &Input, width: u16) -> (String, u16) {
    let letters: Vec<&str> = input.text().graphemes(true).collect();
    let cursor = input.cursor().min(letters.len());
    let room = usize::from(width.saturating_sub(1));
    let (start, before_width) = input_start(&letters, cursor, room);
    let mut visible = String::new();
    let mut used = 0;
    for letter in letters.iter().skip(start) {
        let size = Span::raw(*letter).width();
        if used + size > room {
            break;
        }
        used += size;
        visible.push_str(letter);
    }
    (visible, u16::try_from(before_width).unwrap_or_default())
}

fn input_start(letters: &[&str], cursor: usize, room: usize) -> (usize, usize) {
    let mut start = cursor;
    let mut before_width = 0;
    while start > 0 {
        let previous = start.saturating_sub(1);
        let size = Span::raw(letters[previous]).width();
        if before_width + size > room {
            break;
        }
        before_width += size;
        start = previous;
    }
    (start, before_width)
}

/// A rectangle of at most `width` × `height` in the middle of `area`.
#[must_use]
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [_, middle, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height.min(area.height)),
        Constraint::Fill(1),
    ])
    .areas(area);
    let [_, center, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width.min(area.width)),
        Constraint::Fill(1),
    ])
    .areas(middle);
    center
}

/// `label` padded to `width` columns, cut with `…` when too long.
#[must_use]
pub fn fit(label: &str, width: usize) -> String {
    let count = label.chars().count();
    if count <= width {
        return format!("{label:<width$}");
    }
    let kept: String = label.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// A thin progress bar such as `▰▰▰▱▱▱▱`.
#[must_use]
pub fn meter(done: usize, total: usize, theme: Theme) -> Vec<Span<'static>> {
    const CELLS: usize = 14;
    let filled = done
        .saturating_mul(CELLS)
        .checked_div(total)
        .unwrap_or_default()
        .min(CELLS);
    vec![
        Span::styled("▰".repeat(filled), theme.success()),
        Span::styled("▱".repeat(CELLS.saturating_sub(filled)), theme.faint()),
    ]
}

/// Width in columns of text, as the terminal draws it.
#[must_use]
pub fn columns(text: &str) -> u16 {
    u16::try_from(Span::raw(text.to_owned()).width()).unwrap_or(u16::MAX)
}

/// Inner width of a bordered, padded panel.
#[must_use]
pub const fn inner_width(area: Rect) -> usize {
    const FRAME_COLUMNS: u16 = 4;
    area.width.saturating_sub(FRAME_COLUMNS) as usize
}

/// Rows a bordered panel can show inside its border.
#[must_use]
pub const fn inner_rows(area: Rect) -> usize {
    const FRAME_ROWS: u16 = 2;
    area.height.saturating_sub(FRAME_ROWS) as usize
}
