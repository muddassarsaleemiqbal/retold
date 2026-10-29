//! Setup presents one decision at a time and makes skipped steps explicit.
use super::super::{app::App, onboarding};
use super::{
    Layer,
    widgets::{self, Emphasis, Row, panel},
};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
const WIDTH: u16 = 82;
const HEIGHT: u16 = 20;

pub fn layers(app: &App, selected: usize, area: Rect) -> Vec<Layer> {
    let rect = widgets::centered(area, WIDTH, HEIGHT);
    let theme = app.theme;
    let mut lines = vec![
        Line::from(Span::styled("Make Retold yours", theme.strong())),
        Line::from("Choose a step. You can skip any step and return with F2."),
        Line::default(),
    ];
    lines.extend(
        onboarding::STEPS
            .iter()
            .enumerate()
            .map(|(index, (title, _))| {
                let done = onboarding::completed(&app.store, title);
                let mark = if done { "✓" } else { "○" };
                let style = Row::at(index, selected, Emphasis::Focused).style(theme);
                Line::from(Span::styled(format!(" {mark}  {title}"), style))
            }),
    );
    lines.extend([
        Line::default(),
        Line::from("Account starts at $0. Set finite limits in Money before paid work."),
    ]);
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel("Getting started", Emphasis::Focused, theme));
    vec![Layer::Blank(rect), Layer::text(paragraph, rect)]
}
