//! What a section shows before anything is opened: a row of tabs, each a dashboard, a collection
//! or the section's full list of actions. Cursor, filter and scroll are kept per section and tab,
//! so switching away and back returns to the same place.
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::collection::{self, Collection};
use super::input::Input;
use super::section::Section;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Home: getting started, what to continue, spend at a glance.
    Dashboard,
    /// Spend by window and grouping, with limits.
    Spend,
    Report(&'static super::report::Report),
    Collection(&'static Collection),
    /// Every action in the section, each opening its form.
    Actions,
}

impl Tab {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Dashboard => "Overview",
            Self::Spend => "Spend",
            Self::Report(report) => report.title,
            Self::Collection(collection) => collection.name,
            Self::Actions => "Actions",
        }
    }
}

/// Tabs before Actions, per section in sidebar order; every section ends with Actions.
static SECTION_TABS: [&[Tab]; 11] = [
    &[Tab::Dashboard],
    &[],
    &[Tab::Collection(&collection::VIDEOS)],
    &[Tab::Collection(&collection::SERIES)],
    &[
        Tab::Collection(&collection::JOBS),
        Tab::Report(&super::report::STUDIO),
    ],
    &[
        Tab::Collection(&collection::ASSETS),
        Tab::Collection(&collection::MUSIC),
        Tab::Collection(&collection::EFFECTS),
    ],
    &[
        Tab::Collection(&collection::PROVIDERS),
        Tab::Collection(&collection::ROUTES),
        Tab::Collection(&collection::HEALTH),
        Tab::Collection(&collection::LOGINS),
        Tab::Collection(&collection::DISCOVER),
    ],
    &[
        Tab::Spend,
        Tab::Collection(&collection::LIMITS),
        Tab::Collection(&collection::ATTEMPTS),
        Tab::Report(&super::report::ACCOUNT),
        Tab::Collection(&collection::PROCESSES),
        Tab::Report(&super::report::BACKUPS),
    ],
    &[
        Tab::Collection(&collection::GUIDES),
        Tab::Collection(&collection::CHANNELS),
        Tab::Collection(&collection::PRESETS),
        Tab::Collection(&collection::VIEWS),
        Tab::Collection(&collection::PROFILES),
    ],
    &[
        Tab::Collection(&collection::WORKSPACES),
        Tab::Report(&super::report::DOCTOR),
        Tab::Report(&super::report::STORAGE),
        Tab::Report(&super::report::VIDEO_STORAGE),
        Tab::Collection(&collection::DIAGNOSTICS),
    ],
    &[],
];

#[must_use]
pub fn tabs(section: Section) -> Vec<Tab> {
    let own = SECTION_TABS
        .get(section.position())
        .copied()
        .unwrap_or_default();
    own.iter().copied().chain([Tab::Actions]).collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cursor {
    pub selected: usize,
    pub filter: Input,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landing {
    pub tab: usize,
    cursors: Vec<Cursor>,
    /// Typing goes to the filter until Enter or Esc.
    pub filtering: bool,
    pub detail_scroll: u16,
}

impl Landing {
    #[must_use]
    pub fn new(section: Section) -> Self {
        let count = tabs(section).len();
        Self {
            tab: 0,
            cursors: vec![Cursor::default(); count],
            filtering: false,
            detail_scroll: 0,
        }
    }

    #[must_use]
    pub fn cursor(&self) -> &Cursor {
        self.cursors.get(self.tab).unwrap_or_else(|| empty_cursor())
    }

    #[must_use]
    pub fn selected(&self) -> usize {
        self.cursor().selected
    }

    #[must_use]
    pub fn filter(&self) -> &str {
        self.cursor().filter.text()
    }

    pub fn select(&mut self, row: usize) {
        let changed = self.selected() != row;
        self.cursors[self.tab].selected = row;
        if changed {
            self.detail_scroll = 0;
        }
    }

    /// Moves the selection within `count` rows by an arrow, page or jump key.
    pub fn step(&mut self, count: usize, code: KeyCode) {
        let row = moved(self.selected(), count, code);
        self.select(row);
    }

    /// `tab` only changes through `select_tab`, so it always names an existing cursor.
    pub fn type_filter(&mut self, key: KeyEvent) {
        let cursor = &mut self.cursors[self.tab];
        cursor.filter.edit(key);
        cursor.selected = 0;
    }

    pub fn paste_filter(&mut self, text: &str) {
        let cursor = &mut self.cursors[self.tab];
        cursor.filter.paste(text);
        cursor.selected = 0;
        self.detail_scroll = 0;
    }

    pub fn clear_filter(&mut self) {
        let cursor = &mut self.cursors[self.tab];
        cursor.filter.clear();
        cursor.selected = 0;
    }

    pub const fn select_tab(&mut self, tab: usize) {
        let count = self.cursors.len();
        if tab < count {
            self.tab = tab;
            self.filtering = false;
            self.detail_scroll = 0;
        }
    }

    pub fn step_tab(&mut self, step: isize) {
        let tabs = self.cursors.len();
        let count = tabs.cast_signed().max(1);
        let current = self.tab.cast_signed();
        let next = current.saturating_add(step).rem_euclid(count);
        self.select_tab(next.cast_unsigned());
    }

    pub const fn scroll_detail(&mut self, step: i16) {
        self.detail_scroll = self.detail_scroll.saturating_add_signed(step);
    }
}

fn empty_cursor() -> &'static Cursor {
    static EMPTY: std::sync::OnceLock<Cursor> = std::sync::OnceLock::new();
    EMPTY.get_or_init(Cursor::default)
}

/// Rows a page key jumps.
const PAGE_ROWS: usize = 10;

/// Where an arrow, page or jump key moves a selection in a list of `count` rows.
#[must_use]
pub fn moved(selected: usize, count: usize, code: KeyCode) -> usize {
    let last = count.saturating_sub(1);
    match code {
        KeyCode::Up | KeyCode::Char('k') => selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => selected.saturating_add(1).min(last),
        KeyCode::PageUp => selected.saturating_sub(PAGE_ROWS),
        KeyCode::PageDown => selected.saturating_add(PAGE_ROWS).min(last),
        KeyCode::Home | KeyCode::Char('g') => 0,
        KeyCode::End | KeyCode::Char('G') => last,
        _ => selected,
    }
}

/// Space around each tab title, and between tabs.
const TAB_PADDING: u16 = 2;
const TAB_GAP: u16 = 1;

/// Columns each tab title covers in the tab row, for drawing and for mouse clicks alike.
#[must_use]
pub fn tab_spans(section: Section) -> Vec<(u16, u16)> {
    let widths: Vec<u16> = tabs(section)
        .iter()
        .map(|tab| {
            let letters = u16::try_from(tab.title().chars().count()).unwrap_or(u16::MAX);
            letters.saturating_add(TAB_PADDING)
        })
        .collect();
    widths
        .iter()
        .scan(0_u16, |start, width| {
            let span = (*start, start.saturating_add(*width));
            *start = span.1.saturating_add(TAB_GAP);
            Some(span)
        })
        .collect()
}

/// First visible row so that `selected` stays inside a window of `height` rows.
#[must_use]
pub const fn offset(selected: usize, height: usize) -> usize {
    let fits = height == 0 || selected < height;
    if fits {
        0
    } else {
        selected.saturating_add(1).saturating_sub(height)
    }
}
