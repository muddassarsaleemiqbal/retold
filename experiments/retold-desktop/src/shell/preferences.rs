//! Serialized, coalesced preference saves keep filesystem work outside render and input callbacks.
use gpui_kit::{Context, Window, px};
use retold_app::desktop::preferences::{self, Appearance, AppearanceSettings, Change, WindowSize};

use super::Shell;

impl Shell {
    /// Production startup opts into restoring presentation state; fixture shells remain isolated.
    pub fn restore_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.preferences_enabled {
            return;
        }
        self.preferences_enabled = true;
        self.busy = true;
        let presentation_generation = self.presentation_generation;
        let initial_size = window.viewport_size();
        let work = cx
            .background_executor()
            .spawn(async { preferences::load() });
        cx.spawn_in(window, async move |this, cx| {
            let saved = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.discover_recovery(cx);
                match saved {
                    Ok(saved) => {
                        if this.presentation_generation == presentation_generation {
                            this.appearance_mode = saved.appearance.mode;
                            this.high_contrast = saved.appearance.high_contrast;
                            this.zoom = saved.zoom;
                        }
                        this.apply_appearance(window, cx);
                        window.set_rem_size(px(16. * f32::from(this.zoom) / 100.));
                        if let Some(size) = saved.window && window.viewport_size() == initial_size {
                            window.resize(gpui_kit::size(px(size.width as f32), px(size.height as f32)));
                        }
                        let workspace = saved.recent_workspaces.first().cloned();
                        this.preferences = saved;
                        if let Some(root) = workspace { this.open_workspace(root, cx); }
                        else { this.status = "Choose a workspace to begin. No jobs have been resumed.".into(); }
                    }
                    Err(error) => {
                        this.preference_error = Some(error.message);
                        this.status = "Window preferences could not be restored. Studio is using its defaults.".into();
                    }
                }
                cx.notify();
            });
        }).detach();
    }

    pub(super) fn palette(&self) -> crate::theme::Palette {
        crate::theme::Palette::with_contrast(self.light, self.high_contrast)
    }

    pub(super) fn appearance_settings_value(&self) -> AppearanceSettings {
        AppearanceSettings {
            mode: self.appearance_mode,
            high_contrast: self.high_contrast,
        }
    }

    pub(super) fn apply_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.light = match self.appearance_mode {
            Appearance::System => matches!(
                window.appearance(),
                gpui_kit::WindowAppearance::Light | gpui_kit::WindowAppearance::VibrantLight
            ),
            Appearance::Light => true,
            Appearance::Dark => false,
        };
        crate::theme::apply_contrast(self.light, self.high_contrast, cx);
        crate::theme::zoom(self.zoom, cx);
        cx.notify();
    }

    pub(super) fn choose_appearance(
        &mut self,
        mode: Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.presentation_generation = self.presentation_generation.wrapping_add(1);
        self.appearance_mode = mode;
        self.apply_appearance(window, cx);
        self.queue_preference(Change::Appearance(self.appearance_settings_value()), cx);
    }

    pub(super) fn toggle_contrast(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.presentation_generation = self.presentation_generation.wrapping_add(1);
        self.high_contrast = !self.high_contrast;
        self.apply_appearance(window, cx);
        self.queue_preference(Change::Appearance(self.appearance_settings_value()), cx);
    }

    pub(super) fn queue_preference(&mut self, change: Change, cx: &mut Context<Self>) {
        if !self.preferences_enabled {
            return;
        }
        self.pending_preferences
            .retain(|pending| !pending.same_field(&change));
        self.pending_preferences.push(change);
        self.save_preferences(cx);
    }

    fn save_preferences(&mut self, cx: &mut Context<Self>) {
        if self.preference_saving || self.pending_preferences.is_empty() {
            return;
        }
        let changes = std::mem::take(&mut self.pending_preferences);
        let failed_changes = changes.clone();
        self.preference_saving = true;
        let work = cx
            .background_executor()
            .spawn(async move { preferences::update(&changes) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.preference_saving = false;
                match result {
                    Ok(saved) => {
                        this.preferences = saved;
                        this.preference_error = None;
                    }
                    Err(error) => {
                        this.preference_error = Some(error.message);
                        for change in failed_changes {
                            if !this
                                .pending_preferences
                                .iter()
                                .any(|pending| pending.same_field(&change))
                            {
                                this.pending_preferences.push(change);
                            }
                        }
                        if this.quit_after_preferences {
                            cx.quit();
                        }
                        cx.notify();
                        return;
                    }
                }
                this.save_preferences(cx);
                if this.quit_after_preferences && !this.preference_saving {
                    cx.quit();
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn change_zoom(&mut self, zoom: u16, window: &mut Window, cx: &mut Context<Self>) {
        self.presentation_generation = self.presentation_generation.wrapping_add(1);
        self.zoom = zoom.clamp(100, 200);
        crate::theme::zoom(self.zoom, cx);
        window.set_rem_size(px(16. * f32::from(self.zoom) / 100.));
        self.queue_preference(Change::Zoom(self.zoom), cx);
        cx.notify();
    }

    pub(super) fn remember_window_size(&mut self, window: &Window, cx: &mut Context<Self>) {
        let size = window.viewport_size();
        let width = f32::from(size.width).round().clamp(760., 7680.) as u32;
        let height = f32::from(size.height).round().clamp(640., 7680.) as u32;
        self.queue_preference(Change::Window(WindowSize { width, height }), cx);
    }

    pub(super) fn finish_quit(&mut self, cx: &mut Context<Self>) {
        if self.wait_checkpoints_before_quit(cx) {
            return;
        }
        self.quit_after_preferences = true;
        self.save_preferences(cx);
        if self.preference_saving {
            self.status = "Saving window preferences before closing…".into();
        } else {
            cx.quit();
        }
        cx.notify();
    }
}
