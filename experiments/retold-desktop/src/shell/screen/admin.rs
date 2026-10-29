//! Health, repair review, shareable diagnostics, presets and profiles.
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*};
use retold_app::desktop::admin;

use super::{error_notice, heading_small, muted, notice};
use crate::shell::{Shell, admin::AdminTask, authoring_forms::PresetField};
use retold_app::presets::{
    VALID_ASPECTS, VALID_AUDIO_CODECS, VALID_CAPTION_MODES, VALID_CONTAINERS, VALID_VIDEO_CODECS,
};

impl Shell {
    pub(super) fn diagnostics(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.admin;
        let body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Workspace health"))
            .child(
                Button::new("desktop-health-check")
                    .self_start()
                    .label("Check workspace health")
                    .disabled(self.busy || self.snapshot.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.admin_task(AdminTask::Load, cx))),
            )
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            })
            .when_some(panel.message.as_ref(), |body, message| {
                body.child(notice(message, p))
            });
        let Some(report) = &panel.health else {
            return body.child(muted(
                "Checking reads the workspace only; repairs need a separate confirmation.",
                p,
            ));
        };
        let fixable = report.checks.iter().filter(|check| check.fixable).count();
        let library = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.context.library.clone())
            .unwrap_or_default();
        let shared = admin::redacted(report, &library);
        body.child(if report.healthy {
            notice("Healthy.", p)
        } else {
            error_notice("Needs attention.", p)
        })
        .children(report.checks.iter().map(|check| {
            muted(
                &format!("{} · {} · {}", check.name, check.status, check.message),
                p,
            )
        }))
        .child(muted(
            &format!(
                "{} videos use {} bytes; {} orphaned files.",
                report.disk.video_count,
                report.disk.total_bytes,
                report.disk.orphaned_files.len()
            ),
            p,
        ))
        .when(fixable > 0, |body| {
            body.child(
                Button::new("desktop-health-repair")
                    .self_start()
                    .primary()
                    .label(format!("Apply {fixable} listed repair(s)"))
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.admin_task(AdminTask::Repair, cx))),
            )
        })
        .child(
            Button::new("desktop-health-copy")
                .self_start()
                .ghost()
                .label("Copy shareable diagnostics")
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(shared.clone()))
                })),
        )
        .child(muted(
            "Shareable diagnostics replace workspace and home paths and contain no credentials.",
            p,
        ))
    }

    pub(super) fn presets_and_profiles(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.admin;
        let presets = panel
            .presets
            .iter()
            .flatten()
            .enumerate()
            .map(|(index, preset)| {
                let (ask, name) = (preset.name.clone(), preset.name.clone());
                let confirming = panel.removing.as_deref() == Some(preset.name.as_str());
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .items_center()
                    .child(format!(
                        "{} · {} · {}",
                        preset.name,
                        preset.container,
                        preset.aspects.join(", ")
                    ))
                    .when(!confirming, |row| {
                        row.child(
                            Button::new(("desktop-preset-remove", index))
                                .self_start()
                                .ghost()
                                .label("Remove")
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.admin.removing = Some(ask.clone());
                                    cx.notify();
                                })),
                        )
                    })
                    .when(confirming, |row| {
                        row.child(
                            Button::new(("desktop-preset-confirm", index))
                                .self_start()
                                .primary()
                                .label(format!("Confirm removing {}", preset.name))
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.admin_task(AdminTask::RemovePreset(name.clone()), cx)
                                })),
                        )
                    })
            })
            .collect::<Vec<_>>();
        let (profiles, active) = panel.profiles.clone().unwrap_or_default();
        let profile_rows = profiles
            .iter()
            .enumerate()
            .map(|(index, profile)| {
                let name = profile.name.clone();
                let current = active.as_deref() == Some(profile.name.as_str());
                Button::new(("desktop-profile-use", index))
                    .self_start()
                    .label(if current {
                        format!("{} · active", profile.name)
                    } else {
                        format!("Use {}", profile.name)
                    })
                    .disabled(self.busy || current)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.admin_task(AdminTask::UseProfile(name.clone()), cx)
                    }))
            })
            .collect::<Vec<_>>();
        let loaded = panel.presets.is_some();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Export presets"))
            .when(!loaded, |body| {
                body.child(muted(
                    "Check workspace health to load presets and profiles.",
                    p,
                ))
            })
            .when(loaded && presets.is_empty(), |body| {
                body.child(muted(
                    "No export presets. Create one with `retold export-presets new`.",
                    p,
                ))
            })
            .children(presets)
            .child(self.preset_form(cx))
            .child(heading_small("Configuration profiles"))
            .when(loaded && profile_rows.is_empty(), |body| {
                body.child(muted("No configuration profiles.", p))
            })
            .child(div().flex().flex_wrap().gap_2().children(profile_rows))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        gpui_kit::component::input::Input::new(&self.forms.profile_name)
                            .aria_label("New profile name"),
                    )
                    .child(
                        Button::new("desktop-profile-create")
                            .self_start()
                            .label("Create profile")
                            .disabled(self.busy || self.snapshot.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.submit_profile(cx))),
                    ),
            )
    }

    fn preset_form(&self, cx: &mut Context<Self>) -> Div {
        let preset = &self.forms.preset;
        let rows = [
            (
                PresetField::Aspect,
                "Aspects",
                VALID_ASPECTS,
                preset.aspects.clone(),
            ),
            (
                PresetField::Container,
                "Container",
                VALID_CONTAINERS,
                vec![preset.container.clone()],
            ),
            (
                PresetField::Video,
                "Video",
                VALID_VIDEO_CODECS,
                vec![preset.video_codec.clone()],
            ),
            (
                PresetField::Audio,
                "Audio",
                VALID_AUDIO_CODECS,
                vec![preset.audio_codec.clone()],
            ),
            (
                PresetField::Captions,
                "Captions",
                VALID_CAPTION_MODES,
                vec![preset.caption_mode.clone()],
            ),
        ];
        let rows = rows
            .into_iter()
            .enumerate()
            .map(|(row, (field, label, choices, chosen))| {
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .items_center()
                    .child(label.to_owned())
                    .children(choices.iter().enumerate().map(|(index, choice)| {
                        let value = (*choice).to_owned();
                        Button::new(("desktop-preset-choice", row * 16 + index))
                            .self_start()
                            .label(*choice)
                            .selected(chosen.iter().any(|item| item == choice))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.choose_preset_value(field, &value, cx)
                            }))
                    }))
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(heading_small("New export preset"))
            .child(
                gpui_kit::component::input::Input::new(&self.forms.preset_name)
                    .aria_label("New preset name"),
            )
            .children(rows)
            .child(
                Button::new("desktop-preset-create")
                    .self_start()
                    .label("Create preset")
                    .disabled(self.busy || self.snapshot.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.submit_preset(cx))),
            )
    }
}
