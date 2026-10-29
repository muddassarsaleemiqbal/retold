//! Pages: forms, results and the video workspace.
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use retold_app::protocol::{Outcome as Finished, Payload};
use serde_json::Value;

use super::super::app::App;
use super::super::catalog::{Field, FieldKind, Risk};
use super::super::form::{Entry, Form, LIST_SEPARATOR};
use super::super::landing::offset;
use super::super::page::{Outcome, PLAN_TAB, Page, Reading, VideoPage};
use super::super::store::View;
use super::super::theme::Theme;
use super::landing::view_lines;
use super::widgets::{self, Emphasis, panel};
use super::{Layer, Picture};

const FORM_WIDTH: u16 = 96;
const PROMPT_FORM_SHARE: u16 = 65;
const MILLIS_PER_SECOND: u64 = 1_000;
const IDENTITY_ROWS: u16 = 5;
const STAGE_SHARE: u16 = 50;
const STATUS_ROWS: u16 = 3;
/// Lines each field takes: label, input, help, gap.
const FIELD_ROWS: usize = 4;
/// Columns between a panel's edge and its text: border and padding.
const TEXT_INSET: u16 = 2;
/// Columns before a field's typed text: indent and the `› ` prompt.
const VALUE_INSET: u16 = 2;

pub fn picture(app: &App, page: &Page, body: Rect) -> Picture {
    match page {
        Page::Form(form) => form_picture(app, form, body),
        Page::Reading(reading) => Picture::of(vec![reading_layer(app, reading, body)]),
        Page::Outcome(outcome) => Picture::of(outcome_layers(app, outcome, body)),
        Page::Video(video) => Picture::of(video_layers(app, video, body)),
    }
}

// ----- form -----

fn form_area(body: Rect) -> Rect {
    let width = FORM_WIDTH.min(body.width);
    let left = body.x.saturating_add(body.width.saturating_sub(width) / 2);
    Rect::new(left, body.y, width, body.height)
}

const fn input_width(area: Rect) -> u16 {
    area.width
        .saturating_sub(TEXT_INSET.saturating_mul(2))
        .saturating_sub(VALUE_INSET)
}

fn form_picture(app: &App, form: &Form, body: Rect) -> Picture {
    let prompt = form.action.command() == "creative prompt" && form.complete();
    if prompt {
        let [fields, preview] = Layout::vertical([
            Constraint::Percentage(PROMPT_FORM_SHARE),
            Constraint::Min(1),
        ])
        .areas(body);
        let mut picture = ordinary_form_picture(app, form, fields);
        let contents = app.store.contents(&form.arguments());
        if let Some(contents) = contents {
            picture
                .layers
                .push(super::creative::layer(app, contents, (preview, 0)));
        }
        return picture;
    }
    ordinary_form_picture(app, form, body)
}
fn ordinary_form_picture(app: &App, form: &Form, body: Rect) -> Picture {
    let area = form_area(body);
    let heading = form_heading(form, app.theme);
    let first_field = heading.len();
    let field_lines = form_fields(app, form, area);
    let focus_line = first_field.saturating_add(form.focus_row().saturating_mul(FIELD_ROWS));
    let skip = offset(
        focus_line.saturating_add(FIELD_ROWS),
        widgets::inner_rows(area),
    );
    let shown: Vec<Line<'static>> = heading
        .into_iter()
        .chain(field_lines)
        .chain(button_lines(form, app.theme))
        .skip(skip)
        .collect();
    let block = panel(&form.action.label(), Emphasis::Focused, app.theme);
    let place = Place {
        area,
        line: focus_line.saturating_sub(skip),
    };
    let cursor = form_cursor(app, form, place);
    Picture::with_cursor(
        vec![Layer::text(Paragraph::new(shown).block(block), area)],
        cursor,
    )
}

