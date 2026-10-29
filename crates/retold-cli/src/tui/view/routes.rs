//! Ordered provider fallback chains, with compatible-provider entry selection.
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::super::routes::Editor;
use super::super::theme::Theme;
use super::Layer;
use super::widgets::{self, Emphasis, panel};

const PANEL_COLUMNS: u16 = 92;
const HEIGHT: u16 = 22;
const HEADER_ROWS: usize = 3;
const FOOTER_ROWS: usize = 5;

pub fn layers(editor: &Editor, area: Rect, theme: Theme) -> Vec<Layer> {
    let rect = widgets::centered(area, PANEL_COLUMNS, HEIGHT);
    let visible = widgets::inner_rows(rect).saturating_sub(HEADER_ROWS + FOOTER_ROWS);
    let lines = header(theme)
        .into_iter()
        .chain(chain_lines(
            editor,
            (visible, widgets::inner_width(rect)),
            theme,
        ))
        .chain(controls(editor, widgets::inner_width(rect), theme))
        .collect::<Vec<_>>();
    let title = format!("Route · {}", editor.tier);
    vec![
        Layer::Blank(rect),
        Layer::text(
            Paragraph::new(lines).block(panel(&title, Emphasis::Focused, theme)),
            rect,
        ),
    ]
}

fn header(theme: Theme) -> Vec<Line<'static>> {
    vec![
        Line::from("First entry runs first; later entries are fallbacks."),
        Line::from(Span::styled(
            "Changes take effect after confirmation.",
            theme.muted(),
        )),
        Line::default(),
    ]
}

fn chain_lines(editor: &Editor, size: (usize, usize), theme: Theme) -> Vec<Line<'static>> {
    let (visible, width) = size;
    let empty = editor.chain.is_empty();
    if empty {
        return vec![Line::from(
            "Empty chain. Press + to add a provider before saving.",
        )];
    }
    editor
        .chain
        .iter()
        .enumerate()
        .skip(super::super::landing::offset(editor.selected, visible))
        .take(visible)
        .map(|(index, entry)| {
            let selected = index == editor.selected;
            let mark = if selected { "▌" } else { " " };
            let text = widgets::fit(&format!("{mark} {}. {entry}", index + 1), width);
            let style = if selected {
                theme.strong()
            } else {
                theme.muted()
            };
            Line::from(Span::styled(text, style))
        })
        .collect()
}

fn controls(editor: &Editor, width: usize, theme: Theme) -> Vec<Line<'static>> {
    let error = editor
        .problem
        .as_ref()
        .map(|problem| Line::from(Span::styled(widgets::fit(problem, width), theme.danger())));
    std::iter::once(Line::default())
        .chain(entry_controls(editor, width))
        .chain(error)
        .collect()
}

fn entry_controls(editor: &Editor, width: usize) -> Vec<Line<'static>> {
    if let Some(input) = &editor.adding {
        vec![
            Line::from(widgets::fit(&format!("› {}", input.text()), width)),
            Line::from("Tab picks a provider · type its selector · Enter adds · Esc cancels"),
        ]
    } else {
        vec![Line::from(
            "↑↓ select · J/K reorder · + add · - remove · Enter review · Esc discard",
        )]
    }
}
