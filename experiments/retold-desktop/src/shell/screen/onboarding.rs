//! Account creation (Budget, when no account exists) and provider key login (Providers).
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::Input,
};
use gpui_kit::{Context, Div, div, prelude::*};

use super::{error_notice, heading_small, muted, notice};
use crate::shell::{Shell, onboarding::SetupTask};

impl Shell {
    pub(super) fn account_setup(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.onboarding;
        if self.budget.report.is_some() {
            return div();
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Set up the installation account"))
            .child(muted(
                "The account records what you spend and enforces your limits.",
                p,
            ))
            .child(
                Button::new("desktop-account-preview")
                    .self_start()
                    .label("Review account set-up")
                    .disabled(self.busy)
                    .on_click(
                        cx.listener(|this, _, _, cx| {
                            this.setup_task(SetupTask::PreviewAccount, cx)
                        }),
                    ),
            )
            .when_some(panel.account_preview.as_ref(), |body, preview| {
                body.child(muted(preview, p)).child(
                    Button::new("desktop-account-create")
                        .self_start()
                        .primary()
                        .label("Create the account")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.setup_task(SetupTask::CreateAccount, cx)
                        })),
                )
            })
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            })
            .when_some(panel.message.as_ref(), |body, message| {
                body.child(notice(message, p))
            })
    }

    pub(super) fn reconciliation(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.onboarding;
        let nothing_pending = self.budget.report.as_ref().is_some_and(|report| {
            report.total.unresolved == 0
                && report
                    .carried
                    .as_ref()
                    .is_none_or(|carried| carried.unresolved == 0)
        });
        if nothing_pending && panel.reconcile.is_none() {
            return div();
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Correct a pending charge"))
            .child(muted(
                "If a provider's bill shows the real cost of a pending charge, record it here.",
                p,
            ))
            .child(super::setting_row(
                "Pending charge",
                div()
                    .w(gpui_kit::px(320.))
                    .child(Input::new(&panel.attempt).aria_label("Held attempt ID")),
                p,
            ))
            .child(super::setting_row(
                "Actual cost (USD)",
                div()
                    .w(gpui_kit::px(240.))
                    .child(Input::new(&panel.cost).aria_label("Actual cost in USD")),
                p,
            ))
            .child(
                Button::new("desktop-reconcile-preview")
                    .self_start()
                    .label("Review reconciliation")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.setup_task(SetupTask::PreviewReconcile, cx)
                    })),
            )
            .when_some(panel.reconcile.as_ref(), |body, (_, _, preview)| {
                body.child(muted(&preview.to_string(), p)).child(
                    Button::new("desktop-reconcile-confirm")
                        .self_start()
                        .primary()
                        .label("Record reviewed cost")
                        .disabled(self.busy)
                        .on_click(
                            cx.listener(|this, _, _, cx| this.setup_task(SetupTask::Reconcile, cx)),
                        ),
                )
            })
    }

    pub(super) fn provider_login(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.onboarding;
        let providers: Vec<String> = self
            .providers
            .snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .providers
                    .iter()
                    .map(|provider| provider.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let chosen = panel.login.as_ref().map(|(id, _)| id.clone());
        let choices = providers
            .iter()
            .enumerate()
            .map(|(index, provider)| {
                let id = provider.clone();
                Button::new(("desktop-login-provider", index))
                    .self_start()
                    .label(provider.clone())
                    .selected(chosen.as_deref() == Some(provider.as_str()))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.setup_task(SetupTask::Status(id.clone()), cx)
                    }))
            })
            .collect::<Vec<_>>();
        div().flex().flex_col().gap_2()
            .child(heading_small("Provider login"))
            .child(muted("Choose a provider to check its sign-in. Retold stores only the name of the variable that holds your key.", p))
            .child(div().flex().flex_wrap().gap_2().children(choices))
            .when_some(panel.login.as_ref(), |body, (_, status)| {
                body.child(muted(&format!("{} · {} · {}", status.kind, status.status, status.detail), p))
                    .child(Input::new(&panel.key_env).aria_label("Key environment variable"))
                    .child(Button::new("desktop-login-preview").self_start().label("Review login change").disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.setup_task(SetupTask::PreviewLogin, cx))))
            })
            .when_some(panel.login_preview.as_ref(), |body, preview| {
                body.child(muted(preview, p))
                    .child(Button::new("desktop-login-save").self_start().primary().label("Save login").disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.setup_task(SetupTask::SaveLogin, cx))))
            })
    }
}
