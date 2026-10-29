//! Materials list, provenance/take/usage detail and the music and effects licence list.
use crate::shell::materials::display_name;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*, px};

use super::{error_notice, heading, heading_small, muted, notice};
use crate::shell::{Shell, materials::MaterialTask};

impl Shell {
    pub(super) fn materials_page(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(panel) = &self.materials else {
            return div();
        };
        let body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                Button::new("desktop-materials-back")
                    .self_start()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::ChevronLeft)
                    .ml(gpui_kit::px(-10.))
                    .label("Back to video")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.materials = None;
                        cx.notify();
                    })),
            )
            .child(heading("Materials"))
            .child(muted(
                "Every file this video uses, where it came from, and where it appears.",
                p,
            ))
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            });
        let Some(items) = &panel.items else {
            return body.child(super::skeleton(4, p));
        };
        let selected = panel.detail.as_ref().map(|detail| detail.id.clone());
        let mut highlighted = false;
        let rows = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let id = item.id.clone();
                let chosen = !highlighted && selected.as_deref() == Some(item.id.as_str());
                highlighted |= chosen;
                let healthy = item.status == "valid";
                div()
                    .id(("desktop-material", index))
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(34.))
                    .px_3()
                    .border_b_1()
                    .border_color(p.border.opacity(0.5))
                    .when(chosen, |row| row.bg(p.accent_fill.opacity(0.18)))
                    .hover(move |style| style.bg(p.text.opacity(0.05)))
                    .child(
                        gpui_kit::component::Icon::new(material_icon(&item.kind))
                            .size(px(16.))
                            .text_color(p.secondary),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(display_name(&item.name)),
                    )
                    .child(
                        div()
                            .w(px(110.))
                            .text_color(p.secondary)
                            .child(kind_label(&item.kind)),
                    )
                    .when(!healthy, |row| {
                        row.child(div().text_color(p.error).child("Needs attention"))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.material_task(MaterialTask::Detail(id.clone()), cx)
                    }))
                    .test_support()
            })
            .collect::<Vec<_>>();
        body.when(items.is_empty(), |body| {
            body.child(muted("This video has no saved materials yet.", p))
        })
        .child(
            div()
                .flex()
                .flex_col()
                .rounded_lg()
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .overflow_hidden()
                .children(rows),
        )
        .child(self.material_detail(cx))
        .child(self.audio_library())
        .child(self.audio_import(cx))
    }

    fn material_detail(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(detail) = self
            .materials
            .as_ref()
            .and_then(|panel| panel.detail.as_ref())
        else {
            return div();
        };
        let verified = self.materials.as_ref().and_then(|panel| panel.verified);
        let id = detail.id.clone();
        let provenance = &detail.provenance;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small(&detail.name))
            .child(muted(
                &format!("{} · {} bytes · {}", detail.kind, detail.bytes, detail.path),
                p,
            ))
            .child(muted(
                &format!(
                    "Made by {} ({}) in {} at {}",
                    provenance.provider,
                    provenance.source,
                    provenance.stage,
                    super::when(&provenance.created_at.to_string())
                ),
                p,
            ))
            .when_some(detail.take.as_ref(), |body, take| {
                body.child(muted(
                    &format!(
                        "Take {} of {}{}",
                        take.take_number,
                        take.stage,
                        if take.is_selected { " · selected" } else { "" }
                    ),
                    p,
                ))
            })
            .child(heading_small(&format!(
                "Used in {} place(s)",
                detail.usages.len()
            )))
            .children(detail.usages.iter().map(|usage| {
                muted(
                    &format!(
                        "{} · {} · rebuilds {}",
                        usage.reference, usage.context, usage.dependent_stage
                    ),
                    p,
                )
            }))
            .child(
                Button::new("desktop-material-verify")
                    .self_start()
                    .label("Verify file integrity")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.material_task(MaterialTask::Verify(id.clone()), cx)
                    })),
            )
            .when_some(verified, |body, verified| {
                body.child(if verified {
                    notice("The saved file matches its recorded checksum.", p)
                } else {
                    error_notice("The saved file does not match its recorded checksum.", p)
                })
            })
    }

    fn audio_import(&self, cx: &mut Context<Self>) -> Div {
        use retold_app::audio::AudioCategory;
        let form = &self.audio_form;
        let categories = [
            (AudioCategory::Music, "Music"),
            (AudioCategory::Sfx, "Sound effect"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (category, label))| {
            Button::new(("desktop-audio-category", index))
                .self_start()
                .label(label)
                .selected(form.category == category)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.audio_form.category = category;
                    cx.notify();
                }))
        })
        .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Import licensed audio"))
            .child(div().flex().gap_2().children(categories))
            .child(gpui_kit::component::input::Input::new(&form.licence).aria_label("Licence"))
            .child(
                Button::new("desktop-audio-commercial")
                    .self_start()
                    .label(if form.commercial_use {
                        "Commercial use allowed"
                    } else {
                        "No commercial use"
                    })
                    .selected(form.commercial_use)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.audio_form.commercial_use = !this.audio_form.commercial_use;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("desktop-audio-import")
                    .self_start()
                    .label("Choose audio file…")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.import_audio(cx))),
            )
    }

    fn audio_library(&self) -> Div {
        let p = self.palette();
        let Some(audio) = self
            .materials
            .as_ref()
            .and_then(|panel| panel.audio.as_ref())
        else {
            return div();
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(heading_small("Music and effects"))
            .when(audio.is_empty(), |body| {
                body.child(muted(
                    "No licensed music or effects imported. Use `retold library audio import`.",
                    p,
                ))
            })
            .children(audio.iter().map(|item| {
                let licence = &item.license;
                muted(
                    &format!(
                        "{} · {} · {} · {}{}",
                        item.title,
                        item.category.as_str(),
                        licence.license_type,
                        if licence.commercial_use {
                            "commercial use"
                        } else {
                            "no commercial use"
                        },
                        if item.verified {
                            " · verified"
                        } else {
                            " · unverified"
                        }
                    ),
                    p,
                )
            }))
    }
}

fn kind_label(kind: &str) -> String {
    match kind {
        "image" => "Image".into(),
        "audio" => "Audio".into(),
        "video" => "Video".into(),
        "captions" | "subtitle" | "subtitles" => "Captions".into(),
        other => retold_cli::forms::label(other),
    }
}

fn material_icon(kind: &str) -> gpui_kit::assets::IconName {
    use gpui_kit::assets::IconName;
    match kind {
        "audio" => IconName::AudioWaveform,
        "video" => IconName::Film,
        "image" => IconName::Image,
        "caption" | "captions" | "subtitle" => IconName::Captions,
        _ => IconName::FileText,
    }
}
