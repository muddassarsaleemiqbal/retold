//! Delivery reviews have their own selection and never reuse run confirmation.
use super::{Destination, Shell};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use retold_app::{
    desktop::{self, ReviewedExport},
    plan::ExportOptions,
};
use std::path::PathBuf;

pub(super) struct DeliveryPanel {
    pub library: PathBuf,
    pub reference: String,
    pub title: String,
    pub editions: Vec<desktop::DeliveryEdition>,
    pub options: ExportOptions,
    pub allowance: Entity<InputState>,
    pub request_limit: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    pub review: Option<ReviewedExport>,
    pub error: Option<String>,
    pub presets: Option<Vec<retold_app::presets::PresetRecord>>,
}

impl DeliveryPanel {
    fn request(&self, cx: &Context<Shell>) -> Result<desktop::ExportRequest, String> {
        let max_requests = self
            .request_limit
            .read(cx)
            .value()
            .parse::<u64>()
            .map_err(|_| "Export requests must be a nonnegative whole number.".to_owned())?;
        Ok(desktop::ExportRequest {
            delivery: self.options.clone(),
            spend: self.allowance.read(cx).value().to_string(),
            max_requests,
        })
    }
}

enum Task {
    Review {
        library: PathBuf,
        reference: String,
        options: desktop::ExportRequest,
    },
    Submit(Box<ReviewedExport>),
}
enum Reply {
    Reviewed(Box<ReviewedExport>),
    Submitted(retold_app::queue::JobRecord),
}

impl Task {
    fn execute(self) -> Result<Reply, String> {
        let result = match self {
            Self::Review {
                library,
                reference,
                options,
            } => desktop::review_export(&library, &reference, &options)
                .map(|review| Reply::Reviewed(Box::new(review))),
            Self::Submit(review) => review.submit().map(Reply::Submitted),
        };
        result.map_err(|error| error.message)
    }
}

impl Shell {
    pub(super) fn begin_delivery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.editor.as_ref().is_some_and(|editor| editor.dirty(cx)) {
            self.status = "Save or discard your script changes before reviewing an export.".into();
            self.show_script = true;
            cx.notify();
            return;
        }
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        let Some(video) = self.find_video(reference) else {
            return;
        };
        let editions = match desktop::delivery_editions(&video.editions) {
            Ok(editions) => editions,
            Err(error) => {
                self.status = error.message;
                cx.notify();
                return;
            }
        };
        let selected = editions
            .first()
            .map(|edition| edition.id.clone())
            .into_iter()
            .collect();
        let (allowance, amount_subscription) = delivery_input(window, cx);
        let (request_limit, requests_subscription) = delivery_input(window, cx);
        self.delivery = Some(DeliveryPanel {
            library: snapshot.context.library.clone(),
            reference: reference.clone(),
            title: video.title.clone(),
            editions,
            allowance,
            request_limit,
            _subscriptions: vec![amount_subscription, requests_subscription],
            options: ExportOptions {
                editions: selected,
                formats: vec!["16x9".into()],
                project_only: true,
                ..ExportOptions::default()
            },
            review: None,
            error: None,
            presets: None,
        });
        self.review = None;
        self.load_delivery_presets(cx);
        cx.notify();
    }

    pub(super) fn review_delivery(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = &self.delivery else {
            return;
        };
        let options = match panel.request(cx) {
            Ok(request) => request,
            Err(error) => {
                if let Some(panel) = &mut self.delivery {
                    panel.error = Some(error);
                    panel.review = None;
                }
                cx.notify();
                return;
            }
        };
        self.delivery_task(
            Task::Review {
                library: panel.library.clone(),
                reference: panel.reference.clone(),
                options,
            },
            cx,
        );
    }

    pub(super) fn submit_delivery(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self
            .delivery
            .as_ref()
            .and_then(|panel| panel.review.clone())
        else {
            return;
        };
        let current = self.delivery.as_ref().is_some_and(|panel| {
            panel.reference == review.reference()
                && panel.library == review.library()
                && panel
                    .request(cx)
                    .is_ok_and(|request| review.matches_request(&request))
        });
        if !current {
            if let Some(panel) = &mut self.delivery {
                panel.review = None;
                panel.error = Some(
                    "Export choices or limits changed. Review the export again before confirming."
                        .into(),
                );
            }
            cx.notify();
            return;
        }
        self.delivery_task(Task::Submit(Box::new(review)), cx);
    }

    fn delivery_task(&mut self, task: Task, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        if let Some(panel) = &mut self.delivery {
            panel.error = None;
            panel.review = None;
        }
        let work = cx
            .background_executor()
            .spawn(async move { task.execute() });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(Reply::Reviewed(review)) => {
                        if let Some(panel) = &mut this.delivery {
                            let same = panel.reference == review.reference() && panel.library == review.library();
                            if same { panel.review = Some(*review); }
                        }
                    }
                    Ok(Reply::Submitted(job)) => {
                        this.delivery = None;
                        this.destination = Destination::Activity;
                        this.status = format!("Export submission acknowledged for job {} ({} at acceptance). Activity shows its current saved outcome.", job.id, job.status.as_str());
                        this.refresh(cx);
                    }
                    Err(error) => {
                        if let Some(panel) = &mut this.delivery { panel.error = Some(error); }
                        else { this.status = error; }
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}

fn delivery_input(
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> (Entity<InputState>, Subscription) {
    let input = cx.new(|cx| {
        let mut input = InputState::new(window, cx);
        input.set_value("0", window, cx);
        input
    });
    let subscription = cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change) {
            if let Some(panel) = &mut this.delivery {
                panel.review = None;
            }
            cx.notify();
        }
    });
    (input, subscription)
}
