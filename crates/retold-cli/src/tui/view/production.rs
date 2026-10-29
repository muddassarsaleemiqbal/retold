//! Plan, allowance and blockers are reviewed together before execution.
use super::super::{app::App, production::Review, store::View};
use super::{
    Layer,
    widgets::{self, Emphasis, panel},
};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use serde_json::Value;

const WIDTH: u16 = 100;
const HEIGHT: u16 = 30;

pub fn layers(app: &App, review: &Review, area: Rect) -> Vec<Layer> {
    let rect = widgets::centered(area, WIDTH, HEIGHT);
    let theme = app.theme;
    let mut lines = vec![Line::from(Span::styled(
        review.form.action.label(),
        theme.strong(),
    ))];
    match app.store.view(&review.words) {
        View::Loading => lines.extend(widgets::loading(
            app.now,
            "Preparing the exact plan…",
            theme,
        )),
        View::Failed(payload) => lines.extend(widgets::payload_lines(payload, theme)),
        View::Ready {
            refreshing: true, ..
        } => lines.extend(widgets::loading(
            app.now,
            "Refreshing the exact plan before review…",
            theme,
        )),
        View::Ready { contents, .. } => lines.extend(plan_lines(app, review, contents)),
    }
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((review.scroll, 0))
        .block(panel("Review production plan", Emphasis::Focused, theme));
    vec![Layer::Blank(rect), Layer::text(paragraph, rect)]
}

fn plan_lines(app: &App, review: &Review, plan: &Value) -> Vec<Line<'static>> {
    let theme = app.theme;
    let contents = plan;
    let plan = super::super::production::plan(contents);
    let authority = &plan["authority"];
    let usd = authority["usd_micros"].as_u64().unwrap_or_default();
    let requests = authority["requests"].as_u64().unwrap_or_default();
    let hash = plan["plan_hash"]
        .as_str()
        .unwrap_or("not supplied by this operation");
    let summary = [
        Line::default(),
        Line::from(format!("Plan  {hash}")),
        Line::from(format!(
            "New paid allowance  {} · {requests} requests",
            crate::human::usd(usd)
        )),
        Line::from("Account limits also apply. Saved work is reused when valid."),
        Line::default(),
    ];
    let runnable = review.executable(contents);
    summary
        .into_iter()
        .chain(list_section("Stages", &authority["stages"]))
        .chain(routes(&authority["routes"]))
        .chain(series_parts(plan))
        .chain(blockers(app, plan))
        .chain([Line::default(), run_line(runnable.as_ref(), theme)])
        .collect()
}

/// The run hint, or why the plan cannot run yet.
fn run_line(
    runnable: Option<&super::super::form::Form>,
    theme: super::super::theme::Theme,
) -> Line<'static> {
    let (label, style) = if runnable.is_some() {
        ("  Run reviewed plan  ⏎  ·  esc edit", theme.success())
    } else {
        (
            "Blocked · correct the issue, then edit and review again",
            theme.warning(),
        )
    };
    Line::from(Span::styled(label, style))
}

fn list_section(title: &str, values: &Value) -> Vec<Line<'static>> {
    let names: Vec<&str> = values
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    vec![Line::from(format!("{title}  {}", names.join(" → ")))]
}

fn routes(value: &Value) -> Vec<Line<'static>> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .map(|(task, provider)| {
            Line::from(format!(
                "  {task}  {}",
                provider.as_str().unwrap_or("unknown")
            ))
        })
        .collect()
}

fn blockers(app: &App, plan: &Value) -> Vec<Line<'static>> {
    plan["blockers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|blocker| {
            let message = blocker["message"]
                .as_str()
                .unwrap_or("Inspect this blocker before continuing.");
            Line::from(Span::styled(format!("! {message}"), app.theme.danger()))
        })
        .collect()
}

fn series_parts(plan: &Value) -> Vec<Line<'static>> {
    plan["part_plans"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|part| {
            let id = part[0].as_str().unwrap_or("unknown part");
            let authority = &part[1]["authority"];
            let hash = part[1]["plan_hash"].as_str().unwrap_or("unknown hash");
            let usd = authority["usd_micros"].as_u64().unwrap_or_default();
            let requests = authority["requests"].as_u64().unwrap_or_default();
            [
                Line::from(format!(
                    "{id} · {} · {requests} requests",
                    crate::human::usd(usd)
                )),
                Line::from(format!("  {hash}")),
            ]
        })
        .collect()
}