fn form_fields(app: &App, form: &Form, area: Rect) -> Vec<Line<'static>> {
    let fields = form
        .action
        .fields
        .iter()
        .zip(&form.entries)
        .enumerate()
        .filter(|(index, _)| form.visible_fields().contains(index));
    fields
        .flat_map(|(index, (field, entry))| {
            let paint = Paint {
                emphasis: field_emphasis(app, form, index),
                theme: app.theme,
                width: input_width(area),
            };
            field_lines(field, entry, paint)
        })
        .collect()
}

const fn field_emphasis(app: &App, form: &Form, index: usize) -> Emphasis {
    let modal = app.modal.is_some();
    match (index == form.focus, modal) {
        (true, false) => Emphasis::Focused,
        _ => Emphasis::Quiet,
    }
}

/// Where the form sits and the focused field's first visible line in it.
#[derive(Clone, Copy)]
struct Place {
    area: Rect,
    line: usize,
}

/// How a field is painted: whether it has focus, in the theme.
#[derive(Clone, Copy)]
struct Paint {
    emphasis: Emphasis,
    theme: Theme,
    width: u16,
}

/// The text cursor in the focused text field, just after the typed text.
fn form_cursor(app: &App, form: &Form, place: Place) -> Option<Position> {
    let Some(Entry::Text(input)) = form.entries.get(form.focus) else {
        return None;
    };
    let Place { area, line } = place;
    let modal = app.modal.is_some();
    let width = input_width(area);
    let (_, column) = widgets::input_view(input, width);
    let row = u16::try_from(line.saturating_add(1)).ok()?;
    let top = area.y.saturating_add(1);
    let y = top.saturating_add(row);
    let left = area.x.saturating_add(TEXT_INSET);
    let x = left.saturating_add(VALUE_INSET).saturating_add(column);
    let visible = width > 0 && y < area.bottom().saturating_sub(1);
    (visible && !modal).then_some(Position::new(x, y))
}

fn form_heading(form: &Form, theme: Theme) -> Vec<Line<'static>> {
    let about = form
        .action
        .about
        .lines()
        .map(|line| Line::from(Span::styled(widgets::clean(line), theme.muted())));
    std::iter::once(Line::from(vec![widgets::risk_chip(
        form.action.risk,
        theme,
    )]))
    .chain(about)
    .chain(
        form.step_title()
            .map(|title| Line::from(Span::styled(title, theme.accent()))),
    )
    .chain([Line::default()])
    .collect()
}

fn field_lines(field: &Field, entry: &Entry, paint: Paint) -> Vec<Line<'static>> {
    let Paint {
        emphasis, theme, ..
    } = paint;
    let focused = emphasis == Emphasis::Focused;
    let (marker, title_style) = if focused {
        ("▌ ", theme.title())
    } else {
        ("  ", theme.strong())
    };
    let required = if field.required { " *" } else { "" };
    let label = Line::from(vec![
        Span::styled(marker, theme.accent()),
        Span::styled(field.label.clone(), title_style),
        Span::styled(required, theme.danger()),
        Span::styled(flag_name(field), theme.faint()),
    ]);
    let help = Line::from(Span::styled(
        format!("  {}", help_text(field)),
        theme.faint(),
    ));
    vec![
        label,
        value_line(field, entry, paint),
        help,
        Line::default(),
    ]
}

fn flag_name(field: &Field) -> String {
    field
        .long
        .as_ref()
        .map_or_else(String::new, |long| format!("   --{long}"))
}

fn help_text(field: &Field) -> String {
    let help = widgets::clean(&field.help);
    let several = field
        .multiple
        .then(|| format!("Separate several with {LIST_SEPARATOR}"));
    let hint = match field.kind {
        FieldKind::Flag => "Space switches it on or off.",
        FieldKind::Choice(_) => "← → choose a value.",
        FieldKind::Text => "",
    };
    [help, several.unwrap_or_default()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .reduce(|joined, part| format!("{joined} · {part}"))
        .unwrap_or_else(|| hint.to_owned())
}

fn value_line(field: &Field, entry: &Entry, paint: Paint) -> Line<'static> {
    match entry {
        Entry::Flag(_) => flag_line(entry, paint),
        Entry::Choice(selected) => choice_line(field, *selected, paint.theme),
        Entry::Text(input) => text_line(field, input, paint),
    }
}

