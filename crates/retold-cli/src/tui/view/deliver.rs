//! Ready delivery coverage comes from saved timeline evidence, never inferred from folders.
use super::super::{app::App, page::VideoPage, store::View};
use super::{
    Layer,
    widgets::{self, Emphasis, panel},
};
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};
pub fn layers(app: &App, video: &VideoPage, body: Rect) -> Vec<Layer> {
    let lines = match app.store.view(&video.words()) {
        View::Loading => widgets::loading(app.now, "Loading saved delivery coverage…", app.theme),
        View::Failed(payload) => widgets::payload_lines(payload, app.theme),
        View::Ready { contents, .. } => delivery_lines(contents),
    };
    vec![Layer::text(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((video.scroll, 0))
            .block(panel(
                "Deliver · e reviews export",
                Emphasis::Focused,
                app.theme,
            )),
        body,
    )]
}
fn delivery_lines(contents: &serde_json::Value) -> Vec<Line<'static>> {
    let rows = contents["deliveries"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut lines = delivery_heading(rows);
    lines.extend(delivery_rows(rows));
    lines.push(Line::default());
    lines.push(Line::from(
        "Ready means assembled. Review an export plan to render or save a project.",
    ));
    lines
}
fn delivery_rows(rows: &[serde_json::Value]) -> Vec<Line<'static>> {
    rows.iter()
        .map(|row| {
            Line::from(format!(
                "{} · {} · ready · {}",
                crate::human::text(row, "edition"),
                crate::human::text(row, "format"),
                crate::human::text(row, "id")
            ))
        })
        .collect()
}

fn delivery_heading(rows: &[serde_json::Value]) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from("Edition × format · assembled at ready"),
        Line::default(),
    ];
    if rows.is_empty() {
        lines.push(Line::from(
            "No prepared deliveries. Continue to ready before export.",
        ));
    }
    lines
}
