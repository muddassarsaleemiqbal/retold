//! Section landings: the dashboard, collection lists with live detail, the actions catalogue
//! and spend.
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use serde_json::Value;

use super::super::app::{self, App};
use super::super::catalog::{Action, Field, FieldKind};
use super::super::collection::{self, Collection, Item};
use super::super::dashboard::{self, Row as DashboardRow};
use super::super::landing::{Tab, offset};
use super::super::store::{self, View};
use super::super::theme::Theme;
use super::widgets::{self, Emphasis, Row, fit, inner_rows, inner_width, panel};
use super::{Layer, Picture, main_emphasis, split, split_at};

/// The dashboard list's share of the body beside "At a glance".
const DASHBOARD_SHARE: u16 = 62;
/// The dashboard label column never takes more than two thirds of a row.
const LABEL_LIMIT_THIRDS: usize = 2;
const THIRDS: usize = 3;
/// Mark, spaces and gap around a dashboard row.
const MARK_COLUMNS: usize = 4;
/// Collection rows give half their width to the label.
const LABEL_HALVES: usize = 2;
const GROUP_WIDTH: usize = 15;
const SPEND_SUMMARY_ROWS: u16 = 6;
const FIELD_LABEL_WIDTH: usize = 18;
const DETAIL_KEY_WIDTH: usize = 14;

pub fn picture(app: &App, body: Rect) -> Picture {
    match app.tab() {
        Tab::Dashboard => Picture::of(dashboard_layers(app, body)),
        Tab::Spend => Picture::of(spend_layers(app, body)),
        Tab::Report(report) => Picture::of(vec![super::report::layer(app, report, body)]),
        Tab::Collection(collection) => collection_picture(app, collection, body),
        Tab::Actions => actions_picture(app, body),
    }
}

// ----- dashboard -----

fn dashboard_layers(app: &App, body: Rect) -> Vec<Layer> {
    let (list, side) = split_at(body, DASHBOARD_SHARE);
    let lines = dashboard_lines(app, list);
    let block = panel(greeting(app), main_emphasis(app), app.theme);
    let main = Layer::text(Paragraph::new(lines).block(block), list);
    let glance = side.map(|side| Layer::text(glance(app), side));
    std::iter::once(main).chain(glance).collect()
}

fn greeting(app: &App) -> &'static str {
    let (done, total) = dashboard::progress(&app.store);
    if done < total {
        "Welcome to Retold"
    } else {
        "Home"
    }
}

#[derive(Clone, Copy)]
struct Columns {
    label: usize,
    detail: usize,
}

/// How one row is painted: its selection state in the theme.
#[derive(Clone, Copy)]
struct Look {
    state: Row,
    theme: Theme,
}

fn dashboard_columns(rows: &[DashboardRow], area: Rect) -> Columns {
    let width = inner_width(area);
    let longest = rows
        .iter()
        .map(|row| row.label.chars().count())
        .max()
        .unwrap_or_default();
    let limit = width.saturating_mul(LABEL_LIMIT_THIRDS) / THIRDS;
    let label = longest.saturating_add(2).min(limit);
    Columns {
        label,
        detail: width.saturating_sub(label.saturating_add(MARK_COLUMNS)),
    }
}

fn dashboard_lines(app: &App, area: Rect) -> Vec<Line<'static>> {
    let rows = app.dashboard_rows();
    let columns = dashboard_columns(&rows, area);
    let selected = app.landing().selected();
    let emphasis = main_emphasis(app);
    let lines = rows.iter().enumerate().flat_map(|(index, row)| {
        let heading = dashboard::starts_group(&rows, index).then(|| group_heading(app, row.group));
        let look = Look {
            state: Row::at(index, selected, emphasis),
            theme: app.theme,
        };
        let line = dashboard_row(row, look, columns);
        heading.into_iter().flatten().chain([line])
    });
    let skip = offset(dashboard::line_of(&rows, selected), inner_rows(area));
    lines.skip(skip).collect()
}

