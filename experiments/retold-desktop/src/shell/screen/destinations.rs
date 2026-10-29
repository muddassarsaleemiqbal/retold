//! Copy-to-folder controls for one saved file, and the validated delivery history.
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*};
use retold_app::desktop::media::Item;

use super::{error_notice, heading_small, muted, notice};
use crate::shell::Shell;

impl Shell {
    pub(super) fn copy_controls(&self, item: &Item, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let state = &self.copies;
        let chosen = item.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Button::new("desktop-media-copy")
                    .self_start()
                    .label("Save a copy to…")
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_copy_folder(chosen.clone(), cx)
                    })),
            )
            .when_some(state.replace.as_ref(), |body, (_, folder)| {
                body.child(notice(
                    &format!(
                        "A file with this name already exists in {}.",
                        folder.display()
                    ),
                    p,
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("desktop-media-copy-replace")
                                .self_start()
                                .primary()
                                .label("Replace existing file")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.confirm_replace(cx))),
                        )
                        .child(
                            Button::new("desktop-media-copy-cancel")
                                .self_start()
                                .ghost()
                                .label("Keep existing file")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.copies.replace = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when_some(state.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            })
            .when_some(state.message.as_ref(), |body, message| {
                body.child(notice(message, p))
            })
    }

    pub(super) fn delivery_history(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let body = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(heading_small("Delivery history"))
            .child(
                Button::new("desktop-delivery-history")
                    .self_start()
                    .ghost()
                    .label("Check delivered copies")
                    .on_click(cx.listener(|this, _, _, cx| this.load_delivery_history(cx))),
            );
        let Some(history) = &self.copies.history else {
            return body;
        };
        body.when(history.is_empty(), |body| {
            body.child(muted("No copies delivered from this video yet.", p))
        })
        .children(history.iter().map(|entry| {
            let delivered = &entry.delivered;
            let state = if entry.intact {
                "matches"
            } else {
                "changed or missing"
            };
            muted(
                &format!(
                    "{} → {} · {} · {}",
                    delivered.source,
                    delivered.destination.display(),
                    delivered.delivered_at,
                    state
                ),
                p,
            )
        }))
    }
}
