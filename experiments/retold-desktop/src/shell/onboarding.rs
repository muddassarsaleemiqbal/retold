//! First-launch preparation in Budget and Providers: account creation and provider key login.
use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, Entity, Window};
use retold_app::desktop::onboarding::{self, Apply};
use retold_app::providers::LoginStatus;

use super::{Shell, account_admin::summary};

pub(super) struct OnboardingPanel {
    pub account_preview: Option<String>,
    pub login: Option<(String, LoginStatus)>,
    pub key_env: Entity<InputState>,
    pub login_preview: Option<String>,
    pub attempt: Entity<InputState>,
    pub cost: Entity<InputState>,
    pub reconcile: Option<(String, retold_app::account::Usd, serde_json::Value)>,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl OnboardingPanel {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Shell>) -> Self {
        Self {
            account_preview: None,
            login: None,
            key_env: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Environment variable holding the key, such as OPENROUTER_API_KEY")
            }),
            login_preview: None,
            attempt: cx.new(|cx| InputState::new(window, cx).placeholder("Held attempt ID")),
            cost: cx.new(|cx| InputState::new(window, cx).placeholder("Actual cost in USD")),
            reconcile: None,
            message: None,
            error: None,
        }
    }
}

pub(super) enum SetupTask {
    PreviewAccount,
    CreateAccount,
    Status(String),
    PreviewLogin,
    SaveLogin,
    PreviewReconcile,
    Reconcile,
}

enum Reply {
    AccountPreview(String),
    Status(String, Box<LoginStatus>),
    LoginPreview(String),
    ReconcilePreview(String, retold_app::account::Usd, serde_json::Value),
    Done(String),
}

impl Shell {
    pub(super) fn setup_task(&mut self, task: SetupTask, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let settings = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.context.config.clone());
        let panel = &mut self.onboarding;
        panel.error = None;
        let provider = panel.login.as_ref().map(|(id, _)| id.clone());
        let key_env = panel.key_env.read(cx).value().trim().to_owned();
        let attempt = panel.attempt.read(cx).value().trim().to_owned();
        let cost = panel
            .cost
            .read(cx)
            .value()
            .trim()
            .parse::<retold_app::account::Usd>();
        let reviewed = panel.reconcile.clone();
        self.busy = true;
        let work = cx.background_executor().spawn(async move {
            let missing = || {
                retold_app::protocol::Diagnostic::new(
                    retold_app::protocol::ErrorCode::InputRequired,
                    "Open a workspace and choose a provider first.",
                )
            };
            let timezone = onboarding::local_timezone();
            match task {
                SetupTask::PreviewAccount => {
                    onboarding::account(&timezone).map(|v| Reply::AccountPreview(summary(&v)))
                }
                SetupTask::CreateAccount => {
                    onboarding::initialize(&timezone).map(|v| Reply::Done(summary(&v)))
                }
                SetupTask::Status(id) => {
                    let status = onboarding::login_status(&settings.ok_or_else(missing)?, &id)?;
                    Ok(Reply::Status(id, Box::new(status)))
                }
                SetupTask::PreviewReconcile => {
                    let cost = cost.map_err(|_| {
                        retold_app::protocol::Diagnostic::new(
                            retold_app::protocol::ErrorCode::InputRequired,
                            "Enter the cost as a USD amount.",
                        )
                    })?;
                    let preview = onboarding::preview_reconcile(&attempt, cost)?;
                    Ok(Reply::ReconcilePreview(attempt, cost, preview))
                }
                SetupTask::Reconcile => {
                    let (attempt, cost, preview) = reviewed.ok_or_else(missing)?;
                    onboarding::reconcile(&attempt, cost, &preview)
                        .map(|v| Reply::Done(summary(&v)))
                }
                SetupTask::PreviewLogin | SetupTask::SaveLogin => {
                    let apply = if matches!(task, SetupTask::SaveLogin) {
                        Apply::Save
                    } else {
                        Apply::Preview
                    };
                    let (settings, provider) =
                        (settings.ok_or_else(missing)?, provider.ok_or_else(missing)?);
                    let value =
                        onboarding::configure_login(&settings, &provider, (&key_env, apply))?;
                    Ok(if apply == Apply::Save {
                        Reply::Done(summary(&value))
                    } else {
                        Reply::LoginPreview(summary(&value))
                    })
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let panel = &mut this.onboarding;
                match result {
                    Ok(Reply::AccountPreview(text)) => panel.account_preview = Some(text),
                    Ok(Reply::Status(id, status)) => {
                        panel.login = Some((id, *status));
                        panel.login_preview = None;
                    }
                    Ok(Reply::LoginPreview(text)) => panel.login_preview = Some(text),
                    Ok(Reply::ReconcilePreview(attempt, cost, preview)) => {
                        panel.reconcile = Some((attempt, cost, preview))
                    }
                    Ok(Reply::Done(text)) => {
                        panel.account_preview = None;
                        panel.login_preview = None;
                        panel.reconcile = None;
                        panel.message = Some(text);
                        this.budget.report = None;
                    }
                    Err(error) => panel.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