fn group_heading(app: &App, group: &str) -> Vec<Line<'static>> {
    let title = Span::styled(group.to_uppercase(), app.theme.faint());
    let checklist = group == dashboard::CHECKLIST;
    let progress = if checklist {
        progress_spans(app)
    } else {
        Vec::new()
    };
    let heading: Vec<Span<'static>> = std::iter::once(title).chain(progress).collect();
    vec![Line::default(), Line::from(heading)]
}

fn progress_spans(app: &App) -> Vec<Span<'static>> {
    let theme = app.theme;
    let (done, total) = dashboard::progress(&app.store);
    let caption = Span::styled(format!("  {done} of {total} done"), theme.muted());
    std::iter::once(Span::raw("   "))
        .chain(widgets::meter(done, total, theme))
        .chain([caption])
        .collect()
}

fn dashboard_row(row: &DashboardRow, look: Look, columns: Columns) -> Line<'static> {
    let Look { state, theme } = look;
    let (mark, mark_style) = match row.done {
        Some(true) => ("✓", theme.success()),
        Some(false) => ("○", theme.faint()),
        None => ("›", theme.accent()),
    };
    let finished = row.done == Some(true);
    let label_style = match (state, finished) {
        (Row::Plain, true) => theme.muted(),
        _ => state.style(theme),
    };
    let plain = state == Row::Plain;
    let mark_style = if plain {
        mark_style
    } else {
        state.style(theme)
    };
    Line::from(vec![
        Span::styled(format!(" {mark} "), mark_style),
        Span::styled(fit(&row.label, columns.label), label_style),
        Span::styled(
            format!(" {}", fit(&row.detail, columns.detail)),
            state.secondary(theme),
        ),
    ])
}

fn glance(app: &App) -> Paragraph<'static> {
    let theme = app.theme;
    let pair = |label: &str, value: String| {
        Line::from(vec![
            Span::styled(format!("{label:<12}"), theme.muted()),
            Span::styled(value, theme.strong()),
        ])
    };
    let today = dashboard::today(&app.store).unwrap_or_else(|| "account not set up".into());
    let hint = "Press : to search every action, or a number to jump to a section.";
    let lines = vec![
        Line::default(),
        pair("Providers", providers_summary(app)),
        pair("Today", today),
        pair("Videos", videos_summary(app)),
        Line::default(),
        Line::from(Span::styled(hint, theme.faint())),
    ];
    Paragraph::new(lines).wrap(Wrap { trim: true }).block(panel(
        "At a glance",
        Emphasis::Quiet,
        theme,
    ))
}

fn providers_summary(app: &App) -> String {
    let health = dashboard::health(&app.store);
    let marks: Vec<String> = health
        .iter()
        .map(|(id, status)| format!("{id} {}", status_mark(status)))
        .collect();
    if marks.is_empty() {
        "none yet".into()
    } else {
        marks.join("  ")
    }
}

fn videos_summary(app: &App) -> String {
    let listed = app.store.contents(&store::words("video list"));
    let total = listed.and_then(|contents| contents["total"].as_u64());
    total.map_or_else(|| "…".into(), |count| format!("{count}"))
}

fn status_mark(status: &str) -> &'static str {
    match status {
        "ok" => "✓",
        "limited" => "⏳",
        _ => "✗",
    }
}

// ----- collections -----

fn collection_picture(app: &App, collection: &'static Collection, body: Rect) -> Picture {
    let (list, detail) = split(body);
    let items = app.visible_items();
    let lines = collection_lines(app, (collection, &items), list);
    let name = if collection == &collection::DISCOVER {
        "Discover · Offline"
    } else {
        collection.name
    };
    let (title, cursor) = list_title(app, (name, items.len()), list);
    let block = panel(&title, main_emphasis(app), app.theme);
    let failed = matches!(
        app.store.view(&app.collection_words(collection)),
        View::Failed(_)
    );
    let paragraph = Paragraph::new(lines).block(block);
    // Diagnostics are prose; rows are already fitted to the pane.
    let paragraph = if failed {
        paragraph.wrap(Wrap { trim: false })
    } else {
        paragraph
    };
    let main = Layer::text(paragraph, list);
    let side = detail.map(|area| item_detail(app, collection, area));
    let layers = std::iter::once(main).chain(side).collect();
    Picture::with_cursor(layers, cursor)
}

