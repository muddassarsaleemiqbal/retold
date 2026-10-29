//! Every target remains visible in a scrollable batch confirmation.
use super::super::{app::App, page::Batch};
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
const HEIGHT: u16 = 28;
pub fn layers(app: &App, batch: &Batch, area: Rect) -> Vec<Layer> {
    let forms = &batch.forms;
    let rect = widgets::centered(area, WIDTH, HEIGHT);
    let title = forms
        .first()
        .map_or_else(String::new, |form| form.action.label());
    let mut lines = vec![
        Line::from(format!("{title} · {} selected videos", forms.len())),
        Line::from("Enter applies to every listed video · Esc cancels"),
        Line::from(
            "Each result is kept separately; failures preserve other results. Deletion uses trash.",
        ),
        Line::default(),
    ];
    lines.extend(forms.iter().map(|form| Line::from(form.command_line())));
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((batch.scroll, 0))
        .block(panel("Review batch change", Emphasis::Focused, app.theme));
    vec![Layer::Blank(rect), Layer::text(paragraph, rect)]
}
