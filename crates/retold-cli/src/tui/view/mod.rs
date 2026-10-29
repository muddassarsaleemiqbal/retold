//! Drawing is a pure function of `App`: `picture` lists layers and where the cursor goes, and
//! `frame` paints them. Layout comes from `regions`, which mouse handling uses too, so a click
//! always lands on what was drawn there.
mod bulk;
mod coverage;
mod create;
mod creative;
mod deliver;
mod document;
mod files;
mod landing;
mod modal;
mod onboarding;
mod pages;
mod preview;
mod production;
mod provider_status;
mod report;
mod review;
mod routes;
mod series;
mod shell;
mod stream;
pub mod widgets;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Clear, Paragraph};

use super::app::{App, Focus};
use widgets::Emphasis;

const HEADER_HEIGHT: u16 = 1;
const FOOTER_HEIGHT: u16 = 2;
const TABS_HEIGHT: u16 = 1;
const SIDEBAR_WIDE: u16 = 22;
const SIDEBAR_NARROW: u16 = 5;
/// Below this width the sidebar shows only its icons.
const NARROW_BELOW: u16 = 100;
/// Below this width a list fills the body and its detail is hidden.
pub const SINGLE_PANE_BELOW: u16 = 72;
/// A list's share of the body when its detail sits beside it.
const LIST_SHARE: u16 = 42;

/// One thing to paint, bottom to top.
pub enum Layer {
    /// Boxed: a paragraph is far larger than a rectangle.
    Text(Box<Paragraph<'static>>, Rect),
    /// Erases what is below, for floating layers.
    Blank(Rect),
    /// Restyles what is below, to quiet the screen behind a modal.
    Tint(Style, Rect),
}

impl Layer {
    #[must_use]
    pub fn text(paragraph: Paragraph<'static>, area: Rect) -> Self {
        Self::Text(Box::new(paragraph), area)
    }
}

/// A whole frame: its layers and where the blinking cursor goes, if anywhere.
pub struct Picture {
    pub layers: Vec<Layer>,
    pub cursor: Option<Position>,
}

impl Picture {
    #[must_use]
    pub const fn of(layers: Vec<Layer>) -> Self {
        Self {
            layers,
            cursor: None,
        }
    }

    #[must_use]
    pub const fn with_cursor(layers: Vec<Layer>, cursor: Option<Position>) -> Self {
        Self { layers, cursor }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Regions {
    pub header: Rect,
    pub sidebar: Rect,
    pub tabs: Rect,
    pub body: Rect,
    pub footer: Rect,
}

#[must_use]
pub fn regions(area: Rect) -> Regions {
    let [header, middle, footer] = Layout::vertical([
        Constraint::Length(HEADER_HEIGHT),
        Constraint::Min(1),
        Constraint::Length(FOOTER_HEIGHT),
    ])
    .areas(area);
    let narrow = area.width < NARROW_BELOW;
    let sidebar_width = if narrow { SIDEBAR_NARROW } else { SIDEBAR_WIDE };
    let [sidebar, main] =
        Layout::horizontal([Constraint::Length(sidebar_width), Constraint::Min(1)]).areas(middle);
    let [tabs, body] =
        Layout::vertical([Constraint::Length(TABS_HEIGHT), Constraint::Min(1)]).areas(main);
    Regions {
        header,
        sidebar,
        tabs,
        body,
        footer,
    }
}

/// The list and detail panes of a landing body.
#[must_use]
pub fn split(body: Rect) -> (Rect, Option<Rect>) {
    split_at(body, LIST_SHARE)
}

/// Like `split`, with the list taking `share` percent; narrow bodies show the list alone.
#[must_use]
pub fn split_at(body: Rect, share: u16) -> (Rect, Option<Rect>) {
    let narrow = body.width < SINGLE_PANE_BELOW;
    if narrow {
        return (body, None);
    }
    let [list, detail] =
        Layout::horizontal([Constraint::Percentage(share), Constraint::Fill(1)]).areas(body);
    (list, Some(detail))
}

/// Whether the main area has the keyboard: not the sidebar, and no modal on top.
#[must_use]
pub const fn main_emphasis(app: &App) -> Emphasis {
    let modal = app.modal.is_some();
    match (app.focus, modal) {
        (Focus::Main, false) => Emphasis::Focused,
        _ => Emphasis::Quiet,
    }
}

/// Everything on screen for `app` in `area`, bottom layer first.
#[must_use]
pub fn picture(app: &App, area: Rect) -> Picture {
    let regions = regions(area);
    let mut layers = vec![Layer::Tint(app.theme.canvas(), area)];
    layers.extend(shell::layers(app, regions));
    let body = match app.page() {
        Some(page) => pages::picture(app, page, regions.body),
        None => landing::picture(app, regions.body),
    };
    layers.extend(body.layers);
    let floating = modal::picture(app, area);
    layers.extend(floating.layers);
    layers.extend(shell::toasts(app, regions));
    let modal = app.modal.is_some();
    let cursor = if modal { floating.cursor } else { body.cursor };
    Picture::with_cursor(layers, cursor.filter(|position| area.contains(*position)))
}

/// Paints `app` into whichever frame the terminal provides.
pub fn frame(app: &App) -> impl FnOnce(&mut Frame<'_>) + '_ {
    move |frame| {
        let shown = picture(app, frame.area());
        for layer in shown.layers {
            match layer {
                Layer::Text(paragraph, area) => frame.render_widget(*paragraph, area),
                Layer::Blank(area) => {
                    frame.render_widget(Clear, area);
                    frame.render_widget(Block::default().style(app.theme.canvas()), area);
                }
                Layer::Tint(style, area) => {
                    frame.render_widget(Block::default().style(style), area);
                }
            }
        }
        if let Some(position) = shown.cursor {
            frame.set_cursor_position(position);
        }
    }
}

pub use modal::Hit as ModalHit;
pub fn modal_hit(app: &App, position: Position) -> Option<ModalHit> {
    modal::hit(app, position)
}
pub fn form_focus(app: &App, form: &super::form::Form, position: Position) -> Option<usize> {
    pages::form_focus(app, form, (regions(app.area).body, position))
}
