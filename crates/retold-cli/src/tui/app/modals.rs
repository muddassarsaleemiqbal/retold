//! The command palette, confirmations and item menus.
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::super::form::Form;
use super::super::landing::Tab;
use super::super::page::{Choice, Entry, Menu, Modal, Page, Palette};
use super::super::section;
use super::App;

impl App {
    pub fn open_palette(&mut self) {
        let sections = section::ALL.into_iter().map(|section| Entry {
            label: format!("Go to {}", section.title()),
            detail: section.summary().to_owned(),
            risk: Option::default(),
            choice: Choice::Section(section),
        });
        let actions = self
            .catalog
            .iter()
            .enumerate()
            .map(|(index, action)| Entry {
                label: action.label(),
                detail: action.about.clone(),
                risk: Some(action.risk),
                choice: Choice::Action {
                    index,
                    fill: self.context(action),
                },
            });
        let entries = sections.chain(actions).collect();
        self.modal = Some(Modal::Palette(Palette::new(entries)));
    }

    /// Actions for the selected item or the open video, with it filled in.
    pub(super) fn open_menu(&mut self) {
        let Some((title, groups)) = self.menu_subject() else {
            return;
        };
        let entries = self.menu_entries(&groups);
        if !entries.is_empty() {
            self.modal = Some(Modal::Menu(Menu {
                title,
                entries,
                selected: 0,
            }));
        }
    }

    fn menu_subject(&self) -> Option<(String, Vec<&'static str>)> {
        match (self.page(), self.tab()) {
            (Some(Page::Video(video)), _) => Some((video.reference.clone(), vec!["video"])),
            (None, Tab::Collection(collection)) => {
                let item = self.selected_item()?;
                Some((item.label, collection.groups.to_vec()))
            }
            _ => Option::default(),
        }
    }

    /// Creating a new one is not something to do *to* the selected item.
    fn creates(&self, command: &str) -> bool {
        let creating = match self.tab() {
            Tab::Collection(collection) => collection.create,
            _ => Option::default(),
        };
        creating == Some(command)
    }

    fn menu_entries(&self, groups: &[&str]) -> Vec<Entry> {
        let in_group = |command: &str| groups.iter().any(|group| command.starts_with(group));
        let offered = |command: &str| in_group(command) && !self.creates(command);
        self.catalog
            .iter()
            .enumerate()
            .filter(|(_, action)| offered(&action.command()))
            .filter_map(|(index, action)| {
                let fill = self.context(action);
                (!fill.is_empty()).then(|| Entry {
                    label: action.title.clone(),
                    detail: action.about.clone(),
                    risk: Some(action.risk),
                    choice: Choice::Action { index, fill },
                })
            })
            .collect()
    }

    pub(super) fn modal_key(&mut self, key: KeyEvent) {
        let Some(modal) = self.modal.take() else {
            return;
        };
        match modal {
            Modal::BulkTag(input) => self.bulk_tag_key(input, key),
            Modal::Batch(forms) => self.batch_key(forms, key.code),
            Modal::Document(editor) => self.document_edit_key(editor, key),
            Modal::Route(editor) => self.route_key(editor, key),
            Modal::Help => {}
            Modal::Quit => {
                if key.code == KeyCode::Enter {
                    self.quit_when_idle = true;
                }
            }
            Modal::Confirm(form) => self.confirm_key(form, key.code),
            Modal::Plan(review) => self.plan_key(review, key.code),
            Modal::Preview(preview) => self.preview_key(preview, key.code),
            Modal::Palette(palette) => self.palette_key(palette, key),
            Modal::Values(palette) => self.values_key(palette, key),
            Modal::Browser(browser) => self.browser_key(browser, key),
            Modal::Setup(selected) => self.setup_key(selected, key.code),
            Modal::Destroy { form, typed } => self.destroy_key(form, typed, key),
            Modal::Menu(menu) => self.menu_key(menu, key.code),
        }
    }

    fn route_key(&mut self, editor: super::super::routes::Editor, key: KeyEvent) {
        use super::super::routes::Decision;
        match editor.react(key) {
            Decision::Discard => {}
            Decision::Review { tier, order } => {
                let action = self
                    .catalog
                    .iter()
                    .find(|action| action.command() == "providers route");
                if let Some(action) = action {
                    let form = Form::new(action.clone())
                        .with("tier", &tier)
                        .with("order", &order);
                    self.confirm(form);
                }
            }
            Decision::Editing(editor) => self.modal = Some(Modal::Route(editor)),
        }
    }

    fn destroy_key(&mut self, form: Form, mut typed: super::super::input::Input, key: KeyEvent) {
        let name = form
            .value("reference")
            .or_else(|| form.value("name"))
            .unwrap_or_default();
        match key.code {
            KeyCode::Esc => self.push(Page::Form(form)),
            KeyCode::Enter if !name.is_empty() && typed.text() == name => self.run(form),
            _ => {
                typed.edit(key);
                self.modal = Some(Modal::Destroy { form, typed });
            }
        }
    }

    fn plan_key(&mut self, mut review: super::super::production::Review, code: KeyCode) {
        match code {
            KeyCode::Esc => self.push(Page::Form(review.form)),
            KeyCode::Enter => {
                let form = self
                    .store
                    .current_contents(&review.words)
                    .and_then(|contents| review.executable(contents));
                if let Some(form) = form {
                    self.run(form);
                } else {
                    self.modal = Some(Modal::Plan(review));
                }
            }
            _ => {
                review.scroll = super::keys::scrolled(review.scroll, code);
                self.modal = Some(Modal::Plan(review));
            }
        }
    }

    fn preview_key(&mut self, mut preview: super::super::preview::Preview, code: KeyCode) {
        let executable = self
            .store
            .current_contents(&preview.words)
            .and_then(|contents| preview.executable(contents));
        match code {
            KeyCode::Esc => self.push(Page::Form(preview.form)),
            KeyCode::Enter if executable.is_some() => {
                if let Some(form) = executable {
                    self.run(form);
                }
            }
            _ => {
                preview.scroll_by(code);
                self.modal = Some(Modal::Preview(preview));
            }
        }
    }

    fn confirm_key(&mut self, form: Form, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('y') => self.run(form),
            KeyCode::Esc | KeyCode::Char('n') => self.push(Page::Form(form)),
            _ => self.modal = Some(Modal::Confirm(form)),
        }
    }

    fn palette_key(&mut self, mut palette: Palette, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {}
            KeyCode::Enter => self.choose(palette.choice()),
            _ => {
                palette.press(key);
                self.modal = Some(Modal::Palette(palette));
            }
        }
    }

    fn menu_key(&mut self, mut menu: Menu, code: KeyCode) {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {}
            KeyCode::Enter => self.choose(menu.choice()),
            code => {
                menu.step(code);
                self.modal = Some(Modal::Menu(menu));
            }
        }
    }

    fn choose(&mut self, choice: Option<Choice>) {
        match choice {
            Some(Choice::Value { field, value }) => self.fill_field(&field, &value),
            Some(Choice::Section(section)) => self.go(section),
            Some(Choice::Action { index, fill }) => self.open_action(index, &fill),
            None => {}
        }
    }

    fn values_key(&mut self, mut palette: Palette, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {}
            KeyCode::Enter => self.choose(palette.choice()),
            _ => {
                palette.press(key);
                self.modal = Some(Modal::Values(palette));
            }
        }
    }
}