/// A switch drawn as a filled `● on` chip or a quiet `○ off`.
fn flag_line(entry: &Entry, paint: Paint) -> Line<'static> {
    let Paint {
        emphasis, theme, ..
    } = paint;
    let off_style = match emphasis {
        Emphasis::Focused => theme.strong(),
        Emphasis::Quiet => theme.muted(),
    };
    let (mark, style) = match entry {
        Entry::Flag(true) => ("● on ", theme.chip(theme.success())),
        Entry::Flag(false) | Entry::Text(_) | Entry::Choice(_) => ("○ off", off_style),
    };
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!(" {mark} "), style),
    ])
}

fn text_line(field: &Field, input: &super::super::input::Input, paint: Paint) -> Line<'static> {
    let Paint {
        emphasis, theme, ..
    } = paint;
    let (visible, _) = widgets::input_view(input, paint.width);
    let placeholder = field
        .default
        .as_ref()
        .map_or_else(String::new, |default| format!("{default} (default)"));
    let shown = if visible.is_empty() {
        Span::styled(placeholder, theme.faint())
    } else {
        Span::styled(visible, theme.text())
    };
    let prompt = match emphasis {
        Emphasis::Focused => theme.accent(),
        Emphasis::Quiet => theme.faint(),
    };
    Line::from(vec![Span::styled("› ", prompt), shown])
}

/// Up to this many choices show side by side; more become a compact `‹ value ›` picker.
const INLINE_CHOICES: usize = 4;

fn choice_line(field: &Field, selected: Option<usize>, theme: Theme) -> Line<'static> {
    let choices = match &field.kind {
        FieldKind::Choice(choices) => choices.as_slice(),
        FieldKind::Flag | FieldKind::Text => &[],
    };
    let default_label = field.default.as_ref().map_or_else(
        || "unset".to_owned(),
        |default| format!("{default} (default)"),
    );
    let compact = choices.len() > INLINE_CHOICES;
    if compact {
        return picker_line((choices, selected), &default_label, theme);
    }
    inline_line((choices, selected), &default_label, theme)
}

/// Every choice as a chip, the chosen one filled.
fn inline_line(
    choosing: (&[String], Option<usize>),
    default_label: &str,
    theme: Theme,
) -> Line<'static> {
    let (choices, selected) = choosing;
    let chip = |chosen: bool| -> Style {
        if chosen {
            theme.chip(theme.accent())
        } else {
            theme.muted()
        }
    };
    let unset = Span::styled(format!(" {default_label} "), chip(selected.is_none()));
    let options = choices.iter().enumerate().flat_map(|(index, choice)| {
        [
            Span::raw(" "),
            Span::styled(format!(" {choice} "), chip(selected == Some(index))),
        ]
    });
    let spans: Vec<Span<'static>> = [Span::raw("  "), unset]
        .into_iter()
        .chain(options)
        .collect();
    Line::from(spans)
}

/// `‹ codex_cli ›  3 of 7`: one value at a time, so long lists never overflow the form.
fn picker_line(
    choosing: (&[String], Option<usize>),
    default_label: &str,
    theme: Theme,
) -> Line<'static> {
    let (choices, selected) = choosing;
    let shown = selected
        .and_then(|index| choices.get(index))
        .map_or(default_label, String::as_str);
    let position = selected.map_or_else(
        || format!("{} choices · ← → to pick", choices.len()),
        |index| format!("{} of {}", index.saturating_add(1), choices.len()),
    );
    Line::from(vec![
        Span::raw("  "),
        Span::styled("‹ ", theme.accent()),
        Span::styled(format!(" {shown} "), theme.chip(theme.accent())),
        Span::styled(" ›", theme.accent()),
        Span::styled(format!("   {position}"), theme.muted()),
    ])
}

