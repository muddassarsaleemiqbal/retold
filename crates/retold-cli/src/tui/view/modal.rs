//! Floating layers: the command palette, confirmations, item menus and help.
use ratatui::layout::{Margin, Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph, Wrap};

use super::super::app::App;
use super::super::catalog::Risk;
use super::super::form::Form;
use super::super::landing::offset;
use super::super::page::{Entry, Menu, Modal, Palette};
use super::super::theme::Theme;
use super::widgets::{self, Emphasis, Row, fit, panel};
use super::{Layer, Picture};

const PALETTE_WIDTH: u16 = 100;
const PALETTE_HEIGHT: u16 = 22;
const CONFIRM_WIDTH: u16 = 84;
const CONFIRM_HEIGHT: u16 = 13;
const MENU_WIDTH: u16 = 72;
const MENU_HEIGHT: u16 = 18;
const HELP_WIDTH: u16 = 72;
const HELP_HEIGHT: u16 = 24;
/// Rows above the palette list: the query and a rule.
const PALETTE_HEADER_ROWS: usize = 2;
/// Rows between the top of the screen and a palette.
const TOP_GAP: u16 = 3;
/// Columns between a panel edge and its text.
const TEXT_INSET: u16 = 2;
/// Columns of the `› ` prompt before the query.
const PROMPT_WIDTH: u16 = 2;
/// Entry rows give half their width to the label.
const LABEL_HALVES: usize = 2;
const HELP_KEY_WIDTH: usize = 18;

pub fn picture(app: &App, area: Rect) -> Picture {
    let Some(modal) = &app.modal else {
        return Picture::of(Vec::new());
    };
    let theme = app.theme;
    let tint = Layer::Tint(theme.faint(), area);
    let (mut layers, cursor) = match modal {
        Modal::Palette(palette) | Modal::Values(palette) => palette_layers(palette, area, theme),
        Modal::BulkTag(input) => (tag_layers(input, area, theme), None),
        Modal::Batch(forms) => (super::bulk::layers(app, forms, area), None),
        Modal::Document(editor) => (super::document::edit_layers(app, editor, area), None),
        Modal::Route(editor) => (super::routes::layers(editor, area, theme), None),
        Modal::Browser(browser) => (super::files::layers(app, browser, area), None),
        Modal::Setup(selected) => (super::onboarding::layers(app, *selected, area), None),
        Modal::Destroy { form, typed } => (destroy_layers((form, typed), area, theme), None),
        Modal::Preview(preview) => (super::preview::layers(app, preview, area), None),
        Modal::Confirm(form) => (confirm_layers(form, area, theme), None),
        Modal::Plan(review) => (super::production::layers(app, review, area), None),
        Modal::Menu(menu) => (menu_layers(menu, area, theme), None),
        Modal::Help => (help_layers(area, theme), None),
        Modal::Quit => (quit_layers(area, theme), None),
    };
    layers.extend(mouse_buttons(&layers, theme, modal));
    Picture::with_cursor(std::iter::once(tint).chain(layers).collect(), cursor)
}

fn quit_layers(area: Rect, theme: Theme) -> Vec<Layer> {
    let rect = widgets::centered(area, CONFIRM_WIDTH, CONFIRM_HEIGHT);
    let message = "An action is still running.\n\nEnter leaves once current changes finish.\nEsc returns to the app; Studio jobs continue independently.";
    floating(
        rect,
        Paragraph::new(message)
            .wrap(Wrap { trim: false })
            .block(panel("Leave Retold?", Emphasis::Focused, theme)),
    )
}

/// The form being run and what has been typed to confirm it.
fn destroy_layers(
    confirming: (&Form, &super::super::input::Input),
    area: Rect,
    theme: Theme,
) -> Vec<Layer> {
    let (form, typed) = confirming;
    let rect = widgets::centered(area, CONFIRM_WIDTH, CONFIRM_HEIGHT);
    let name = form
        .value("reference")
        .or_else(|| form.value("name"))
        .unwrap_or_default();
    let lines = vec![
        Line::from(Span::styled(
            "Permanent deletion cannot be undone.",
            theme.danger(),
        )),
        Line::default(),
        Line::from(format!("Type the exact reference to delete: {name}")),
        Line::from(Span::styled(format!("› {}", typed.text()), theme.strong())),
        Line::default(),
        Line::from("Enter deletes only when the reference matches. Esc returns to the form."),
    ];
    floating(
        rect,
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(panel("Delete permanently", Emphasis::Focused, theme)),
    )
}

