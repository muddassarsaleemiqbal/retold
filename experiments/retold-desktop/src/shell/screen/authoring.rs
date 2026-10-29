//! Guides and channels: create one, open one, and edit a field after previewing it.
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::Input,
};
use gpui_kit::{Context, Div, div, prelude::*};
use retold_app::desktop::authoring::Resource;

use super::{error_notice, heading_small, muted, notice};
use crate::shell::{Shell, authoring::AuthoringTask};

impl Shell {
    pub(super) fn authoring_controls(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.authoring;
        let blocked = self.busy || self.snapshot.is_none();
        let kinds = [(Resource::Guide, "Guide"), (Resource::Channel, "Channel")]
            .into_iter()
            .enumerate()
            .map(|(index, (resource, label))| {
                Button::new(("desktop-authoring-kind", index))
                    .self_start()
                    .label(label)
                    .selected(panel.resource == resource)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.authoring.resource = resource;
                        this.authoring.document = None;
                        this.authoring.preview = None;
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        let action = |id: &'static str, label: &'static str, task: AuthoringTask| {
            Button::new(id)
                .self_start()
                .label(label)
                .disabled(blocked)
                .on_click(cx.listener(move |this, _, _, cx| this.authoring_task(task, cx)))
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Guides and channels"))
            .child(div().flex().gap_2().children(kinds))
            .child(Input::new(&panel.id).aria_label("Guide or channel ID"))
            .child(Input::new(&panel.language).aria_label("Language"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(action(
                        "desktop-authoring-preview-create",
                        "Review new",
                        AuthoringTask::PreviewCreate,
                    ))
                    .child(
                        action("desktop-authoring-create", "Create", AuthoringTask::Create)
                            .primary(),
                    )
                    .child(action(
                        "desktop-authoring-open",
                        "Open existing",
                        AuthoringTask::Open,
                    )),
            )
            .when_some(panel.document.as_ref(), |body, document| {
                body.child(muted(
                    &format!(
                        "{} · hash {} · missing: {}",
                        document.id,
                        &document.hash[..12.min(document.hash.len())],
                        if document.missing.is_empty() {
                            "nothing".to_owned()
                        } else {
                            document.missing.join(", ")
                        }
                    ),
                    p,
                ))
                .child(Input::new(&panel.field).aria_label("Field to change"))
                .child(Input::new(&panel.value).aria_label("New value"))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(action(
                            "desktop-authoring-preview-edit",
                            "Review change",
                            AuthoringTask::PreviewEdit,
                        ))
                        .child(
                            action("desktop-authoring-edit", "Save change", AuthoringTask::Edit)
                                .primary(),
                        ),
                )
            })
            .when_some(panel.preview.as_ref(), |body, preview| {
                body.child(muted(preview, p))
            })
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            })
            .when_some(panel.message.as_ref(), |body, message| {
                body.child(notice(message, p))
            })
    }
}
