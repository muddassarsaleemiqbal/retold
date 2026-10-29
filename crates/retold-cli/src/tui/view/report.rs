//! Health checks and storage groups use readable result rows rather than raw data.
use super::super::{
    app::App,
    report::Report,
    store::{self, View},
};
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

pub fn layer(app: &App, report: &Report, area: Rect) -> Layer {
    let lines = match app.store.view(&store::words(report.command)) {
        View::Loading => widgets::loading(app.now, "Inspecting workspace…", app.theme),
        View::Failed(payload) => widgets::payload_lines(payload, app.theme),
        View::Ready { contents, .. } if report == &super::super::report::BACKUPS => {
            backup_lines(contents)
        }
        View::Ready { contents, .. } => report_lines(app, contents),
    };
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.landing().detail_scroll, 0))
        .block(panel(report.title, Emphasis::Focused, app.theme));
    Layer::text(paragraph, area)
}
fn report_lines(app: &App, contents: &Value) -> Vec<Line<'static>> {
    let checks = contents["report"]["checks"].as_array();
    if let Some(checks) = checks {
        return checks
            .iter()
            .flat_map(|check| check_lines(app, check))
            .collect();
    }
    let storage = contents.get("groups").is_some();
    if storage {
        return storage_lines(contents);
    }
    crate::human::render(contents)
        .unwrap_or_else(|| contents.to_string())
        .lines()
        .map(|line| Line::from(line.to_owned()))
        .collect()
}

fn storage_lines(contents: &Value) -> Vec<Line<'static>> {
    let heading = format!(
        "{} files · {} bytes · {} bytes in intermediate work",
        contents["total_files"], contents["total_bytes"], contents["reclaimable_bytes"]
    );
    let rows = contents["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|group| {
            Line::from(format!(
                "{} · {} files · {} bytes",
                crate::human::text(group, "name"),
                group["files"],
                group["bytes"]
            ))
        });
    [
        Line::from(heading),
        Line::from("b backup · v verify backup · f review repairs · Ctrl-R refresh"),
        Line::default(),
    ]
    .into_iter()
    .chain(rows)
    .collect()
}
fn check_lines(app: &App, check: &Value) -> Vec<Line<'static>> {
    let status = crate::human::text(check, "status");
    let style = match status.as_str() {
        "ok" => app.theme.success(),
        "warn" => app.theme.warning(),
        _ => app.theme.danger(),
    };
    let fixable = check["fixable"] == true;
    let repair = if fixable { " · repair available" } else { "" };
    vec![
        Line::from(Span::styled(
            format!("{status} · {}{repair}", crate::human::text(check, "name")),
            style,
        )),
        Line::from(crate::human::text(check, "message")),
        Line::default(),
    ]
}

fn backup_lines(contents: &Value) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from("Account recovery backup"),
        Line::from(format!(
            "Account: {}",
            crate::human::text(contents, "account_id")
        )),
        Line::default(),
        Line::from(
            "b  Create backup — choose a new folder, then review its contents and active owners.",
        ),
        Line::from(
            "v  Verify backup — choose an existing folder to check its checksum and recorded contents.",
        ),
        Line::default(),
        Line::from("Includes accounting, limits, jobs and unresolved financial evidence."),
    ];
    lines.extend(
        retold_app::account::backup::EXCLUDED_ITEMS
            .iter()
            .map(|item| Line::from(format!("Excluded: {item}"))),
    );
    lines.extend([
        Line::default(),
        Line::from("Backups may be stored anywhere. Use v to inspect a saved backup; this screen is not an inventory."),
        Line::from("Workspace media needs a separate backup in System → Storage."),
    ]);
    lines
}