fn floating(area: Rect, paragraph: Paragraph<'static>) -> Vec<Layer> {
    vec![Layer::Blank(area), Layer::text(paragraph, area)]
}

fn top_centered(area: Rect, width: u16, height: u16) -> Rect {
    let margin = TEXT_INSET.saturating_mul(2);
    let width = width.min(area.width.saturating_sub(margin));
    let height = height.min(area.height.saturating_sub(TOP_GAP));
    let left = area.x.saturating_add(area.width.saturating_sub(width) / 2);
    let top = area.y.saturating_add(TOP_GAP.min(area.height));
    Rect::new(left, top, width, height)
}

fn palette_layers(palette: &Palette, area: Rect, theme: Theme) -> (Vec<Layer>, Option<Position>) {
    let rect = top_centered(area, PALETTE_WIDTH, PALETTE_HEIGHT);
    let matches = palette.matches();
    let width = widgets::inner_width(rect);
    let listed = palette_entries(palette, rect, theme);
    let nothing = matches
        .is_empty()
        .then(|| Line::from(Span::styled("No matches.", theme.muted())));
    let (query, column) = query_line(palette, rect, theme);
    let rule = Line::from(Span::styled("─".repeat(width), theme.border()));
    let lines: Vec<Line<'static>> = [query, rule]
        .into_iter()
        .chain(listed)
        .chain(nothing)
        .collect();
    let values = palette
        .entries
        .iter()
        .all(|entry| matches!(entry.choice, super::super::page::Choice::Value { .. }));
    let label = if values { "Choose value" } else { "Commands" };
    let title = format!("{label} · {}", matches.len());
    let paragraph = Paragraph::new(lines).block(panel(&title, Emphasis::Focused, theme));
    let cursor = palette_cursor(rect, column);
    (floating(rect, paragraph), cursor)
}

fn palette_entries(palette: &Palette, rect: Rect, theme: Theme) -> Vec<Line<'static>> {
    let entries = palette.matches();
    let rows = widgets::inner_rows(rect).saturating_sub(PALETTE_HEADER_ROWS);
    let width = widgets::inner_width(rect);
    entries
        .iter()
        .enumerate()
        .skip(offset(palette.selected, rows))
        .take(rows)
        .map(|(index, entry)| {
            entry_line(
                entry,
                Look::at(index, palette.selected, theme),
                labels(&entries, width),
            )
        })
        .collect()
}

fn query_line(palette: &Palette, rect: Rect, theme: Theme) -> (Line<'static>, u16) {
    let room = query_width(rect);
    let (visible, column) = widgets::input_view(&palette.query, room);
    let query = if visible.is_empty() {
        Span::styled("Search…", theme.faint())
    } else {
        Span::styled(visible, theme.strong())
    };
    (
        Line::from(vec![Span::styled("› ", theme.accent()), query]),
        column,
    )
}

const fn query_width(rect: Rect) -> u16 {
    rect.width
        .saturating_sub(TEXT_INSET.saturating_mul(2).saturating_add(PROMPT_WIDTH))
}

/// A list row in a focused modal, in the theme.
#[derive(Clone, Copy)]
struct Look {
    state: Row,
    theme: Theme,
}

impl Look {
    const fn at(index: usize, selected: usize, theme: Theme) -> Self {
        Self {
            state: Row::at(index, selected, Emphasis::Focused),
            theme,
        }
    }
}

fn palette_cursor(rect: Rect, column: u16) -> Option<Position> {
    let left = rect
        .x
        .saturating_add(TEXT_INSET)
        .saturating_add(PROMPT_WIDTH);
    let position = Position::new(left.saturating_add(column), rect.y.saturating_add(1));
    let has_room = query_width(rect) > 0;
    let inside = rect.inner(Margin::new(TEXT_INSET, 1)).contains(position);
    (has_room && inside).then_some(position)
}

/// Column widths for entry rows: labels as wide as the longest, up to half the row.
#[derive(Clone, Copy)]
struct Labels {
    label: usize,
    width: usize,
}

fn labels(entries: &[&Entry], width: usize) -> Labels {
    let longest = entries
        .iter()
        .map(|entry| entry.label.chars().count())
        .max()
        .unwrap_or_default();
    let half = (width / LABEL_HALVES).max(1);
    Labels {
        label: longest.saturating_add(2).min(half),
        width,
    }
}

