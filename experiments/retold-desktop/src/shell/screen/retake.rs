//! Retake request and its reviewed confirmation for one saved output.
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*};
use retold_app::retake::Retake;

use super::notice;
use crate::shell::Shell;

impl Shell {
    pub(super) fn retake_controls(
        &self,
        target: Retake,
        label: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.palette();
        let review = self.retake_matches(&target);
        let requested = target.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Button::new("desktop-retake-review")
                    .self_start()
                    .label(label.to_owned())
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.review_retake(requested.clone(), cx);
                    })),
            )
            .when_some(review, |body, review| {
                body.child(notice(
                    &format!(
                        "Makes a new version on the next reviewed run. Rebuild: {}.",
                        review.plan.affected_stages.join(", ")
                    ),
                    p,
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("desktop-retake-confirm")
                                .self_start()
                                .primary()
                                .label("Confirm retake")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.confirm_retake(cx))),
                        )
                        .child(
                            Button::new("desktop-retake-cancel")
                                .self_start()
                                .ghost()
                                .label("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.retake = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
    }
}