fn collection_lines(app: &App, source: (&Collection, &[Item]), list: Rect) -> Vec<Line<'static>> {
    let (collection, items) = source;
    let waiting = format!("Loading {}…", collection.name.to_lowercase());
    let words = app.collection_words(collection);
    match app.store.view(&words) {
        View::Loading => widgets::loading(app.now, &waiting, app.theme),
        View::Failed(payload) => widgets::payload_lines(payload, app.theme),
        View::Ready { .. } if items.is_empty() => empty_collection(app, collection),
        View::Ready { .. } => item_lines(app, items, list),
    }
}

fn list_title(app: &App, heading: (&str, usize), list: Rect) -> (String, Option<Position>) {
    let (name, count) = heading;
    let title = format!("{name} · {count}");
    let filter = app.landing().filter();
    let filtering = app.landing().filtering;
    if filter.is_empty() && !filtering {
        (title, None)
    } else {
        filter_title(app, &format!("{title} · /"), list)
    }
}

/// The edited grapheme and its neighbours stay visible within the list's border.
fn filter_title(app: &App, prefix: &str, list: Rect) -> (String, Option<Position>) {
    const TITLE_INSET: u16 = 2;
    let available = list.width.saturating_sub(TITLE_INSET.saturating_mul(2));
    let prefix_fits = widgets::columns(prefix) < available;
    let prefix = if prefix_fits { prefix } else { "/" };
    let width = available.saturating_sub(widgets::columns(prefix));
    let (shown, column) = widgets::input_view(&app.landing().cursor().filter, width);
    let filtering = app.landing().filtering;
    let start = list.x.saturating_add(TITLE_INSET);
    let prefix_end = start.saturating_add(widgets::columns(prefix));
    let x = prefix_end.saturating_add(column);
    let visible = filtering && width > 0 && list.height > 0;
    let cursor = visible.then_some(Position::new(x, list.y));
    (format!("{prefix}{shown}"), cursor)
}

fn empty_collection(app: &App, collection: &Collection) -> Vec<Line<'static>> {
    let filtered = !app.landing().filter().is_empty();
    let text = if filtered {
        "Nothing matches the filter. Esc clears it."
    } else {
        collection.empty
    };
    widgets::empty(text, app.theme)
}

fn item_lines(app: &App, items: &[Item], area: Rect) -> Vec<Line<'static>> {
    let selected = app.landing().selected();
    let height = inner_rows(area);
    let width = inner_width(area);
    let label = (width / LABEL_HALVES).max(1);
    let columns = Columns {
        label,
        detail: width.saturating_sub(label).saturating_sub(1),
    };
    let emphasis = main_emphasis(app);
    items
        .iter()
        .enumerate()
        .skip(offset(selected, height))
        .take(height)
        .map(|(index, item)| {
            let look = Look {
                state: Row::at(index, selected, emphasis),
                theme: app.theme,
            };
            let mut shown = item.clone();
            let selected = app.selected_videos.contains(&item.key);
            if selected {
                shown.label = format!("[x] {}", shown.label);
            }
            item_line(&shown, look, columns)
        })
        .collect()
}

fn item_line(item: &Item, look: Look, columns: Columns) -> Line<'static> {
    let Look { state, theme } = look;
    let label = fit(&widgets::clean(&item.label), columns.label);
    let note = fit(&widgets::clean(&item.note), columns.detail);
    Line::from(vec![
        Span::styled(label, state.style(theme)),
        Span::styled(format!(" {note}"), state.secondary(theme)),
    ])
}

