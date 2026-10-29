//! Kit's native, virtualized command palette preserves keyboard navigation and dialog focus.
use std::cell::Cell;

use gpui_kit::component::{
    Disableable as _, WindowExt as _,
    button::Button,
    command::{Command, CommandItem, CommandState},
};
use gpui_kit::{AppContext as _, Context, ParentElement as _, Styled as _, Window, div, px};

use super::{Destination, Shell};

gpui_kit::actions!(
    studio_navigation,
    [
        SearchCommands,
        OpenWorkspace,
        CreateVideo,
        RefreshWorkspace,
        ToggleAppearance,
        GoHome,
        GoVideos,
        GoSeries,
        GoActivity,
        GoProviders,
        GoBudget,
        GoSettings,
        GoHelp,
        PreviousScene,
        NextScene,
        ApproveCurrent
    ]
);

fn go_bindings(modifier: &str) -> Vec<gpui_kit::KeyBinding> {
    use gpui_kit::KeyBinding;
    vec![
        KeyBinding::new(&format!("{modifier}-1"), GoHome, None),
        KeyBinding::new(&format!("{modifier}-2"), GoVideos, None),
        KeyBinding::new(&format!("{modifier}-3"), GoSeries, None),
        KeyBinding::new(&format!("{modifier}-4"), GoActivity, None),
        KeyBinding::new(&format!("{modifier}-5"), GoProviders, None),
        KeyBinding::new(&format!("{modifier}-6"), GoBudget, None),
        KeyBinding::new(&format!("{modifier}-7"), GoSettings, None),
        KeyBinding::new(&format!("{modifier}-8"), GoHelp, None),
    ]
}

/// The Go menu, one item per destination with its shortcut.
#[must_use]
pub fn go_menu() -> gpui_kit::Menu {
    use gpui_kit::MenuItem;
    gpui_kit::Menu {
        name: "Go".into(),
        items: vec![
            MenuItem::action("Home", GoHome),
            MenuItem::action("Videos", GoVideos),
            MenuItem::action("Series", GoSeries),
            MenuItem::action("Activity", GoActivity),
            MenuItem::action("Providers", GoProviders),
            MenuItem::action("Budget", GoBudget),
            MenuItem::action("Settings", GoSettings),
            MenuItem::action("Help", GoHelp),
        ],
        disabled: false,
    }
}

impl Shell {
    /// Keyboard and menu navigation follow the same guards as the navigation rail.
    pub(super) fn go_from_shortcut(&mut self, destination: Destination, cx: &mut Context<Self>) {
        if self.busy || self.control_busy || self.has_unsaved_setup(cx) {
            return;
        }
        self.go_to(destination, cx);
    }
}

pub fn bind_keys(cx: &mut gpui_kit::App) {
    use gpui_kit::KeyBinding;
    cx.bind_keys([
        KeyBinding::new("cmd-k", SearchCommands, None),
        KeyBinding::new("cmd-o", OpenWorkspace, None),
        KeyBinding::new("cmd-n", CreateVideo, None),
        KeyBinding::new("cmd-r", RefreshWorkspace, None),
        KeyBinding::new("cmd-[", PreviousScene, None),
        KeyBinding::new("cmd-]", NextScene, None),
        KeyBinding::new("cmd-enter", ApproveCurrent, None),
    ]);
    cx.bind_keys(go_bindings("cmd"));
    // Other platforms use Control; on macOS a second binding would be shown instead of ⌘.
    if !cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("ctrl-k", SearchCommands, None),
            KeyBinding::new("ctrl-o", OpenWorkspace, None),
            KeyBinding::new("ctrl-n", CreateVideo, None),
            KeyBinding::new("ctrl-r", RefreshWorkspace, None),
        ]);
        cx.bind_keys(go_bindings("ctrl"));
    }
}

#[derive(Debug, Clone)]
enum Choice {
    Page(Destination),
    Open,
    Create,
    Refresh,
    Appearance,
    Stop,
    Video(String),
}

/// The Go action for a page, so the palette shows its ⌘-number shortcut.
fn go_action(destination: Destination) -> Box<dyn gpui_kit::Action> {
    match destination {
        Destination::Home => Box::new(GoHome),
        Destination::Videos => Box::new(GoVideos),
        Destination::Series => Box::new(GoSeries),
        Destination::Activity => Box::new(GoActivity),
        Destination::Providers => Box::new(GoProviders),
        Destination::Budget => Box::new(GoBudget),
        Destination::Settings => Box::new(GoSettings),
        Destination::Help => Box::new(GoHelp),
    }
}

