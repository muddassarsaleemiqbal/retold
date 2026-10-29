//! Live job following: one pending tick at a time, newer events only, bounded by the event window.
use std::time::Duration;

use gpui_kit::Context;

use super::Shell;

/// How often a followed, unfinished job is re-read.
const FOLLOW_INTERVAL: Duration = Duration::from_millis(1000);

impl Shell {
    pub(super) fn toggle_follow(&mut self, cx: &mut Context<Self>) {
        self.follow.enabled = !self.follow.enabled;
        self.follow.generation = self.follow.generation.wrapping_add(1);
        if self.follow.enabled {
            self.schedule_follow(cx);
        }
        cx.notify();
    }

    fn schedule_follow(&mut self, cx: &mut Context<Self>) {
        let generation = self.follow.generation;
        let job = self
            .observed_job
            .as_ref()
            .map(|job| job.page.job.id.clone());
        let timer = cx.background_executor().timer(FOLLOW_INTERVAL);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                this.follow_tick(generation, job.as_deref(), cx)
            });
        })
        .detach();
    }

    fn follow_tick(&mut self, generation: u64, job: Option<&str>, cx: &mut Context<Self>) {
        let current = self.observed_job.as_ref();
        let same_job = current.map(|observed| observed.page.job.id.as_str()) == job;
        let live = self.follow.enabled && self.follow.generation == generation && same_job;
        if !live {
            return;
        }
        let finished = current.is_none_or(|observed| observed.page.is_terminal);
        if finished {
            self.follow.enabled = false;
            cx.notify();
            return;
        }
        if !self.busy {
            self.refresh_job(false, cx);
        }
        self.schedule_follow(cx);
    }
}

#[derive(Default)]
pub(super) struct Follow {
    pub enabled: bool,
    generation: u64,
}