fn item_detail(app: &App, collection: &Collection, area: Rect) -> Layer {
    let theme = app.theme;
    let Some(item) = app.selected_item() else {
        let ready = matches!(
            app.store.view(&app.collection_words(collection)),
            View::Ready { .. }
        );
        let hint = collection
            .create
            .filter(|_| ready)
            .map_or("", |_| "Press n to add the first one.");
        let block = panel("", Emphasis::Quiet, theme);
        let lines = widgets::empty(hint, theme)
            .into_iter()
            .chain(music_coverage(app, collection));
        return Layer::text(
            Paragraph::new(lines.collect::<Vec<_>>())
                .wrap(Wrap { trim: false })
                .block(block),
            area,
        );
    };
    let paragraph = Paragraph::new(detail_lines(app, collection, &item))
        .wrap(Wrap { trim: false })
        .scroll((app.landing().detail_scroll, 0))
        .block(panel(&widgets::clean(&item.label), Emphasis::Quiet, theme));
    Layer::text(paragraph, area)
}

fn detail_lines(app: &App, collection: &Collection, item: &Item) -> Vec<Line<'static>> {
    let lines = projected_details(collection, item);
    let projected = !lines.is_empty();
    if projected {
        return lines;
    }
    if collection == &collection::DISCOVER {
        return discovery_detail(item, app.theme);
    }
    if collection == &collection::PROVIDERS {
        return provider_detail(app, item);
    }
    if collection == &collection::HEALTH || collection == &collection::LOGINS {
        return super::provider_status::lines(app, &item.key);
    }
    let shown = collection.detail.map_or_else(
        || raw_lines(&item.raw, app.theme),
        |command| {
            let words = app::detail_words(command, item);
            view_lines(app, app.store.view(&words), "Loading details…")
        },
    );
    shown
        .into_iter()
        .chain(music_coverage(app, collection))
        .collect()
}

fn provider_detail(app: &App, item: &Item) -> Vec<Line<'static>> {
    let mut definition = item.raw.clone();
    if let Some(fields) = definition.as_object_mut() {
        fields.remove("health");
        fields.remove("authentication");
    }
    super::provider_status::lines(app, &item.key)
        .into_iter()
        .chain(raw_lines(&definition, app.theme))
        .chain(serves_lines(app, item))
        .collect()
}

fn music_coverage(app: &App, collection: &Collection) -> Vec<Line<'static>> {
    if collection == &collection::MUSIC {
        super::coverage::lines(app)
    } else {
        Vec::new()
    }
}

fn discovery_detail(item: &Item, theme: Theme) -> Vec<Line<'static>> {
    [
        Line::from("Enter opens an editable provider definition."),
        Line::from("Review the alias, models and options before saving."),
        Line::from("Existing aliases are never replaced."),
        Line::from("Login and models are unchecked. No requests sent."),
        Line::default(),
    ]
    .into_iter()
    .chain(raw_lines(&item.raw, theme))
    .collect()
}

/// Providers have no `show` command; the tasks routed to them complete the picture.
fn serves_lines(app: &App, item: &Item) -> Vec<Line<'static>> {
    let routes = app.store.contents(&store::words("providers route"));
    let serving: Vec<String> = routes
        .and_then(|contents| contents["routing"].as_object())
        .map(|routing| {
            routing
                .iter()
                .filter(|(_, chain)| chain_uses(chain, &item.key))
                .map(|(tier, _)| tier.clone())
                .collect()
        })
        .unwrap_or_default();
    let summary = if serving.is_empty() {
        "not routed yet: open Routes".to_owned()
    } else {
        serving.join(", ")
    };
    vec![
        Line::default(),
        key_value("Serves", &summary, app.theme),
        Line::from("l status · c configure login · t review a free test"),
    ]
}

fn chain_uses(chain: &Value, id: &str) -> bool {
    let entries = chain.as_array().map(Vec::as_slice).unwrap_or_default();
    entries
        .iter()
        .filter_map(Value::as_str)
        .any(|entry| entry.split(':').next() == Some(id))
}

fn key_value(key: &str, value: &str, theme: Theme) -> Line<'static> {
    // A key as wide as its column still keeps one space before the value.
    let mut label = format!("{key:<DETAIL_KEY_WIDTH$}");
    if !label.ends_with(' ') {
        label.push(' ');
    }
    Line::from(vec![
        Span::styled(label, theme.muted()),
        Span::styled(widgets::clean(value), theme.text()),
    ])
}