impl Choice {
    fn icon(&self) -> gpui_kit::assets::IconName {
        use gpui_kit::assets::IconName;
        match self {
            Self::Page(destination) => destination.icon(),
            Self::Open => IconName::FolderOpen,
            Self::Create => IconName::Plus,
            Self::Refresh => IconName::RefreshCw,
            Self::Appearance => IconName::Moon,
            Self::Stop => IconName::CircleStop,
            Self::Video(_) => IconName::Film,
        }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    label: String,
    choice: Choice,
    disabled: bool,
}

impl Shell {
    pub fn search_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let entries = self.command_entries(cx);
        let state = cx.new(|cx| CommandState::new(window, cx));
        let initial_focus = Cell::new(true);
        let search_blocked = self.busy
            || self.snapshot.is_none()
            || self.draft.is_some()
            || self.import.is_some()
            || self.new_root.is_some();
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            if initial_focus.replace(false) {
                state.update(cx, |state, cx| state.focus(window, cx));
            }
            let choices = entries.clone();
            let library_search = weak.clone();
            let weak = weak.clone();
            dialog
                .close_button(false)
                .overlay(true)
                .margin_top(px(96.))
                .w(px(620.))
                .child(
                    Command::new(&state)
                        .placeholder("Search commands and visible videos…")
                        .empty(move |state, _, cx| {
                            let query = state.query(cx).to_string();
                            let weak = library_search.clone();
                            div().p_4().child(
                                Button::new("desktop-command-search-library")
                                    .label("Search all videos")
                                    .disabled(search_blocked)
                                    .on_click(move |_, window, cx| {
                                        window.close_dialog(cx);
                                        let _ = weak.update(cx, |this, cx| {
                                            this.destination = Destination::Videos;
                                            this.selected_video = None;
                                            this.show_script = false;
                                            this.show_lab = false;
                                            this.search.update(cx, |input, cx| {
                                                input.set_value(query.clone(), window, cx)
                                            });
                                            this.search_videos(cx);
                                        });
                                    }),
                            )
                        })
                        .items(entries.iter().map(|entry| {
                            let item = CommandItem::new();
                            let item = match entry.choice {
                                Choice::Page(destination) => item.action(go_action(destination)),
                                _ => item,
                            };
                            item.icon(entry.choice.icon())
                                .label(entry.label.clone())
                                .disabled(entry.disabled)
                        }))
                        .on_confirm(move |index, window, cx| {
                            let Some(entry) = choices.get(index.row) else {
                                return;
                            };
                            if entry.disabled {
                                return;
                            }
                            window.close_dialog(cx);
                            let _ = weak.update(cx, |this, cx| {
                                this.choose_command(&entry.choice, window, cx)
                            });
                        }),
                )
        });
    }

    fn command_entries(&self, cx: &Context<Self>) -> Vec<Entry> {
        let mut entries: Vec<_> = Destination::ALL
            .into_iter()
            .map(|page| Entry {
                label: page.command_label().into(),
                choice: Choice::Page(page),
                disabled: false,
            })
            .collect();
        let unsaved = self.has_unsaved_setup(cx);
        let selected = self.snapshot.is_some();
        for (label, choice, disabled) in [
            (
                "Open a workspace…",
                Choice::Open,
                self.busy || self.control_busy || unsaved,
            ),
            (
                "Create a video draft",
                Choice::Create,
                self.busy || !selected || unsaved,
            ),
            (
                "Refresh the workspace",
                Choice::Refresh,
                self.busy || !selected,
            ),
            ("Switch light or dark appearance", Choice::Appearance, false),
            (
                "Stop all jobs in this workspace",
                Choice::Stop,
                !selected || self.control_busy,
            ),
        ] {
            entries.push(Entry {
                label: label.into(),
                choice,
                disabled,
            });
        }
        entries.extend(
            self.video_items()
                .iter()
                .filter(|video| !video.is_archived)
                .map(|video| Entry {
                    label: format!(
                        "Open video: {} ({})",
                        video.title,
                        video
                            .editions
                            .iter()
                            .map(|edition| retold_app::desktop::edition_label(edition))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    choice: Choice::Video(video.reference.clone()),
                    disabled: false,
                }),
        );
        entries
    }

    fn choose_command(&mut self, choice: &Choice, window: &mut Window, cx: &mut Context<Self>) {
        match choice {
            Choice::Page(destination) => {
                self.go_to(*destination, cx);
                if *destination == Destination::Videos {
                    self.selected_video = None;
                    self.show_script = false;
                }
            }
            Choice::Open => self.choose_workspace(cx),
            Choice::Create => self.begin_draft(window, cx),
            Choice::Refresh => self.refresh(cx),
            Choice::Appearance => self.toggle_appearance(cx),
            Choice::Stop => self.stop_all(cx),
            Choice::Video(reference) => self.select_video(reference, cx),
        }
        cx.notify();
    }

    pub(super) fn select_video(&mut self, reference: &str, cx: &mut Context<Self>) {
        if self
            .scenes
            .as_ref()
            .is_some_and(|editor| editor.reference != reference && editor.dirty(cx))
        {
            self.return_to_scenes(cx);
            self.status =
                "Save or discard your scene direction before opening another video.".into();
            return;
        }
        self.show_scenes = false;
        if self
            .notes
            .as_ref()
            .is_some_and(|panel| panel.reference != reference && panel.dirty(cx))
        {
            self.return_to_notes(cx);
            self.status = "Save or clear your review note before opening another video.".into();
            return;
        }
        self.show_notes = false;
        if let Some(editor) = &self.editor
            && editor.dirty(cx)
        {
            if editor.reference == reference {
                self.show_script = true;
            } else {
                self.status =
                    "Save or discard your script changes before opening another video.".into();
                cx.notify();
                return;
            }
        } else {
            self.show_script = false;
        }
        let present = self.find_video(reference).is_some();
        if !present {
            self.status =
                "That video is no longer in the current workspace snapshot. Refresh to locate it."
                    .into();
            cx.notify();
            return;
        }
        self.selected_video = Some(reference.into());
        self.retake = None;
        self.retake_video = None;
        self.clear_media(cx);
        self.delivery = None;
        self.characters = None;
        self.destination = Destination::Videos;
        self.show_lab = false;
        self.review = None;
        cx.notify();
    }

    pub(super) fn toggle_appearance(&mut self, cx: &mut Context<Self>) {
        self.presentation_generation = self.presentation_generation.wrapping_add(1);
        self.light = !self.light;
        self.appearance_mode = if self.light {
            retold_app::desktop::preferences::Appearance::Light
        } else {
            retold_app::desktop::preferences::Appearance::Dark
        };
        crate::theme::apply_contrast(self.light, self.high_contrast, cx);
        crate::theme::zoom(self.zoom, cx);
        self.queue_preference(
            retold_app::desktop::preferences::Change::Appearance(self.appearance_settings_value()),
            cx,
        );
        cx.notify();
    }
}