const fn button_label(risk: Risk) -> &'static str {
    match risk {
        Risk::Read => "  Show  ⏎ ",
        Risk::Write => "  Review and run  ⏎ ",
        Risk::Spend => "  Review cost and run  ⏎ ",
        Risk::Destructive => "  Review and remove  ⏎ ",
    }
}

fn button_lines(form: &Form, theme: Theme) -> Vec<Line<'static>> {
    let style = if form.on_button() {
        theme.chip(theme.accent())
    } else {
        theme.muted()
    };
    let button = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            if form.last_step() {
                button_label(form.action.risk)
            } else {
                "  Continue  ⏎ "
            },
            style,
        ),
    ]);
    let problem = form.problem.as_ref().map(|problem| {
        [
            Line::default(),
            Line::from(Span::styled(format!("  {problem}"), theme.danger())),
        ]
    });
    std::iter::once(button)
        .chain(problem.into_iter().flatten())
        .collect()
}

// ----- results -----

fn reading_layer(app: &App, reading: &Reading, body: Rect) -> Layer {
    let document = reading.document_subject().is_some();
    if document {
        return super::document::layer(app, reading, body);
    }
    let series = reading.series_id().is_some();
    if series {
        return super::series::layer(app, reading, body);
    }
    if let Some(id) = reading.provider_id() {
        return super::provider_status::layer(app, (id, reading.scroll), body);
    }
    let streaming = reading.stream.is_some();
    if streaming {
        return super::stream::layer(app, reading, body);
    }
    ordinary_reading_layer(app, reading, body)
}

fn ordinary_reading_layer(app: &App, reading: &Reading, body: Rect) -> Layer {
    let creative = reading.words.first().is_some_and(|word| word == "creative");
    if creative {
        let contents = app.store.contents(&reading.words);
        if let Some(contents) = contents {
            return super::creative::layer(app, contents, (body, reading.scroll));
        }
    }
    let view = app.store.view(&reading.words);
    let refreshing = matches!(
        view,
        View::Ready {
            refreshing: true,
            ..
        }
    );
    let title = if refreshing {
        format!("{} · refreshing", reading.title)
    } else {
        reading.title.clone()
    };
    let paragraph = Paragraph::new(view_lines(app, view, "Working…"))
        .wrap(Wrap { trim: false })
        .scroll((reading.scroll, 0))
        .block(panel(&title, Emphasis::Focused, app.theme));
    Layer::text(paragraph, body)
}

fn outcome_layers(app: &App, outcome: &Outcome, body: Rect) -> Vec<Layer> {
    let theme = app.theme;
    let [status, content] =
        Layout::vertical([Constraint::Length(STATUS_ROWS), Constraint::Min(1)]).areas(body);
    let (chip, note) = outcome_status(app, outcome);
    let status_line = Line::from(vec![
        chip,
        Span::raw("  "),
        Span::styled(note, theme.muted()),
    ]);
    let title = outcome.form.action.label();
    let lines = outcome_lines(app, outcome);
    let result = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((outcome.scroll, 0))
        .block(panel("Result", Emphasis::Quiet, theme));
    vec![
        Layer::text(
            Paragraph::new(status_line).block(panel(&title, Emphasis::Focused, theme)),
            status,
        ),
        Layer::text(result, content),
    ]
}

fn outcome_lines(app: &App, outcome: &Outcome) -> Vec<Line<'static>> {
    let theme = app.theme;
    outcome.payload.as_ref().map_or_else(
        || {
            widgets::loading(
                app.now,
                "Running. Keep using the app; the result appears here.",
                theme,
            )
        },
        |payload| widgets::payload_lines(payload, theme),
    )
}

