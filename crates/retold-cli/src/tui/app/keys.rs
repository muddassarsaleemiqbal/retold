//! Keys: modals first, then text fields, then the open page, the sidebar or the landing.
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::super::form::Entry;
use super::super::landing::Tab;
use super::super::page::{Modal, Page};
use super::super::section::Section;
use super::{App, Focus, Tone};

/// Lines a page key scrolls.
const PAGE_STEP: u16 = 10;
/// F2 opens guided setup, F3 picks a value for the field, F6 edits it in the editor.
const SETUP_KEY: u8 = 2;
const PICK_KEY: u8 = 3;
const EDIT_KEY: u8 = 6;

impl App {
    pub(super) fn request_quit(&mut self) {
        let idle = self.running.is_empty();
        if idle {
            self.quit = true;
        } else {
            self.modal = Some(Modal::Quit);
        }
    }
    pub fn press(&mut self, key: KeyEvent) {
        let released = key.kind == KeyEventKind::Release;
        if released {
            return;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        // The file browser edits its own selection with F6.
        let browsing = matches!(self.modal, Some(Modal::Browser(_)));
        match (control, key.code) {
            (true, KeyCode::Char('c')) => self.request_quit(),
            (true, KeyCode::Char('k')) => self.open_palette(),
            (true, KeyCode::Char('r')) => self.refresh(),
            (_, KeyCode::F(PICK_KEY)) => self.pick_field(),
            (_, KeyCode::F(SETUP_KEY)) => self.open_setup(),
            (_, KeyCode::F(EDIT_KEY)) if !browsing => self.edit_path(),
            _ => self.route(key),
        }
    }

    fn route(&mut self, key: KeyEvent) {
        let filtering_events = self.stream_typing();
        if filtering_events {
            self.stream_filter_key(key);
            return;
        }
        let modal = self.modal.is_some();
        if modal {
            self.modal_key(key);
        } else if self.typing() {
            self.typing_key(key);
        } else {
            self.command_key(key);
        }
    }

    /// Whether typing currently goes into a text field.
    #[must_use]
    pub fn typing(&self) -> bool {
        let palette = matches!(self.modal, Some(Modal::Palette(_)));
        let modal = self.modal.is_some();
        let text_field = match self.page() {
            Some(Page::Form(form)) => matches!(form.entries.get(form.focus), Some(Entry::Text(_))),
            Some(_) => bool::default(),
            None => self.landing().filtering,
        };
        palette || (!modal && text_field)
    }

    pub(super) fn refresh(&mut self) {
        self.store.invalidate();
        self.toast("Refreshing…".into(), Tone::Notice);
    }

    /// Keys that mean something everywhere outside text fields.
    fn command_key(&mut self, key: KeyEvent) {
        let on_page = self.page().is_some();
        let on_sidebar = self.focus == Focus::Sidebar;
        match key.code {
            KeyCode::Char(':') => self.open_palette(),
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Char('T') => self.cycle_theme(),
            KeyCode::Char('y') => self.copy_command(),
            KeyCode::Char(digit) if digit.is_ascii_digit() => self.jump(digit),
            _ if on_page => self.page_key(key),
            _ if on_sidebar => self.sidebar_key(key.code),
            _ => self.landing_key(key.code),
        }
    }

    fn cycle_theme(&mut self) {
        self.theme = self.theme.next();
        let name = self.theme.name();
        self.toast(format!("Theme: {name}"), Tone::Notice);
    }

    fn jump(&mut self, digit: char) {
        if let Some(section) = Section::from_digit(digit) {
            self.go(section);
        }
    }

    pub const fn go(&mut self, section: Section) {
        self.section = section;
        self.focus = Focus::Main;
    }

    fn sidebar_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up | KeyCode::Char('k') => self.section = self.section.offset(-1),
            KeyCode::Down | KeyCode::Char('j') => self.section = self.section.offset(1),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
                self.focus = Focus::Main;
            }
            KeyCode::Char('q') => self.request_quit(),
            _ => {}
        }
    }

    fn landing_key(&mut self, code: KeyCode) {
        let report = self.report_command(code);
        if let Some(command) = report {
            self.report_key(command);
            return;
        }
        let bulk = self.bulk_command(code);
        if let Some(command) = bulk {
            self.bulk_key(command);
            return;
        }
        let operation = self.operation_for(code);
        if let Some(operation) = operation {
            self.operation_key(operation);
            return;
        }
        if self.money_claims(code) {
            self.money_key(code);
            return;
        }
        let provider = self.provider_claims(code);
        if provider {
            self.provider_command(code);
            return;
        }
        self.standard_landing_key(code);
    }

    fn standard_landing_key(&mut self, code: KeyCode) {
        let here = self.here();
        match code {
            KeyCode::Left | KeyCode::Char('h') => self.focus = Focus::Sidebar,
            KeyCode::Tab | KeyCode::Char(']') => self.landings[here].step_tab(1),
            KeyCode::BackTab | KeyCode::Char('[') => self.landings[here].step_tab(-1),
            KeyCode::Char('/') => self.start_filter(),
            KeyCode::Char('n') => self.create(),
            KeyCode::Char('.') => self.open_menu(),
            KeyCode::Enter => self.activate(),
            KeyCode::Char('q') => self.request_quit(),
            KeyCode::Esc => self.landings[here].clear_filter(),
            KeyCode::Char('J') | KeyCode::PageDown => {
                self.landings[here].scroll_detail(PAGE_STEP.cast_signed());
            }
            KeyCode::Char('K') | KeyCode::PageUp => {
                self.landings[here].scroll_detail(-PAGE_STEP.cast_signed());
            }
            code => self.move_selection(code),
        }
    }

    fn report_command(&self, code: KeyCode) -> Option<&'static str> {
        let Tab::Report(report) = self.tab() else {
            return None;
        };
        let KeyCode::Char(code) = code else {
            return None;
        };
        report
            .shortcuts()
            .iter()
            .find(|(key, _, _)| key.starts_with(code))
            .map(|(_, command, _)| *command)
    }

    fn report_key(&mut self, command: &str) {
        let fill = if command == "doctor" {
            vec![("fix".into(), "true".into())]
        } else {
            Vec::new()
        };
        self.open_path(command, &fill);
    }

    fn money_claims(&self, code: KeyCode) -> bool {
        self.tab() == Tab::Spend && matches!(code, KeyCode::Char('w' | 'b' | 'l' | 'a'))
    }

    fn money_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('w') => self.spend.next_window(),
            KeyCode::Char('b') => self.spend.next_group(),
            KeyCode::Char('l') => self.open_path("account limit", &Vec::new()),
            _ => self.open_path("account show", &Vec::new()),
        }
    }

    fn start_filter(&mut self) {
        let filterable = matches!(self.tab(), Tab::Collection(_) | Tab::Actions);
        let here = self.here();
        self.landings[here].filtering = filterable;
    }

    pub(super) fn move_selection(&mut self, code: KeyCode) {
        let here = self.here();
        if matches!(self.tab(), Tab::Report(_)) {
            let change = match code {
                KeyCode::Up | KeyCode::Char('k') => -1,
                KeyCode::Down | KeyCode::Char('j') => 1,
                _ => 0,
            };
            self.landings[here].scroll_detail(change);
            return;
        }
        let count = self.row_count();
        self.landings[here].step(count, code);
    }

    fn typing_key(&mut self, key: KeyEvent) {
        let in_form = matches!(self.page(), Some(Page::Form(_)));
        if in_form {
            self.form_key(key);
            return;
        }
        let here = self.here();
        match key.code {
            KeyCode::Enter | KeyCode::Down => self.landings[here].filtering = false,
            KeyCode::Esc => {
                self.landings[here].filtering = false;
                self.landings[here].clear_filter();
            }
            _ => self.landings[here].type_filter(key),
        }
    }

    fn page_key(&mut self, key: KeyEvent) {
        match self.page() {
            Some(Page::Form(_)) => self.form_key(key),
            Some(Page::Reading(_) | Page::Outcome(_)) => self.result_key(key.code),
            Some(Page::Video(_)) => self.video_key(key.code),
            None => {}
        }
    }

    fn form_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.pop();
            return;
        }
        let here = self.here();
        let top = self.pages[here].last_mut();
        let Some(Page::Form(form)) = top else {
            return;
        };
        let submitted = form.submits(key).then(|| form.clone());
        form.press(key);
        if let Some(form) = submitted {
            self.submit(form);
        }
    }

    fn result_key(&mut self, code: KeyCode) {
        if self.document_claims(code) {
            self.document_key(code);
            return;
        }
        if self.series_claims(code) {
            self.series_key(code);
            return;
        }
        let provider = self.provider_claims(code);
        if provider {
            self.provider_command(code);
            return;
        }
        let claimed = self.stream_claims(code);
        self.stream_command(code);
        if claimed {
            return;
        }
        match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace | KeyCode::Left => self.pop(),
            KeyCode::Char('e') => self.edit_again(),
            KeyCode::Char('r') => self.refresh(),
            code => self.scroll_page(code),
        }
    }

    pub(super) fn scroll_page(&mut self, code: KeyCode) {
        let here = self.here();
        let top = self.pages[here].last_mut();
        let scroll = match top {
            Some(Page::Reading(reading)) => &mut reading.scroll,
            Some(Page::Outcome(outcome)) => &mut outcome.scroll,
            Some(Page::Video(video)) => &mut video.scroll,
            _ => return,
        };
        *scroll = scrolled(*scroll, code);
    }

    fn video_key(&mut self, code: KeyCode) {
        let reviewing = self.review_claims(code);
        if reviewing {
            self.review_command(code);
            return;
        }
        match code {
            KeyCode::Tab | KeyCode::Char(']') | KeyCode::Right => self.step_video_tab(1),
            KeyCode::BackTab | KeyCode::Char('[') => self.step_video_tab(-1),
            KeyCode::Char('c') => self.video_action("video continue"),
            KeyCode::Char('e') => self.video_action("video export"),
            KeyCode::Char('a') => self.video_action("video approve"),
            KeyCode::Char('.') => self.open_menu(),
            KeyCode::Char('b') => {
                if let Some(path) = self.video_directory() {
                    self.browse(path, None);
                }
            }
            KeyCode::Char('s') => self.video_action("video edit line"),
            KeyCode::Char('h') => self.video_action("video edit shot"),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace | KeyCode::Left => self.pop(),
            code => self.scroll_page(code),
        }
    }

    fn step_video_tab(&mut self, step: isize) {
        let here = self.here();
        let top = self.pages[here].last_mut();
        if let Some(Page::Video(video)) = top {
            video.step_tab(step);
        }
        self.refresh_media();
    }
}

pub(super) const fn scrolled(scroll: u16, code: KeyCode) -> u16 {
    match code {
        KeyCode::Up | KeyCode::Char('k') => scroll.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => scroll.saturating_add(1),
        KeyCode::PageUp => scroll.saturating_sub(PAGE_STEP),
        KeyCode::PageDown | KeyCode::Char(' ') => scroll.saturating_add(PAGE_STEP),
        KeyCode::Home | KeyCode::Char('g') => 0,
        _ => scroll,
    }
}
