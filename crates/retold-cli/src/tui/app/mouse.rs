//! Terminal events: keys, resizes and the mouse. Clicks select and open; the wheel scrolls.
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};

use super::super::landing::{self, Tab};
use super::super::section::{self, Section};
use super::super::{dashboard, view};
use super::{App, Focus};

impl App {
    /// Applies one terminal event.
    pub fn react(&mut self, event: &Event) {
        match event {
            Event::Key(key) => self.press(*key),
            Event::Mouse(mouse) => self.mouse(*mouse),
            Event::Resize(width, height) => self.area = Rect::new(0, 0, *width, *height),
            Event::Paste(text) => self.paste(text),
            _ => {}
        }
    }

    fn mouse(&mut self, mouse: MouseEvent) {
        let position = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::ScrollDown => self.wheel(KeyCode::Down),
            MouseEventKind::ScrollUp => self.wheel(KeyCode::Up),
            MouseEventKind::Down(MouseButton::Left) => self.click(position),
            _ => {}
        }
    }

    /// A click on the sidebar, the tab row or a list row; modals take no clicks.
    fn click(&mut self, position: Position) {
        let regions = view::regions(self.area);
        let modal = self.modal.is_some();
        if modal {
            self.click_modal(position);
            return;
        }
        let on_page = self.page().is_some();
        if on_page {
            self.click_page(position);
            return;
        }
        let on_sidebar = regions.sidebar.contains(position);
        let on_tabs = regions.tabs.contains(position);
        if on_sidebar {
            self.click_sidebar(regions.sidebar, position);
        } else if on_tabs {
            self.click_tabs(regions.tabs, position);
        } else {
            self.click_list(regions.body, position);
        }
    }

    fn click_sidebar(&mut self, sidebar: Rect, position: Position) {
        let below_top = position.y.saturating_sub(sidebar.y.saturating_add(1));
        let found = section::ALL.get(usize::from(below_top)).copied();
        if let Some(section) = found {
            self.click_section(section);
        }
    }

    fn click_tabs(&mut self, tabs: Rect, position: Position) {
        let column = position.x.saturating_sub(tabs.x);
        let spans = landing::tab_spans(self.section);
        let found = spans
            .iter()
            .position(|(start, end)| (*start..*end).contains(&column));
        if let Some(tab) = found {
            self.click_tab(tab);
        }
    }

    fn click_list(&mut self, body: Rect, position: Position) {
        let (list, _) = view::split(body);
        let inside = list.contains(position) && position.y > list.y;
        let landing = self.page().is_none();
        if !(inside && landing) {
            return;
        }
        let height = view::widgets::inner_rows(list);
        let line = usize::from(position.y.saturating_sub(list.y.saturating_add(1)));
        let row = self.row_at(line, height);
        if let Some(row) = row {
            self.click_row(row);
        }
    }

    /// The row drawn on visible `line` of a list `height` rows tall.
    fn row_at(&self, line: usize, height: usize) -> Option<usize> {
        let selected = self.landing().selected();
        let dashboard = self.tab() == Tab::Dashboard;
        if !dashboard {
            return Some(line.saturating_add(landing::offset(selected, height)));
        }
        let rows = self.dashboard_rows();
        let first = landing::offset(dashboard::line_of(&rows, selected), height);
        dashboard::row_at(&rows, line.saturating_add(first))
    }

    pub fn click_section(&mut self, section: Section) {
        self.modal = Option::default();
        self.go(section);
    }

    pub fn click_tab(&mut self, tab: usize) {
        let landing = self.page().is_none();
        if landing {
            let here = self.here();
            self.landings[here].select_tab(tab);
        }
    }

    /// A click on a list row: the first click selects, a second opens.
    pub fn click_row(&mut self, row: usize) {
        let exists = row < self.row_count();
        if !exists {
            return;
        }
        self.focus = Focus::Main;
        let already = self.landing().selected() == row;
        if already {
            self.activate();
        } else {
            let here = self.here();
            self.landings[here].select(row);
        }
    }

    fn wheel(&mut self, code: KeyCode) {
        let routed = self.modal.is_some() || self.page().is_some();
        if routed {
            self.press(KeyEvent::new(code, KeyModifiers::NONE));
        } else {
            self.move_selection(code);
        }
    }
}

impl App {
    fn click_modal(&mut self, position: Position) {
        let hit = view::modal_hit(self, position);
        match hit {
            Some(view::ModalHit::Confirm) => {
                self.press(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            }
            Some(view::ModalHit::Cancel) => {
                self.press(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            }
            Some(view::ModalHit::Row(row)) => self.click_modal_row(row),
            None => {}
        }
    }
    fn click_modal_row(&mut self, row: usize) {
        use super::super::page::Modal;
        let selected = match &mut self.modal {
            Some(Modal::Palette(palette) | Modal::Values(palette))
                if row < palette.matches().len() =>
            {
                Some(&mut palette.selected)
            }
            Some(Modal::Menu(menu)) if row < menu.entries.len() => Some(&mut menu.selected),
            _ => None,
        };
        if let Some(selected) = selected {
            let active = *selected == row;
            *selected = row;
            if active {
                self.press(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            }
        }
    }
    fn click_page(&mut self, position: Position) {
        use super::super::page::Page;
        let Some(Page::Form(form)) = self.page() else {
            return;
        };
        let Some(focus) = view::form_focus(self, form, position) else {
            return;
        };
        let here = self.here();
        let Some(Page::Form(form)) = self.pages[here].last_mut() else {
            return;
        };
        form.focus = focus;
        let button = form.on_button();
        let code = if button {
            KeyCode::Enter
        } else {
            KeyCode::Char(' ')
        };
        let switch = matches!(
            form.entries.get(focus),
            Some(super::super::form::Entry::Flag(_) | super::super::form::Entry::Choice(_))
        );
        if button || switch {
            self.press(KeyEvent::new(code, KeyModifiers::NONE));
        }
    }
}
