//! A per-video spending ceiling, set from the video page with the CLI's own operation.
use gpui_kit::Context;
use retold_app::protocol::Payload;

use super::Shell;

impl Shell {
    /// Save the ceiling typed for the open video.
    pub(super) fn set_video_budget(&mut self, cx: &mut Context<Self>) {
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        let amount = self.video_budget.read(cx).value().trim().to_owned();
        if amount.parse::<f64>().is_err() {
            self.status = "Enter the budget in dollars, such as 2.50.".into();
            cx.notify();
            return;
        }
        let arguments = vec![
            "--workspace".to_owned(),
            snapshot.context.root.display().to_string(),
            "video".to_owned(),
            "edit".to_owned(),
            "budget".to_owned(),
            reference.clone(),
            amount.clone(),
        ];
        self.busy = true;
        let work = cx
            .background_executor()
            .spawn(async move { retold_cli::run_operation(&arguments) });
        cx.spawn(async move |this, cx| {
            let (payload, text) = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.status = match payload {
                    Payload::Error { .. } => format!("Budget not changed. {}", text.trim()),
                    _ => format!("This video may now spend up to ${amount}."),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
