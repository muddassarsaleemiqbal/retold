use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
    input::Input,
};
use gpui_kit::{Context, Div, Styled, div, prelude::*, px};

use super::{card, heading_small, muted};
use crate::shell::Shell;

impl Shell {
    pub(super) fn run_review(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(heading_small("Review a run"))
            .child(muted(
                "Set the most this run may spend, then review what it will do.",
                p,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(muted("Spending cap (USD)", p))
                            .child(
                                Input::new(&self.allowance)
                                    .aria_label("Run allowance USD")
                                    .disabled(self.busy)
                                    .w(px(190.)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(muted("Request cap", p))
                            .child(
                                Input::new(&self.request_limit)
                                    .aria_label("Run maximum paid requests")
                                    .disabled(self.busy)
                                    .w(px(190.)),
                            ),
                    ),
            )
            .child(
                Button::new("desktop-review-run")
                    .self_start()
                    .label("Review plan")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.review_selected(cx))),
            );
        let Some(review) = &self.review else {
            return body;
        };
        let authority = &review.plan.authority;
        body = body
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .child(
                        card(
                            "Stop after",
                            if review.options.until.as_deref() == Some("storyboard") {
                                "Scene sequence"
                            } else {
                                "Ready to export"
                            },
                            p,
                        )
                        .flex_1()
                        .min_w(px(220.)),
                    )
                    .child(
                        card(
                            "Most this run may spend",
                            &format!(
                                "{} · {} paid requests",
                                usd(authority.usd_micros),
                                authority.requests
                            ),
                            p,
                        )
                        .flex_1()
                        .min_w(px(220.)),
                    ),
            )
            .when(!authority.routes.is_empty(), |body| {
                body.child(heading_small("Provider accounts"))
            })
            .children(authority.routes.iter().map(|(route, account)| {
                muted(&format!("{route}: bills to {}", account_label(account)), p)
            }))
            .child(self.blockers(&review.plan.blockers, cx))
            .when(review.plan.is_unblocked(), |body| {
                body.child(
                    Button::new("desktop-confirm-run")
                        .self_start()
                        .primary()
                        .label("Confirm reviewed run")
                        .tooltip("Changing the spending limit needs a new review")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.submit_review(cx))),
                )
            });
        body
    }
}

impl Shell {
    /// A blocker with the one action that clears it, where the app can offer one.
    pub(super) fn blocker(
        &self,
        index: usize,
        blocker: &retold_app::plan::Blocker,
        cx: &mut Context<Self>,
    ) -> Div {
        use retold_app::protocol::ErrorCode;
        let fix = match blocker.code {
            ErrorCode::AccountRequired => Some(
                Button::new(("desktop-blocker-fix", index))
                    .label("Set an account limit")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.go_to(crate::shell::Destination::Budget, cx);
                        cx.notify();
                    })),
            ),
            ErrorCode::ReviewRequired => Some(
                Button::new(("desktop-blocker-fix", index))
                    .label("Review script")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.delivery = None;
                        this.open_script(window, cx);
                        cx.notify();
                    })),
            ),
            ErrorCode::NotReady => Some(
                Button::new(("desktop-blocker-fix", index))
                    .label("Back to video")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.delivery = None;
                        this.review = None;
                        cx.notify();
                    })),
            ),
            _ => None,
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_3()
            .px_3()
            .py_2()
            .child(
                div()
                    .flex_1()
                    .min_w(px(240.))
                    .child(blocker.message.clone()),
            )
            .when_some(fix, |row, button| row.child(button.disabled(self.busy)))
    }

    /// Every blocker in one card: a heading, then one row per blocker with its fix.
    pub(super) fn blockers(
        &self,
        blockers: &[retold_app::plan::Blocker],
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.palette();
        if blockers.is_empty() {
            return super::success_notice("Nothing to fix. Ready when you are.", p);
        }
        let count = blockers.len();
        div()
            .flex()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .overflow_hidden()
            .child(div().w(px(3.)).flex_none().bg(p.attention))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px_3()
                            .pt_3()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(if count == 1 {
                                "One thing to fix first".to_owned()
                            } else {
                                format!("{count} things to fix first")
                            }),
                    )
                    .children(blockers.iter().enumerate().map(|(index, blocker)| {
                        self.blocker(index, blocker, cx)
                            .when(index + 1 < count, |row| {
                                row.border_b_1().border_color(p.border)
                            })
                    })),
            )
    }
}

/// "openrouter:OPENROUTER_API_KEY" → "OpenRouter account"; the key's variable name stays hidden.
pub(super) fn account_label(account: &str) -> String {
    let provider = account.split(':').next().unwrap_or(account);
    let name = match provider {
        "openrouter" => "OpenRouter".to_owned(),
        "openai" => "OpenAI".to_owned(),
        "elevenlabs" => "ElevenLabs".to_owned(),
        other => retold_cli::forms::label(other),
    };
    format!("{name} account")
}

pub(super) fn usd(micros: u64) -> String {
    super::budget::amount(retold_app::account::types::Unit::UsdMicros, micros)
}
