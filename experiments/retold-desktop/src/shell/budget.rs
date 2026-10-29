//! Installation-wide reports retain the last successful result on refresh failure.
use super::Shell;
use gpui_kit::Context;
use retold_app::account::{
    report::{Grouping, Query, Report},
    types::Window,
};

pub(super) struct BudgetPanel {
    pub report: Option<Report>,
    pub error: Option<String>,
    pub window: Window,
    pub grouping: Grouping,
    pub page: usize,
}

impl BudgetPanel {
    pub fn new() -> Self {
        Self {
            report: None,
            error: None,
            window: Window::Month,
            grouping: Grouping::Provider,
            page: 0,
        }
    }
}

impl Shell {
    pub(super) fn load_budget(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.budget.error = None;
        let query = Query {
            window: self.budget.window,
            by: self.budget.grouping,
        };
        let work = cx
            .background_executor()
            .spawn(async move { retold_app::desktop::budget::inspect(query) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(report) => {
                        this.budget.report = Some(report);
                        this.budget.page = 0;
                    }
                    Err(error) => this.budget.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
