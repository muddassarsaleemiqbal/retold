//! Home readiness checklist and explicit bundled-guide example copying.
use gpui_kit::Context;
use retold_app::desktop::authoring::{self, Preview};
use retold_app::desktop::readiness::{self, Item};

use super::{Shell, account_admin::summary};

#[derive(Default)]
pub(super) struct ReadinessPanel {
    pub items: Option<Vec<Item>>,
    pub example_preview: Option<String>,
    pub message: Option<String>,
    pub error: Option<String>,
    /// A read-only check is running; it never blocks the rest of the app.
    pub checking: bool,
}

#[derive(Clone, Copy)]
pub(super) enum ReadinessTask {
    Check,
    PreviewExample,
    CopyExample,
}

enum Reply {
    Items(Vec<Item>),
    Preview(String),
    Copied(String),
}

impl Shell {
    pub(super) fn readiness_task(&mut self, task: ReadinessTask, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let quiet = matches!(task, ReadinessTask::Check);
        if (quiet && self.readiness.checking) || (!quiet && self.busy) {
            return;
        }
        let (library, config) = (
            snapshot.context.library.clone(),
            snapshot.context.config.clone(),
        );
        self.readiness.error = None;
        if quiet {
            self.readiness.checking = true;
        } else {
            self.busy = true;
        }
        let work = cx.background_executor().spawn(async move {
            match task {
                ReadinessTask::Check => readiness::checklist(&library, &config).map(Reply::Items),
                ReadinessTask::PreviewExample => authoring::copy_example(&config, Preview::Only)
                    .map(|v| Reply::Preview(summary(&v))),
                ReadinessTask::CopyExample => authoring::copy_example(&config, Preview::Write)
                    .map(|v| Reply::Copied(summary(&v))),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if quiet {
                    this.readiness.checking = false;
                } else {
                    this.busy = false;
                }
                let panel = &mut this.readiness;
                let copied = matches!(result, Ok(Reply::Copied(_)));
                match result {
                    Ok(Reply::Items(items)) => panel.items = Some(items),
                    Ok(Reply::Preview(text)) => panel.example_preview = Some(text),
                    Ok(Reply::Copied(text)) => {
                        panel.example_preview = None;
                        panel.message = Some(text);
                    }
                    Err(error) => panel.error = Some(error.message),
                }
                if copied {
                    this.refresh(cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
