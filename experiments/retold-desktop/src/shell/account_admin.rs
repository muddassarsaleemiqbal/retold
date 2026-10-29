//! Reviewed account limit edits and provider tests.
use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, Entity, Window};
use retold_app::account::types::Window as Period;
use retold_app::desktop::account_admin::{self, LimitEdit, LimitReview, TestReview};

use super::Shell;

pub(super) struct AccountAdminPanel {
    pub period: Period,
    pub usd: Entity<InputState>,
    pub requests: Entity<InputState>,
    pub limit: Option<LimitReview>,
    pub test: Option<TestReview>,
    pub result: Option<String>,
    pub error: Option<String>,
}

impl AccountAdminPanel {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Shell>) -> Self {
        Self {
            period: Period::Month,
            usd: cx.new(|cx| InputState::new(window, cx).placeholder("USD cap, such as 5.00")),
            requests: cx.new(|cx| InputState::new(window, cx).placeholder("Paid request cap")),
            limit: None,
            test: None,
            result: None,
            error: None,
        }
    }

    fn edit(&self, cx: &Context<Shell>) -> Result<LimitEdit, String> {
        let usd = self.usd.read(cx).value().trim().to_owned();
        let requests = self.requests.read(cx).value().trim().to_owned();
        let requests = (!requests.is_empty())
            .then(|| {
                requests
                    .parse::<u64>()
                    .map_err(|_| "Enter a whole number of requests.".to_owned())
            })
            .transpose()?;
        Ok(LimitEdit {
            scope: "account".into(),
            window: self.period,
            usd: (!usd.is_empty()).then_some(usd),
            requests,
            remove: false,
        })
    }
}

pub(super) enum AccountTask {
    ReviewLimit,
    ApplyLimit,
    ReviewTest,
    RunTest,
}

enum Reply {
    Limit(Box<LimitReview>),
    Test(Box<TestReview>),
    Done(serde_json::Value),
}

impl Shell {
    pub(super) fn account_task(&mut self, task: AccountTask, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let settings = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.context.config.clone());
        let panel = &mut self.account_admin;
        panel.error = None;
        let edit = match &task {
            AccountTask::ReviewLimit => match panel.edit(cx) {
                Ok(edit) => Some(edit),
                Err(error) => {
                    panel.error = Some(error);
                    cx.notify();
                    return;
                }
            },
            _ => None,
        };
        let (limit, test) = (panel.limit.clone(), panel.test.clone());
        self.busy = true;
        let work = cx.background_executor().spawn(async move {
            let missing = || {
                retold_app::protocol::Diagnostic::new(
                    retold_app::protocol::ErrorCode::InputRequired,
                    "Review the change first.",
                )
            };
            match task {
                AccountTask::ReviewLimit => account_admin::review_limit(edit.ok_or_else(missing)?)
                    .map(|review| Reply::Limit(Box::new(review))),
                AccountTask::ApplyLimit => {
                    account_admin::apply_limit(&limit.ok_or_else(missing)?).map(Reply::Done)
                }
                AccountTask::ReviewTest => {
                    account_admin::review_test(&settings.ok_or_else(missing)?, None)
                        .map(|review| Reply::Test(Box::new(review)))
                }
                AccountTask::RunTest => account_admin::run_test(
                    &settings.ok_or_else(missing)?,
                    &test.ok_or_else(missing)?,
                )
                .map(Reply::Done),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let panel = &mut this.account_admin;
                match result {
                    Ok(Reply::Limit(review)) => panel.limit = Some(*review),
                    Ok(Reply::Test(review)) => panel.test = Some(*review),
                    Ok(Reply::Done(value)) => {
                        panel.limit = None;
                        panel.test = None;
                        panel.result = Some(summary(&value));
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

/// The human sentence of a shared result, or its compact JSON when it has none.
pub(super) fn summary(value: &serde_json::Value) -> String {
    value
        .get("text")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| value.to_string(), str::to_owned)
}
