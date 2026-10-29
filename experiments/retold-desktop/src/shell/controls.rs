//! A separate control lane keeps Stop all available during normal reads and mutations.
use gpui_kit::Context;

use super::{
    Shell,
    backend::{Reply, Request},
};

impl Shell {
    pub(super) fn stop_all(&mut self, cx: &mut Context<Self>) {
        if self.control_busy {
            return;
        }
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let Ok(activity) = &snapshot.activity else {
            return;
        };
        let root = snapshot.context.root.clone();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let request = Request::Stop {
            root: root.clone(),
            library: snapshot.context.library.clone(),
            state: activity.state_directory.clone(),
            control_id: format!(
                "desktop-{}-{nonce}-{}",
                std::process::id(),
                self.control_generation
            ),
        };
        self.control_busy = true;
        self.control_generation = self.control_generation.wrapping_add(1);
        self.review = None;
        self.status =
            "Recording Stop all for this workspace. Completion is not yet confirmed…".into();
        let work = cx
            .background_executor()
            .spawn(async move { request.execute() });
        cx.spawn(async move |this, cx| {
            let reply = work.await;
            let _ = this.update(cx, |this, cx| {
                this.control_busy = false;
                let same_root = this.snapshot.as_ref().is_some_and(|snapshot| snapshot.context.root == root);
                if !same_root { return; }
                match reply {
                    Ok(Reply::Stopped { stop, activity, .. }) => {
                        this.status = stop_summary(&stop);
                        this.last_stop = Some(stop);
                        this.observed_job = None;
                        this.activity_options = Default::default();
                        if let Some(snapshot) = &mut this.snapshot { snapshot.activity = *activity; }
                    }
                    Err(error) => this.status = format!("Stop request is not confirmed: {error}. Inspect and retry this workspace's controls."),
                    _ => this.status = "Stop request is not confirmed: the worker returned an unexpected reply.".into(),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}

pub(super) fn stop_summary(stop: &retold_app::studio::ScopedStop) -> String {
    let pending = stop.receipt.stopped.len();
    let active = stop.receipt.stopping.len();
    if active == 0 && pending == 0 {
        return "Queue paused. Nothing was running.".to_owned();
    }
    if active == 0 {
        return format!("Queue paused. {pending} waiting jobs were stopped; nothing was running.");
    }
    let acknowledgment = if stop.worker_acknowledged {
        "The worker accepted cancellation."
    } else {
        "Worker acknowledgment is unavailable."
    };
    format!(
        "Queue placed on hold. {pending} pending jobs stopped; {active} active cancellations requested. {acknowledgment} Refresh to check their outcomes."
    )
}
