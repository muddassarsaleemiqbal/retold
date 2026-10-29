//! A financial diff is visible before the person confirms the exact command.
use super::super::{app::App, preview::Preview, store::View};
use super::{
    Layer,
    widgets::{self, Emphasis, panel},
};
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};

const WIDTH: u16 = 96;
const HEIGHT: u16 = 36;

pub fn layers(app: &App, preview: &Preview, area: Rect) -> Vec<Layer> {
    let rect = widgets::centered(area, WIDTH, HEIGHT);
    let mut lines = vec![Line::from(preview.form.command_line()), Line::default()];
    if super::super::defaults::applies(&preview.form) {
        lines.extend(super::create::summary(&preview.form));
    }
    match app.store.view(&preview.words) {
        View::Loading => lines.extend(widgets::loading(app.now, "Calculating changes…", app.theme)),
        View::Failed(payload) => lines.extend(widgets::payload_lines(payload, app.theme)),
        View::Ready {
            refreshing: true, ..
        } => lines.extend(widgets::loading(
            app.now,
            "Refreshing changes before review…",
            app.theme,
        )),
        View::Ready { contents, .. } => lines.extend(ready_lines(contents, app.theme)),
    }
    let title = if super::super::defaults::applies(&preview.form) {
        "Review new draft"
    } else {
        "Review change"
    };
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((preview.scroll, 0))
        .block(panel(title, Emphasis::Focused, app.theme));
    vec![Layer::Blank(rect), Layer::text(paragraph, rect)]
}

fn ready_lines(
    contents: &serde_json::Value,
    theme: super::super::theme::Theme,
) -> Vec<Line<'static>> {
    let text = crate::human::render(contents)
        .or_else(|| contents["text"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "Preview unavailable.".into());
    text.lines()
        .map(|line| Line::from(line.to_owned()))
        .chain(super::create::route_lines(contents, theme))
        .chain([Line::from(
            "Enter applies to current state · Esc edits · No execution authority is granted.",
        )])
        .collect()
}
