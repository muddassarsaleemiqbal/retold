//! Directory list with a bounded text/media preview; all contents arrive asynchronously.
use super::super::{app::App, files::Browser, theme::Theme};
use super::{
    Layer,
    widgets::{self, Emphasis, Row, panel},
};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use retold_app::protocol::Payload;
use serde_json::Value;

const WIDTH: u16 = 120;
const HEIGHT: u16 = 34;
/// Narrower browsers show the list alone.
const PREVIEW_AT: u16 = 100;
const HEADER_ROWS: u16 = 3;
const FOOTER_ROWS: u16 = 2;
/// The list's share of the browser beside the preview.
const LIST_SHARE: u16 = 45;
const BROWSER_KEYS: &str = "↑↓ select · enter open/choose · backspace parent\nF4 choose folder · F6 edit · o open · space audio · esc back";

pub fn layers(app: &App, browser: &Browser, area: Rect) -> Vec<Layer> {
    let rect = widgets::centered(area, WIDTH, HEIGHT);
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(HEADER_ROWS),
        Constraint::Min(1),
        Constraint::Length(FOOTER_ROWS),
    ])
    .areas(rect);
    let wide = rect.width >= PREVIEW_AT;
    let (list, preview) = if wide {
        super::split_at(body, LIST_SHARE)
    } else {
        (body, None)
    };
    let keys = Paragraph::new(BROWSER_KEYS).style(app.theme.muted());
    let side = preview.map(|area| Layer::text(preview_text(app, browser, area), area));
    vec![
        Layer::Blank(rect),
        Layer::text(heading(app, browser), header),
        Layer::text(listing(app, browser, list), list),
        Layer::text(keys, footer),
    ]
    .into_iter()
    .chain(side)
    .collect()
}

fn heading(app: &App, browser: &Browser) -> Paragraph<'static> {
    let path = browser.path.to_string_lossy().into_owned();
    Paragraph::new(vec![
        Line::from(Span::styled(path, app.theme.strong())),
        Line::from(format!("Filter: {}", browser.query.text())),
        Line::from(Span::styled(
            browser.notice().unwrap_or_default(),
            app.theme.warning(),
        )),
    ])
}

fn entry_line(entry: &Value, row: Row, theme: Theme) -> Line<'static> {
    let folder = entry["directory"] == true;
    let marker = if folder { "▸" } else { " " };
    let name = entry["name"].as_str().unwrap_or_default();
    Line::from(Span::styled(format!("{marker} {name}"), row.style(theme)))
}

fn entry_lines(app: &App, browser: &Browser, area: Rect) -> Vec<Line<'static>> {
    let rows = widgets::inner_rows(area);
    let first = super::super::landing::offset(browser.selected, rows);
    browser
        .entries()
        .iter()
        .enumerate()
        .skip(first)
        .take(rows)
        .map(|(index, entry)| {
            let row = Row::at(index, browser.selected, Emphasis::Focused);
            entry_line(entry, row, app.theme)
        })
        .collect()
}

fn listing(app: &App, browser: &Browser, area: Rect) -> Paragraph<'static> {
    let lines = match &browser.listing {
        None => widgets::loading(app.now, "Reading directory…", app.theme),
        Some(payload @ Payload::Error { .. }) => widgets::payload_lines(payload, app.theme),
        Some(_) => entry_lines(app, browser, area),
    };
    let lines = if lines.is_empty() {
        vec![Line::from(
            "No matching files. Type to filter; ctrl-u clears.",
        )]
    } else {
        lines
    };
    let partial = browser.notice().is_some();
    let title = if partial { "Files · partial" } else { "Files" };
    Paragraph::new(lines).block(panel(title, Emphasis::Focused, app.theme))
}

/// The decoded image, when there is one and the theme can show colour.
fn preview_image<'a>(app: &App, browser: &'a Browser) -> Option<&'a Value> {
    let Some(Payload::Result { contents, .. }) = &browser.preview else {
        return Option::default();
    };
    let colour = app.theme != Theme::NoColor;
    contents.get("image").filter(|_| colour)
}

fn preview_text(app: &App, browser: &Browser, area: Rect) -> Paragraph<'static> {
    let block = panel("Preview", Emphasis::Quiet, app.theme);
    if let Some(image) = preview_image(app, browser) {
        let inside = area.inner(Margin::new(2, 1));
        return Paragraph::new(super::super::raster::lines(image, inside)).block(block);
    }
    let lines = browser.preview.as_ref().map_or_else(
        || vec![Line::from("Move to a file to preview it.")],
        |payload| widgets::payload_lines(payload, app.theme),
    );
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(block)
}