fn outcome_status(app: &App, outcome: &Outcome) -> (Span<'static>, String) {
    let theme = app.theme;
    let elapsed = app.now.saturating_sub(outcome.started) / MILLIS_PER_SECOND;
    match &outcome.payload {
        None => {
            let chip = format!(" {} running ", widgets::spinner(app.now));
            (
                Span::styled(chip, theme.chip(theme.accent())),
                format!("{elapsed}s"),
            )
        }
        Some(Payload::Result {
            outcome: finished, ..
        }) => finished_chip(*finished, theme),
        Some(Payload::Error {
            outcome: finished, ..
        }) => refused_chip(*finished, theme),
        Some(Payload::Event(_)) => (
            Span::styled(" • event ", theme.chip(theme.notice())),
            String::new(),
        ),
    }
}

fn finished_chip(finished: Finished, theme: Theme) -> (Span<'static>, String) {
    match finished {
        Finished::Queued | Finished::Detached => (
            Span::styled(" ◎ in the studio ", theme.chip(theme.notice())),
            "Follow it in Studio (5).".into(),
        ),
        Finished::Completed | Finished::Blocked | Finished::Failed => (
            Span::styled(" ✓ done ", theme.chip(theme.success())),
            String::new(),
        ),
    }
}

fn refused_chip(finished: Finished, theme: Theme) -> (Span<'static>, String) {
    match finished {
        Finished::Blocked => (
            Span::styled(" ⚠ blocked ", theme.chip(theme.warning())),
            "Nothing was changed.".into(),
        ),
        Finished::Completed | Finished::Queued | Finished::Detached | Finished::Failed => (
            Span::styled(" ✗ failed ", theme.chip(theme.danger())),
            "See the fix below.".into(),
        ),
    }
}

// ----- video -----

fn video_layers(app: &App, video: &VideoPage, body: Rect) -> Vec<Layer> {
    if video.tab == super::super::app::review::REVIEW_TAB {
        return super::review::layers(app, video, body);
    }
    let view = app.store.view(&video.words());
    if video.current().title == "Deliver" {
        return super::deliver::layers(app, video, body);
    }
    let plan = video.tab == PLAN_TAB;
    if let (true, View::Ready { contents, .. }) = (plan, view) {
        return plan_layers(
            app,
            contents,
            Scrolled {
                area: body,
                scroll: video.scroll,
            },
        );
    }
    let paragraph = Paragraph::new(view_lines(app, view, "Loading…"))
        .wrap(Wrap { trim: false })
        .scroll((video.scroll, 0))
        .block(panel(video.current().title, Emphasis::Focused, app.theme));
    vec![Layer::text(paragraph, body)]
}

/// Areas of the Plan tab: identity on top, stages left, blockers and next steps right.
struct PlanAreas {
    identity: Rect,
    stages: Rect,
    blockers: Rect,
    next: Rect,
}

fn plan_areas(body: Rect) -> PlanAreas {
    let [identity, rest] =
        Layout::vertical([Constraint::Length(IDENTITY_ROWS), Constraint::Min(1)]).areas(body);
    let [stages, side] =
        Layout::horizontal([Constraint::Percentage(STAGE_SHARE), Constraint::Fill(1)]).areas(rest);
    let [blockers, next] = Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(side);
    PlanAreas {
        identity,
        stages,
        blockers,
        next,
    }
}

/// A body area and how far its main pane is scrolled.
#[derive(Clone, Copy)]
struct Scrolled {
    area: Rect,
    scroll: u16,
}

fn plan_layers(app: &App, plan: &Value, place: Scrolled) -> Vec<Layer> {
    let theme = app.theme;
    let Scrolled { area: body, scroll } = place;
    let areas = plan_areas(body);
    let wrapped = |lines: Vec<Line<'static>>, title: &str| {
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(panel(title, Emphasis::Quiet, theme))
    };
    let stages = Paragraph::new(stage_lines(plan, theme))
        .scroll((scroll, 0))
        .block(panel("Stages", Emphasis::Focused, theme));
    vec![
        Layer::text(
            wrapped(identity_lines(plan, theme), "Video"),
            areas.identity,
        ),
        Layer::text(stages, areas.stages),
        Layer::text(
            wrapped(blocker_lines(plan, theme), "Blockers"),
            areas.blockers,
        ),
        Layer::text(
            wrapped(next_lines(plan, theme), "Continue would"),
            areas.next,
        ),
    ]
}

