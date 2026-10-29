//! Limit editor (Budget) and provider test review (Providers).
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::Input,
};
use gpui_kit::{Context, Div, div, prelude::*};
use retold_app::account::types::Window as Period;

use super::{error_notice, heading_small, muted, notice};
use crate::shell::{
    Shell,
    account_admin::{AccountTask, summary},
};

impl Shell {
    pub(super) fn limit_editor(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.account_admin;
        let periods = [
            (Period::Day, "Per day"),
            (Period::Month, "Per month"),
            (Period::Lifetime, "Lifetime"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (period, label))| {
            Button::new(("desktop-limit-period", index))
                .self_start()
                .label(label)
                .selected(panel.period == period)
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.account_admin.period = period;
                    this.account_admin.limit = None;
                    cx.notify();
                }))
        })
        .collect::<Vec<_>>();
        div().flex().flex_col().gap_2()
            .child(heading_small("Change the account limit"))
            .child(muted("Limits never erase recorded spend. Lowering one below current use blocks new paid work.", p))
            .child(div().flex().child(super::segmented(periods, p)))
            .child(super::setting_row("Spending cap (USD)", div().w(gpui_kit::px(240.)).child(Input::new(&panel.usd).aria_label("USD cap")), p))
            .child(super::setting_row("Request cap", div().w(gpui_kit::px(240.)).child(Input::new(&panel.requests).aria_label("Paid request cap")), p))
            .child(Button::new("desktop-limit-review").self_start().label("Review limit change").disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| this.account_task(AccountTask::ReviewLimit, cx))))
            .when_some(panel.error.as_ref(), |body, error| body.child(error_notice(error, p)))
            .when_some(panel.result.as_ref(), |body, result| body.child(notice(result, p)))
            .when_some(panel.limit.as_ref(), |body, review| {
                body.child(muted(&review.preview.get("limit_changed").map_or_else(|| summary(&review.preview), ToString::to_string), p))
                    .child(Button::new("desktop-limit-apply").self_start().primary().label("Confirm limit change").disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.account_task(AccountTask::ApplyLimit, cx))))
            })
    }

    pub(super) fn provider_tests(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.account_admin;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Test providers"))
            .child(muted("Send one short test request to each provider.", p))
            .child(
                Button::new("desktop-provider-test-review")
                    .self_start()
                    .label("Send test request…")
                    .disabled(self.busy || self.snapshot.is_none())
                    .on_click(
                        cx.listener(|this, _, _, cx| {
                            this.account_task(AccountTask::ReviewTest, cx)
                        }),
                    ),
            )
            .when_some(panel.test.as_ref(), |body, review| {
                body.child(muted(&summary(&review.plan), p)).child(
                    Button::new("desktop-provider-test-run")
                        .self_start()
                        .primary()
                        .label("Run reviewed probes")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.account_task(AccountTask::RunTest, cx)
                        })),
                )
            })
    }
}
