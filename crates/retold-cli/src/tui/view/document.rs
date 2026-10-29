//! Schema fields are selected in a list; edits keep type details out of the primary input.
use super::super::{
    app::App,
    document::{self, Editor},
    page::Reading,
    store::View,
};
use super::{
    Layer,
    widgets::{self, Emphasis, Row, panel},
};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

const EDIT_WIDTH: u16 = 92;
const EDIT_HEIGHT: u16 = 16;
const INPUT_INSET: u16 = 4;

pub fn layer(app: &App, reading: &Reading, body: Rect) -> Layer {
    let lines = match app.store.view(&reading.words) {
        View::Loading => widgets::loading(app.now, "Loading configuration fields…", app.theme),
        View::Failed(payload) => widgets::payload_lines(payload, app.theme),
        View::Ready { contents, .. } => field_lines(app, (reading, &contents["document"]), body),
    };
    Layer::text(
        Paragraph::new(lines).block(panel(
            "Configuration · Enter edit · Ins add · Del remove · J/K move",
            Emphasis::Focused,
            app.theme,
        )),
        body,
    )
}
fn field_lines(
    app: &App,
    source: (&Reading, &serde_json::Value),
    body: Rect,
) -> Vec<Line<'static>> {
    let (reading, document) = source;
    let selected = usize::from(reading.scroll);
    let visible = widgets::inner_rows(body);
    let first = super::super::landing::offset(selected, visible);
    document::fields(document)
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .map(|(index, field)| {
            let value = if field.missing {
                "(not set)".into()
            } else {
                field
                    .value
                    .as_str()
                    .map_or_else(|| field.value.to_string(), str::to_owned)
            };
            let text = widgets::fit(
                &format!("{}  {value}", field.path.trim_start_matches('/')),
                widgets::inner_width(body),
            );
            Line::from(Span::styled(
                text,
                Row::at(index, selected, Emphasis::Focused).style(app.theme),
            ))
        })
        .collect()
}
pub fn edit_layers(app: &App, editor: &Editor, area: Rect) -> Vec<Layer> {
    let rect = widgets::centered(area, EDIT_WIDTH, EDIT_HEIGHT);
    let lines = edit_lines(editor, rect, app.theme);
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel(
            "Edit configuration field",
            Emphasis::Focused,
            app.theme,
        ));
    vec![Layer::Blank(rect), Layer::text(paragraph, rect)]
}

fn edit_lines(
    editor: &Editor,
    rect: Rect,
    theme: super::super::theme::Theme,
) -> Vec<Line<'static>> {
    let field = editor.form.value("field").unwrap_or_default();
    let before = current_value(editor, rect);
    let input = widgets::input_view(&editor.input, rect.width.saturating_sub(INPUT_INSET)).0;
    let problem = editor.problem.clone().unwrap_or_default();
    let hint = edit_hint(editor);
    vec![
        Line::from(field),
        Line::from(format!("Current: {before}")),
        Line::default(),
        Line::from(format!("› {input}")),
        Line::from(hint),
        Line::from(Span::styled(problem, theme.danger())),
        Line::from("Enter reviews changes · Esc cancels"),
    ]
}

fn current_value(editor: &Editor, rect: Rect) -> String {
    let before = if editor.missing {
        "Not set; the input is a suggestion. Review before adding.".into()
    } else {
        editor
            .original
            .as_str()
            .map_or_else(|| editor.original.to_string(), str::to_owned)
    };
    widgets::fit(
        &before,
        widgets::inner_width(rect).saturating_sub("Current: ".len()),
    )
}
const fn edit_hint(editor: &Editor) -> &'static str {
    let adding = editor.append_to.is_some();
    if adding {
        "Adds one item; the selected item is a starting template."
    } else {
        "Text is unquoted. Lists and sections use JSON arrays or objects."
    }
}