fn identity_lines(plan: &Value, theme: Theme) -> Vec<Line<'static>> {
    let identity = crate::video_screen::identity(plan)
        .into_iter()
        .map(|line| Line::from(Span::styled(widgets::clean(&line), theme.text())));
    let status = Line::from(vec![
        Span::styled("Status  ", theme.muted()),
        Span::styled(crate::video_screen::status(plan), theme.strong()),
    ]);
    identity.chain([status]).collect()
}

fn next_lines(plan: &Value, theme: Theme) -> Vec<Line<'static>> {
    crate::video_screen::plan_lines(plan)
        .into_iter()
        .map(|line| Line::from(Span::styled(widgets::clean(&line), theme.text())))
        .collect()
}

fn stage_lines(plan: &Value, theme: Theme) -> Vec<Line<'static>> {
    let heading = format!(
        "  {:<12} {:>5} {:>3} {:>3}  STATE",
        "STAGE", "STEPS", "OK", "BAD"
    );
    let rows = crate::video_screen::stages(plan).into_iter().map(|row| {
        let (mark, style) = if row.planned {
            ("▸ ", theme.accent())
        } else {
            ("  ", theme.text())
        };
        let text = format!(
            "{:<12} {:>5} {:>3} {:>3}  {}",
            row.stage, row.recorded, row.verified, row.unresolved, row.decision
        );
        Line::from(vec![
            Span::styled(mark, theme.accent()),
            Span::styled(text, style),
        ])
    });
    let legend = Line::from(Span::styled("▸ runs when you continue", theme.faint()));
    std::iter::once(Line::from(Span::styled(heading, theme.faint())))
        .chain(rows)
        .chain([Line::default(), legend])
        .collect()
}

fn blocker_lines(plan: &Value, theme: Theme) -> Vec<Line<'static>> {
    let blockers = crate::video_screen::blockers(plan);
    if blockers.is_empty() {
        return vec![Line::from(Span::styled(
            "✓ Nothing blocks this plan.",
            theme.success(),
        ))];
    }
    blockers
        .iter()
        .flat_map(|blocker| one_blocker(blocker, theme))
        .collect()
}

fn one_blocker(blocker: &crate::video_screen::Blocker, theme: Theme) -> Vec<Line<'static>> {
    let what = Line::from(Span::styled(
        format!("⚠ {}", widgets::clean(&blocker.what)),
        theme.warning(),
    ));
    let affects = Line::from(Span::styled(
        format!("  Affects {}", widgets::clean(&blocker.affects)),
        theme.muted(),
    ));
    let actions = blocker.actions.iter().map(|action| {
        Line::from(Span::styled(
            format!("  → {}", widgets::clean(action)),
            theme.accent(),
        ))
    });
    [what, affects]
        .into_iter()
        .chain(actions)
        .collect::<Vec<_>>()
}

pub fn form_focus(app: &App, form: &Form, location: (Rect, Position)) -> Option<usize> {
    let (body, position) = location;
    let area = form_area(body);
    if !area.contains(position) {
        return None;
    }
    let heading = form_heading(form, app.theme).len();
    let row = clicked_form_row(form, (area, heading), position)?;
    Some(form.field_at_row(row))
}

fn click_line(area: Rect, position: Position) -> usize {
    usize::from(position.y.saturating_sub(area.y + 1))
}

fn form_skip(form: &Form, area: Rect, heading: usize) -> usize {
    let focused = heading + form.focus_row() * FIELD_ROWS;
    offset(focused + FIELD_ROWS, widgets::inner_rows(area))
}

fn clicked_form_row(form: &Form, layout: (Rect, usize), position: Position) -> Option<usize> {
    let (area, heading) = layout;
    let line = click_line(area, position) + form_skip(form, area, heading);
    Some(line.checked_sub(heading)? / FIELD_ROWS)
}
