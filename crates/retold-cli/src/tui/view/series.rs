//! A saved parts matrix; completion and video links come from the series registry.
use super::super::{app::App, page::Reading, store::View, theme::Theme};
use super::{
    Layer,
    widgets::{self, Emphasis, Row, panel},
};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

pub fn layer(app: &App, reading: &Reading, body: Rect) -> Layer {
    let lines = match app.store.view(&reading.words) {
        View::Loading => widgets::loading(app.now, "Loading series parts…", app.theme),
        View::Failed(payload) => widgets::payload_lines(payload, app.theme),
        View::Ready { contents, .. } => matrix(app, (reading, contents), body),
    };
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel(
            "Series · Enter video · n draft · l link · p plan · r run · a add",
            Emphasis::Focused,
            app.theme,
        ));
    Layer::text(paragraph, body)
}

fn matrix(app: &App, source: (&Reading, &serde_json::Value), body: Rect) -> Vec<Line<'static>> {
    let (reading, contents) = source;
    let parts = app.series_parts();
    let selected = usize::from(reading.scroll);
    let visible = widgets::inner_rows(body).saturating_sub(2) / 2;
    let first = super::super::landing::offset(selected, visible);
    let rows = parts
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .flat_map(|(index, part)| {
            let progress = &contents["parts_progress"][part["id"].as_str().unwrap_or_default()];
            let started = part["video_ref"].is_string();
            let evidence = if started {
                progress_line(progress, app.theme)
            } else {
                Line::styled(
                    "  No video linked · n creates a draft · l links an existing video",
                    app.theme.muted(),
                )
            };
            [part_line(app, (index, selected), part), evidence]
        });
    [summary(app, &contents["series"], &parts), Line::default()]
        .into_iter()
        .chain(rows)
        .collect()
}

fn summary(app: &App, series: &serde_json::Value, parts: &[serde_json::Value]) -> Line<'static> {
    let title = crate::human::text(series, "work");
    let budget = crate::human::usd(crate::human::amount(&series["budget_usd_micros"]));
    let total = parts.len();
    let complete = parts
        .iter()
        .filter(|part| part["completed"] == true)
        .count();
    let spans = std::iter::once(Span::raw(format!("{title} · budget {budget} · ")))
        .chain(widgets::meter(complete, total, app.theme))
        .chain([Span::raw(format!(" {complete}/{total} parts complete"))]);
    Line::from(spans.collect::<Vec<_>>())
}

fn part_line(app: &App, selection: (usize, usize), part: &serde_json::Value) -> Line<'static> {
    let (index, selected) = selection;
    let complete = part["completed"] == true;
    let state = if complete { "complete" } else { "unfinished" };
    let reference = part["video_ref"].as_str().unwrap_or("no video yet");
    let text = format!(
        "{} · {} · {state} · {reference}",
        crate::human::text(part, "id"),
        crate::human::text(part, "title")
    );
    Line::from(Span::styled(
        text,
        Row::at(index, selected, Emphasis::Focused).style(app.theme),
    ))
}

fn progress_line(progress: &serde_json::Value, theme: Theme) -> Line<'static> {
    let Some(stages) = progress.as_array() else {
        return Line::styled(
            "  Stage evidence unavailable · open the part for details",
            theme.muted(),
        );
    };
    let cells = stages.iter().flat_map(|stage| stage_cell(stage, theme));
    let legend = Span::styled("   ✓ cached · ! repair · · not run", theme.faint());
    Line::from(
        std::iter::once(Span::raw(" "))
            .chain(cells)
            .chain([legend])
            .collect::<Vec<_>>(),
    )
}

fn stage_cell(stage: &serde_json::Value, theme: Theme) -> [Span<'static>; 2] {
    let (glyph, style) = match stage["decision"].as_str() {
        Some("reused_from_cache") => ("✓", theme.success()),
        Some("requires_repair") => ("!", theme.danger()),
        _ => ("·", theme.muted()),
    };
    let name = crate::human::text(stage, "stage");
    [
        Span::raw(" "),
        Span::styled(format!("{glyph} {name}"), style),
    ]
}
