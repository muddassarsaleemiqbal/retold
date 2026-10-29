//! Creative findings retain source locations and severity in a readable result hierarchy.
use super::super::{app::App, theme::Theme};
use super::{
    Layer,
    widgets::{self, Emphasis, panel},
};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use serde_json::Value;
const MAX_DEPTH: usize = 5;
const MAX_ROWS: usize = 300;

pub fn layer(app: &App, contents: &Value, position: (Rect, u16)) -> Layer {
    let (area, scroll) = position;
    let lines = lines(contents, app.theme);
    Layer::text(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .block(panel("Creative findings", Emphasis::Focused, app.theme)),
        area,
    )
}
fn lines(contents: &Value, theme: Theme) -> Vec<Line<'static>> {
    let rows = walk(contents, "", 0);
    rows.into_iter()
        .take(MAX_ROWS)
        .map(|row| {
            let lower = row.to_lowercase();
            let style = if lower.contains("error") || lower.contains("fail") {
                theme.danger()
            } else if lower.contains("warn") {
                theme.warning()
            } else {
                theme.text()
            };
            Line::from(Span::styled(widgets::clean(&row), style))
        })
        .collect()
}
fn walk(value: &Value, label: &str, depth: usize) -> Vec<String> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    match value {
        Value::Object(fields) => std::iter::once(label.to_owned())
            .chain(fields.iter().flat_map(|(key, value)| {
                walk(value, &super::super::catalog::humanize(key), depth + 1)
            }))
            .take(MAX_ROWS)
            .collect(),
        Value::Array(items) => std::iter::once(format!("{label} · {}", items.len()))
            .chain(
                items
                    .iter()
                    .enumerate()
                    .flat_map(|(index, item)| walk(item, &format!("  {}", index + 1), depth + 1)),
            )
            .take(MAX_ROWS)
            .collect(),
        Value::Null => Vec::new(),
        _ => vec![format!(
            "{label}: {}",
            value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned)
        )],
    }
}
