//! Small, themed native pieces shared by the initial review workspace.

use gpui_kit::component::{
    Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::Textarea,
};
use gpui_kit::{Context, Div, FontWeight, IntoElement, Role, Styled, div, prelude::*, px};

use super::Studio;
use crate::model::Edition;
use crate::theme::Palette;

impl Studio {
    pub(super) fn header(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.light);
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .px_6()
            .py_4()
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("▷  Retold Studio"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("background-check")
                            .small()
                            .ghost()
                            .label(if self.check.is_some() {
                                "Stop check"
                            } else {
                                "Check responsiveness"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(cancel) = &this.check {
                                    cancel.cancel();
                                    this.status = "Stopping the background check…".into();
                                    cx.notify();
                                } else {
                                    this.start_check(cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("open-media")
                            .label("Open media…")
                            .on_click(cx.listener(|this, _, _, cx| this.choose_media(cx))),
                    )
                    .child(
                        Button::new("appearance")
                            .ghost()
                            .label(if self.light {
                                "Dark appearance"
                            } else {
                                "Light appearance"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.light = !this.light;
                                crate::theme::apply(this.light, cx);
                                cx.notify();
                            })),
                    ),
            )
    }

    pub(super) fn editions(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .gap_2()
            .children(
                Edition::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, edition)| {
                        Button::new(("edition", index))
                            .small()
                            .label(edition.label())
                            .accessibility_label(format!("{} edition", edition.label()))
                            .selected(self.session.edition == edition)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.edition(edition, window, cx)
                            }))
                    }),
            )
    }

    pub(super) fn inspector(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.light);
        let approved = self.session.draft().approved && !self.changed(cx);
        div().flex().flex_col().gap_4().min_w_0().p_4().bg(p.surface).rounded_lg()
            .child(label("Scene review", p.text))
            .child(div().text_color(if approved { p.accent } else { p.attention })
                .child(if approved { "Approved in this session" } else { "Waiting for your review" }))
            .child(div().text_size(px(12.)).text_color(p.secondary).child("Check the setting and narration. Approval is tied to this edition's current draft."))
            .child(Button::new("approve").primary().label("Approve in session").disabled(approved)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.capture_edits(cx);
                    this.session.approve();
                    this.status = "Current scene draft approved in this session. No production state changed.".into();
                    cx.notify();
                })))
            .child(label("Notes for this scene", p.secondary))
            .child(Textarea::new(&self.note).aria_label("Scene notes").h(px(104.)))
            .child(Button::new("save-draft").label("Keep draft").disabled(!self.changed(cx))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.capture_edits(cx);
                    this.status = "Draft kept in this session.".into();
                    cx.notify();
                })))
    }

    pub(super) fn sequence(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.light);
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(label("Scene sequence", p.text))
                    .child(label(
                        &format!("{} need review", self.session.remaining()),
                        p.attention,
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .children(
                        self.session
                            .scenes()
                            .iter()
                            .enumerate()
                            .map(|(index, scene)| {
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        Button::new(("scene", index))
                                            .label(format!("{}  {}", index + 1, scene.title))
                                            .accessibility_label(format!(
                                                "Select scene {}",
                                                index + 1
                                            ))
                                            .selected(index == self.session.selected)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.select(index, window, cx)
                                            })),
                                    )
                                    .child(label(scene.duration, p.secondary))
                            }),
                    ),
            )
    }

    pub(super) fn status(&self) -> impl IntoElement {
        let p = Palette::new(self.light);
        div()
            .id("status")
            .role(Role::Status)
            .aria_label(self.status.clone())
            .text_size(px(12.))
            .text_color(p.secondary)
            .px_6()
            .py_3()
            .border_t_1()
            .border_color(p.border)
            .child(self.status.clone())
    }
}

pub(super) fn label(text: &str, color: gpui_kit::Hsla) -> Div {
    div()
        .text_size(px(12.))
        .text_color(color)
        .child(text.to_owned())
}
