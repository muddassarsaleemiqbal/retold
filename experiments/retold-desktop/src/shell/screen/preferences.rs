//! Presentation controls and recent workspaces are independent of production permissions.
use gpui_kit::component::{Disableable as _, Selectable as _, Sizable as _, button::Button};
use gpui_kit::{Context, Div, div, prelude::*};

use super::{heading_small, muted, segmented, setting_row};
use crate::shell::Shell;
use retold_app::desktop::preferences::Appearance;

impl Shell {
    pub(super) fn appearance_settings(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(heading_small("Appearance and text size"))
            .child(setting_row(
                "Appearance",
                segmented(
                    [
                        (Appearance::System, "System"),
                        (Appearance::Light, "Light"),
                        (Appearance::Dark, "Dark"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (mode, label))| {
                        Button::new(("desktop-color-mode", index))
                            .label(if self.high_contrast && self.appearance_mode == mode {
                                format!("✓ {label}")
                            } else {
                                label.into()
                            })
                            .accessibility_label(format!("Appearance {label}"))
                            .selected(self.appearance_mode == mode)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.choose_appearance(mode, window, cx)
                            }))
                    }),
                    p,
                ),
                p,
            ))
            .child(setting_row(
                "Text size",
                segmented(
                    [100, 125, 150, 200]
                        .into_iter()
                        .enumerate()
                        .map(|(index, zoom)| {
                            Button::new(("desktop-text-size", index))
                                .label(if self.high_contrast && self.zoom == zoom {
                                    format!("✓ {zoom}%")
                                } else {
                                    format!("{zoom}%")
                                })
                                .accessibility_label(format!("Text size {zoom} percent"))
                                .selected(self.zoom == zoom)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.change_zoom(zoom, window, cx)
                                }))
                        }),
                    p,
                ),
                p,
            ))
            .child(setting_row(
                "Increase contrast",
                {
                    let shell = cx.entity().downgrade();
                    div().flex().child(
                        gpui_kit::component::switch::Switch::new("desktop-high-contrast")
                            .checked(self.high_contrast)
                            .when(self.zoom >= 150, |switch| switch.large())
                            .accessibility_label("Increase contrast")
                            .on_click(move |_, window, app| {
                                let _ =
                                    shell.update(app, |this, cx| this.toggle_contrast(window, cx));
                            }),
                    )
                },
                p,
            ))
            .child(setting_row(
                "Window",
                div().flex().child(
                    Button::new("desktop-remember-window")
                        .self_start()
                        .label("Save current window size")
                        .on_click(
                            cx.listener(|this, _, window, cx| {
                                this.remember_window_size(window, cx)
                            }),
                        ),
                ),
                p,
            ))
            .when(self.preference_error.is_some(), |body| {
                body.child(
                    Button::new("desktop-retry-preferences")
                        .self_start()
                        .label("Retry preferences")
                        .on_click(
                            cx.listener(|this, _, window, cx| {
                                this.remember_window_size(window, cx)
                            }),
                        ),
                )
            })
    }

    pub(super) fn recent_workspaces(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when(!self.preferences.recent_workspaces.is_empty(), |body| {
                body.child(heading_small("Recent workspaces"))
            })
            .children(
                self.preferences
                    .recent_workspaces
                    .iter()
                    .enumerate()
                    .map(|(index, root)| {
                        let root = root.clone();
                        let location = root.display().to_string();
                        let label = root
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Open recent workspace")
                            .to_owned();
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                Button::new(("desktop-recent-workspace", index))
                                    .self_start()
                                    .label(label)
                                    .accessibility_label(format!(
                                        "Open workspace {}",
                                        root.display()
                                    ))
                                    .disabled(
                                        self.busy
                                            || self.control_busy
                                            || self.has_unsaved_setup(cx),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_workspace(root.clone(), cx)
                                    })),
                            )
                            .child(muted(&location, p))
                    }),
            )
    }
}