fn entry_line(entry: &Entry, look: Look, columns: Labels) -> Line<'static> {
    let Look { state, theme } = look;
    let Labels {
        label: label_width,
        width,
    } = columns;
    let chip = entry.risk.map(|risk| widgets::risk_chip(risk, theme));
    let chip_width = chip.as_ref().map_or(0, Span::width);
    let used = label_width.saturating_add(chip_width);
    let detail_width = width.saturating_sub(used.saturating_add(2));
    let detail = fit(&widgets::clean(&entry.detail), detail_width);
    let spans: Vec<Span<'static>> = [
        Span::styled(fit(&entry.label, label_width), state.style(theme)),
        Span::styled(format!(" {detail} "), state.secondary(theme)),
    ]
    .into_iter()
    .chain(chip)
    .collect();
    Line::from(spans)
}

fn confirm_layers(form: &Form, area: Rect, theme: Theme) -> Vec<Layer> {
    if super::super::defaults::applies(form) {
        return super::create::layers(form, area, theme);
    }
    let rect = widgets::centered(area, CONFIRM_WIDTH, CONFIRM_HEIGHT);
    let role = widgets::risk_role(form.action.risk, theme);
    let block = confirmation_frame(form, theme);
    let buttons = Line::from(vec![
        Span::styled("  Run  ⏎  ", theme.chip(role)),
        Span::raw("   "),
        Span::styled(" Cancel  esc ", theme.muted()),
    ]);
    let command = format!("$ {}", widgets::clean(&form.command_line()));
    let lines = vec![
        Line::from(vec![widgets::risk_chip(form.action.risk, theme)]),
        Line::default(),
        Line::from(Span::styled(consequence(form), theme.text())),
        Line::default(),
        Line::from(Span::styled(command, theme.accent())),
        Line::default(),
        buttons,
    ];
    floating(
        rect,
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
    )
}

fn confirmation_frame(form: &Form, theme: Theme) -> Block<'static> {
    let role = widgets::risk_role(form.action.risk, theme);
    let title = Span::styled(
        format!(" {} ", form.action.label()),
        role.add_modifier(Modifier::BOLD),
    );
    let frame = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(role);
    frame
        .padding(Padding::new(TEXT_INSET, TEXT_INSET, 1, 0))
        .title(title)
}

/// The allowance given with `--spend`, if any.
fn allowance(form: &Form) -> Option<String> {
    let words = form.arguments();
    let position = words.iter().position(|word| word == "--spend")?;
    words.get(position.saturating_add(1)).cloned()
}

/// One sentence on what running this will do, specific to how risky it is.
fn consequence(form: &Form) -> String {
    let switching = form.action.command() == "workspace switch";
    if switching {
        return "This changes the default workspace and clears unsaved TUI drafts and selections. No paid requests are sent.".into();
    }
    match (form.action.risk, allowance(form)) {
        (Risk::Spend, Some(amount)) => format!(
            "This may send paid requests, spending at most ${amount} and never more than your account limits allow."
        ),
        (Risk::Spend, None) => {
            "No allowance is given, so only free and already-saved work runs. Nothing is charged."
                .into()
        }
        (Risk::Destructive, _) => {
            "This removes or replaces data. Check the command before running it.".into()
        }
        (Risk::Write, _) => {
            "This changes your files or settings. No paid requests are sent.".into()
        }
        (Risk::Read, _) => "This only reads saved state.".into(),
    }
}

fn menu_layers(menu: &Menu, area: Rect, theme: Theme) -> Vec<Layer> {
    let rows = u16::try_from(menu.entries.len()).unwrap_or(u16::MAX);
    let height = MENU_HEIGHT.min(rows.saturating_add(2));
    let rect = widgets::centered(area, MENU_WIDTH, height);
    let width = widgets::inner_width(rect);
    let visible = widgets::inner_rows(rect);
    let lines: Vec<Line<'static>> = menu
        .entries
        .iter()
        .enumerate()
        .skip(offset(menu.selected, visible))
        .take(visible)
        .map(|(index, entry)| {
            let entries: Vec<&Entry> = menu.entries.iter().collect();
            entry_line(
                entry,
                Look::at(index, menu.selected, theme),
                labels(&entries, width),
            )
        })
        .collect();
    let block = panel(&widgets::clean(&menu.title), Emphasis::Focused, theme);
    floating(rect, Paragraph::new(lines).block(block))
}