/// Fields of an item, one per line, for collections without a detail command.
fn raw_lines(raw: &Value, theme: Theme) -> Vec<Line<'static>> {
    let Some(map) = raw.as_object() else {
        return widgets::paragraph_lines(&format!("{raw}"), theme);
    };
    map.iter()
        .map(|(key, value)| {
            let shown = match value {
                Value::Null => "unknown".to_owned(),
                Value::String(text) => text.clone(),
                other => format!("{other}"),
            };
            key_value(&super::super::catalog::humanize(key), &shown, theme)
        })
        .collect()
}

/// Lines for any cached read: a spinner, the result, or the refusal and its fix.
pub fn view_lines(app: &App, view: View<'_>, waiting: &str) -> Vec<Line<'static>> {
    match view {
        View::Loading => widgets::loading(app.now, waiting, app.theme),
        View::Failed(payload) => widgets::payload_lines(payload, app.theme),
        View::Ready { contents, .. } => {
            let payload = retold_app::protocol::Payload::Result {
                outcome: retold_app::protocol::Outcome::Completed,
                contents: contents.clone(),
            };
            widgets::payload_lines(&payload, app.theme)
        }
    }
}

// ----- actions -----

fn actions_picture(app: &App, body: Rect) -> Picture {
    let (list, detail) = split(body);
    let (title, cursor) = list_title(app, ("Actions", app.section_actions().len()), list);
    let block = panel(&title, main_emphasis(app), app.theme);
    let main = Layer::text(Paragraph::new(action_lines(app, list)).block(block), list);
    let side = detail
        .zip(app.selected_action())
        .map(|(area, action)| Layer::text(action_detail(action, app.theme), area));
    let layers = std::iter::once(main).chain(side).collect();
    Picture::with_cursor(layers, cursor)
}

fn action_lines(app: &App, list: Rect) -> Vec<Line<'static>> {
    let actions = app.section_actions();
    if actions.is_empty() {
        return widgets::empty("No action matches.", app.theme);
    }
    let selected = app.landing().selected();
    let height = inner_rows(list);
    let width = inner_width(list);
    let emphasis = main_emphasis(app);
    actions
        .iter()
        .enumerate()
        .skip(offset(selected, height))
        .take(height)
        .map(|(index, (_, action))| {
            let look = Look {
                state: Row::at(index, selected, emphasis),
                theme: app.theme,
            };
            action_line(action, look, width)
        })
        .collect()
}

fn action_line(action: &Action, look: Look, width: usize) -> Line<'static> {
    let Look { state, theme } = look;
    let title_width = width.saturating_sub(GROUP_WIDTH.saturating_add(1));
    let plain = state == Row::Plain;
    let group_style = if plain {
        theme.faint()
    } else {
        state.style(theme)
    };
    Line::from(vec![
        Span::styled(fit(&action.group, GROUP_WIDTH), group_style),
        Span::styled(
            format!(" {}", fit(&action.title, title_width)),
            state.style(theme),
        ),
    ])
}

fn action_detail(action: &Action, theme: Theme) -> Paragraph<'static> {
    let heading = [
        Line::from(Span::styled(action.label(), theme.title())),
        Line::from(vec![widgets::risk_chip(action.risk, theme)]),
        Line::default(),
    ];
    let command = format!("$ retold {}", action.command());
    let lines: Vec<Line<'static>> = heading
        .into_iter()
        .chain(widgets::paragraph_lines(&action.about, theme))
        .chain([
            Line::default(),
            Line::from(Span::styled(command, theme.accent())),
        ])
        .chain(field_lines(action, theme))
        .chain(note_lines(action, theme))
        .chain([Line::default(), opens_line(action, theme)])
        .collect();
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel("", Emphasis::Quiet, theme))
}

fn field_lines(action: &Action, theme: Theme) -> Vec<Line<'static>> {
    let asks_nothing = action.fields.is_empty();
    if asks_nothing {
        return Vec::new();
    }
    let heading = [
        Line::default(),
        Line::from(Span::styled("ASKS FOR", theme.faint())),
    ];
    let fields = action
        .fields
        .iter()
        .map(|field| field_summary(field, theme));
    heading.into_iter().chain(fields).collect()
}

