//! Recorded execution health and offline credential evidence are displayed separately.
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use serde_json::Value;

use super::super::{
    app::App,
    page::Reading,
    store::{self, View},
    theme::Theme,
};
use super::{
    Layer,
    widgets::{self, Emphasis, panel},
};

pub fn layer(app: &App, subject: (&str, u16), area: Rect) -> Layer {
    let (id, scroll) = subject;
    Layer::text(
        Paragraph::new(lines(app, id))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .block(panel(
                &format!("Provider status · {id}"),
                Emphasis::Focused,
                app.theme,
            )),
        area,
    )
}

pub fn lines(app: &App, id: &str) -> Vec<Line<'static>> {
    let health = health_lines(app, id);
    let login = login_lines(app, id);
    health
        .into_iter()
        .chain([Line::default()])
        .chain(login)
        .chain([
            Line::default(),
            Line::from("c configure login · t review a free test · Ctrl-R refresh"),
            Line::from(Span::styled(
                "Offline inspection; no provider requests sent.",
                app.theme.muted(),
            )),
        ])
        .collect()
}

fn health_lines(app: &App, id: &str) -> Vec<Line<'static>> {
    match app.store.view(&store::words("providers health")) {
        View::Loading => widgets::loading(app.now, "Loading recorded health…", app.theme),
        View::Failed(payload) => widgets::payload_lines(payload, app.theme),
        View::Ready { contents, .. } => {
            let record = contents["health"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|record| record["id"].as_str() == Some(id));
            record.map_or_else(
                || widgets::empty("No health record for this provider.", app.theme),
                |record| health_record(record, app.theme),
            )
        }
    }
}

fn health_record(record: &Value, theme: Theme) -> Vec<Line<'static>> {
    let status = record["status"].as_str().unwrap_or("not_checked");
    vec![
        row("Recorded health", label(status), theme),
        row("Observed (UTC)", value(record, "observed_at_utc"), theme),
        row("Retry at (UTC)", value(record, "retry_at_utc"), theme),
        row(
            "Failure streak",
            &record["observation"]["consecutive_failures"]
                .as_u64()
                .unwrap_or(0)
                .to_string(),
            theme,
        ),
        Line::from(Span::styled(
            "Recorded observations do not prove current availability.",
            theme.muted(),
        )),
    ]
}

fn label(status: &str) -> &str {
    const LABELS: [(&str, &str); 7] = [
        ("not_checked", "Not checked"),
        ("cooldown", "Limited · cooling down"),
        ("cooldown_elapsed", "Cooldown elapsed · not retested"),
        ("logged_out", "Logged out"),
        ("ok", "Last check succeeded"),
        ("failing", "Failing"),
        ("unavailable", "Unavailable"),
    ];
    LABELS
        .iter()
        .find(|(code, _)| *code == status)
        .map_or(status, |(_, text)| *text)
}

fn login_lines(app: &App, id: &str) -> Vec<Line<'static>> {
    match app.store.view(&Reading::provider(id).words) {
        View::Loading => widgets::loading(app.now, "Inspecting login offline…", app.theme),
        View::Failed(payload) => widgets::payload_lines(payload, app.theme),
        View::Ready { contents, .. } => login_record(&contents["login_status"], app.theme),
    }
}

fn login_record(record: &Value, theme: Theme) -> Vec<Line<'static>> {
    vec![
        row("Login evidence", value(record, "status"), theme),
        row("Provider kind", value(record, "kind"), theme),
        row("Profile folder", value(record, "profile_dir"), theme),
        row("Key variable", value(record, "credential_variable"), theme),
        Line::from(widgets::clean(value(record, "detail"))),
    ]
}

fn value<'a>(record: &'a Value, field: &str) -> &'a str {
    record[field].as_str().unwrap_or("Not recorded")
}

fn row(label: &str, value: &str, theme: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), theme.muted()),
        Span::styled(widgets::clean(value), theme.strong()),
    ])
}