const HELP: [(&str, &str); 19] = [
    ("1 – 9, 0", "jump to a section"),
    (": or ctrl-k", "search every action"),
    ("↑ ↓  j k", "move"),
    ("← →  h l", "sidebar and main"),
    ("⇥  [ ]", "next or previous tab"),
    ("⏎", "open the selection"),
    (".", "actions for the selection"),
    ("n", "create a new one"),
    ("/", "filter the list"),
    ("J K  PgUp PgDn", "scroll the detail"),
    ("e", "edit and run again"),
    ("ctrl-r", "refresh everything"),
    ("T", "change theme"),
    ("esc", "back, close or clear"),
    ("q", "quit"),
    ("", ""),
    ("Forms", "↑↓ move between fields · space switches"),
    (
        "",
        "← → picks a value · ⏎ next field · ctrl-s continue or run",
    ),
    ("Mouse", "click to select, again to open · wheel scrolls"),
];

fn help_layers(area: Rect, theme: Theme) -> Vec<Layer> {
    let rect = widgets::centered(area, HELP_WIDTH, HELP_HEIGHT);
    let lines: Vec<Line<'static>> = HELP
        .iter()
        .map(|(key, what)| {
            Line::from(vec![
                Span::styled(format!("{key:<HELP_KEY_WIDTH$}"), theme.key()),
                Span::styled(*what, theme.text()),
            ])
        })
        .collect();
    floating(
        rect,
        Paragraph::new(lines).block(panel("Keys", Emphasis::Focused, theme)),
    )
}

fn tag_layers(input: &super::super::input::Input, area: Rect, theme: Theme) -> Vec<Layer> {
    let rect = widgets::centered(area, CONFIRM_WIDTH, CONFIRM_HEIGHT);
    let text = format!(
        "Tag selected videos\n\n› {}\n\nEnter reviews every target · Esc cancels",
        input.text()
    );
    floating(
        rect,
        Paragraph::new(text).wrap(Wrap { trim: false }).block(panel(
            "Add tag",
            Emphasis::Focused,
            theme,
        )),
    )
}

#[derive(Debug, Clone, Copy)]
pub enum Hit {
    Confirm,
    Cancel,
    Row(usize),
}
pub fn hit(app: &App, position: Position) -> Option<Hit> {
    let modal = app.modal.as_ref()?;
    let picture = picture(app, app.area);
    let rect = picture.layers.iter().find_map(|layer| match layer {
        Layer::Blank(rect) => Some(*rect),
        _ => None,
    })?;
    if !rect.contains(position) {
        return None;
    }
    let buttons = rect.bottom().saturating_sub(2);
    if position.y == buttons {
        return Some(if position.x < rect.x + rect.width / 2 {
            Hit::Confirm
        } else {
            Hit::Cancel
        });
    }
    modal.row_hit(rect, position)
}
impl Modal {
    fn row_hit(&self, rect: Rect, position: Position) -> Option<Hit> {
        let header = match self {
            Self::Palette(_) | Self::Values(_) => PALETTE_HEADER_ROWS,
            Self::Menu(_) => 0,
            _ => return None,
        };
        let first = rect
            .y
            .saturating_add(1)
            .saturating_add(u16::try_from(header).unwrap_or_default());
        let row = usize::from(position.y.checked_sub(first)?);
        match self {
            Self::Palette(palette) | Self::Values(palette) => {
                let visible = widgets::inner_rows(rect).saturating_sub(PALETTE_HEADER_ROWS);
                Some(Hit::Row(row + offset(palette.selected, visible)))
            }
            _ => Some(Hit::Row(row)),
        }
    }
}
fn mouse_buttons(layers: &[Layer], theme: Theme, modal: &Modal) -> Vec<Layer> {
    let rect = layers.iter().find_map(|layer| match layer {
        Layer::Blank(rect) => Some(*rect),
        _ => None,
    });
    let Some(rect) = rect else {
        return Vec::new();
    };
    button_layer(rect, theme, modal)
}
fn button_layer(rect: Rect, theme: Theme, modal: &Modal) -> Vec<Layer> {
    let width = usize::from(rect.width.saturating_sub(2)) / 2;
    let confirm = matches!(
        modal,
        Modal::Confirm(_) | Modal::Plan(_) | Modal::Preview(_) | Modal::Batch(_)
    );
    let label = if confirm {
        " Enter · confirm displayed action"
    } else {
        " Enter · continue / review"
    };
    let text = format!("{label:<width$}{}", " Esc · cancel");
    let button_area = Rect::new(
        rect.x + 1,
        rect.bottom().saturating_sub(2),
        rect.width.saturating_sub(2),
        1,
    );
    vec![Layer::text(
        Paragraph::new(text).style(theme.surface()),
        button_area,
    )]
}