fn note_lines(action: &Action, theme: Theme) -> Vec<Line<'static>> {
    let lines: Vec<Line<'static>> = action
        .notes
        .lines()
        .map(|line| Line::from(Span::styled(widgets::clean(line), theme.muted())))
        .collect();
    if lines.is_empty() {
        return lines;
    }
    std::iter::once(Line::default()).chain(lines).collect()
}

fn opens_line(action: &Action, theme: Theme) -> Line<'static> {
    let immediate = action.runs_immediately();
    let text = if immediate {
        "⏎ shows it now"
    } else {
        "⏎ opens the form"
    };
    Line::from(Span::styled(text, theme.faint()))
}

fn field_summary(field: &Field, theme: Theme) -> Line<'static> {
    let (mark, mark_style) = if field.required {
        ("●", theme.accent())
    } else {
        ("○", theme.faint())
    };
    Line::from(vec![
        Span::styled(format!(" {mark} "), mark_style),
        Span::styled(format!("{:<FIELD_LABEL_WIDTH$}", field.label), theme.text()),
        Span::styled(kind_label(field), theme.muted()),
    ])
}

fn kind_label(field: &Field) -> String {
    match &field.kind {
        FieldKind::Flag => "switch".to_owned(),
        FieldKind::Text => "text".to_owned(),
        FieldKind::Choice(choices) => choices.join(" / "),
    }
}

// ----- spend -----

fn spend_layers(app: &App, body: Rect) -> Vec<Layer> {
    let theme = app.theme;
    let [summary, table] =
        Layout::vertical([Constraint::Length(SPEND_SUMMARY_ROWS), Constraint::Min(1)]).areas(body);
    let (summary_lines, rows) = spend_lines(app);
    let summary_block = panel(app.spend.window(), main_emphasis(app), theme);
    let table_block = panel(
        &format!("By {} · charged + held", app.spend.group()),
        Emphasis::Quiet,
        theme,
    );
    vec![
        Layer::text(Paragraph::new(summary_lines).block(summary_block), summary),
        Layer::text(Paragraph::new(rows).block(table_block), table),
    ]
}

fn spend_lines(app: &App) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let theme = app.theme;
    match app.store.view(&app.spend.words()) {
        View::Ready { contents, .. } => {
            let report = &contents["spend"];
            (spend_summary(report, theme), spend_rows(report, theme))
        }
        other => (view_lines(app, other, "Loading spend…"), Vec::new()),
    }
}

fn spend_summary(report: &Value, theme: Theme) -> Vec<Line<'static>> {
    crate::tui::spend_screen::summary_lines(report)
        .into_iter()
        .map(|line| Line::from(Span::styled(line, theme.text())))
        .collect()
}

fn spend_row_text(row: &crate::tui::spend_screen::SpendRow) -> String {
    format!(
        "{:<24} {:>8} {:>10} {:>12}   {}",
        widgets::clean(&row.key),
        row.attempts,
        row.held_usd,
        row.charged_usd,
        row.headroom
    )
}

fn spend_rows(report: &Value, theme: Theme) -> Vec<Line<'static>> {
    let rows = crate::tui::spend_screen::rows(report);
    if rows.is_empty() {
        return widgets::empty("Nothing spent this month.", theme);
    }
    let heading = format!(
        "{:<24} {:>8} {:>10} {:>12}   {}",
        "PROVIDER", "ATTEMPTS", "HELD", "CHARGED", "HEADROOM"
    );
    let body = rows
        .iter()
        .map(|row| Line::from(Span::styled(spend_row_text(row), theme.text())));
    let chart = super::super::spend_screen::chart(report)
        .into_iter()
        .map(Line::from);
    std::iter::once(Line::from(Span::styled(heading, theme.faint())))
        .chain(body)
        .chain([Line::default()])
        .chain(chart)
        .collect()
}

fn projected_details(collection: &Collection, item: &Item) -> Vec<Line<'static>> {
    let lines = if collection == &collection::PROCESSES {
        super::super::processes::details(&item.raw)
    } else {
        collection.money_lines(item)
    };
    lines
        .into_iter()
        .map(|line| Line::from(widgets::clean(&line)))
        .collect()
}
