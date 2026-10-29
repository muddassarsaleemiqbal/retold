//! Series registry, part matrix, part linking and reviewed subset runs.
use std::collections::BTreeSet;

use gpui_kit::Context;
use retold_app::desktop::series::{self, Matrix, SeriesPlan};
use retold_app::series::SeriesSummary;

use super::Shell;

#[derive(Default)]
pub(super) struct SeriesPanel {
    pub creating: Option<NewForm>,
    pub list: Option<Vec<SeriesSummary>>,
    pub matrix: Option<Matrix>,
    pub selected: BTreeSet<String>,
    pub linking: Option<String>,
    pub plan: Option<SeriesPlan>,
    pub addition: Option<series::PartAddition>,
    pub error: Option<String>,
    pub message: Option<String>,
    generation: u64,
}

#[derive(Clone)]
pub(super) enum SeriesTask {
    List,
    Open(String),
    Link(String, String),
    Plan,
    Run,
    Create(series::NewSeries),
    ReviewAdd(usize),
    Add,
}

enum Reply {
    List(Vec<SeriesSummary>),
    Matrix(Box<Matrix>),
    Plan(Box<SeriesPlan>),
    Submitted(String),
    Addition(series::PartAddition),
}

impl SeriesPanel {
    /// The parts selector the planner accepts, in series order; none selects every part.
    pub(super) fn selector(&self) -> Option<String> {
        let matrix = self.matrix.as_ref()?;
        let chosen: Vec<_> = matrix
            .manifest
            .parts
            .iter()
            .filter(|part| self.selected.contains(&part.id))
            .map(|part| part.id.clone())
            .collect();
        (!chosen.is_empty()).then(|| chosen.join(","))
    }

    pub(super) fn toggle(&mut self, part: &str) {
        if !self.selected.remove(part) {
            self.selected.insert(part.into());
        }
        self.plan = None;
    }
}

impl Shell {
    pub(super) fn series_task(&mut self, task: SeriesTask, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        if self.busy {
            return;
        }
        let root = snapshot.context.library.clone();
        let panel = &mut self.series;
        panel.generation = panel.generation.wrapping_add(1);
        panel.error = None;
        let generation = panel.generation;
        let id = panel
            .matrix
            .as_ref()
            .map(|matrix| matrix.manifest.id.clone());
        let selector = panel.selector();
        let reviewed = panel.plan.clone();
        let addition = panel.addition.clone();
        if matches!(task, SeriesTask::Open(_)) {
            panel.selected.clear();
            panel.plan = None;
            panel.linking = None;
        }
        self.busy = matches!(
            task,
            SeriesTask::Run | SeriesTask::Link(..) | SeriesTask::Create(_)
        );
        let creating = matches!(task, SeriesTask::Create(_));
        let work = cx.background_executor().spawn(async move {
            let missing = || {
                retold_app::protocol::Diagnostic::new(
                    retold_app::protocol::ErrorCode::InputRequired,
                    "Open a series first.",
                )
            };
            match task {
                SeriesTask::List => series::list(&root).map(Reply::List),
                SeriesTask::Open(id) => {
                    series::matrix(&root, &id).map(|m| Reply::Matrix(Box::new(m)))
                }
                SeriesTask::Link(part, video) => {
                    let id = id.ok_or_else(missing)?;
                    series::link(&root, &id, (&part, &video)).map(|m| Reply::Matrix(Box::new(m)))
                }
                SeriesTask::Plan => {
                    let id = id.ok_or_else(missing)?;
                    series::plan(&root, &id, (selector.as_deref(), "0"))
                        .map(|plan| Reply::Plan(Box::new(plan)))
                }
                SeriesTask::ReviewAdd(count) => {
                    series::review_add_parts(&root, &id.ok_or_else(missing)?, count)
                        .map(Reply::Addition)
                }
                SeriesTask::Add => series::add_parts(
                    &root,
                    &id.ok_or_else(missing)?,
                    &addition.ok_or_else(missing)?,
                )
                .map(|m| Reply::Matrix(Box::new(m))),
                SeriesTask::Create(new) => {
                    series::create(&root, &new).map(|m| Reply::Matrix(Box::new(m)))
                }
                SeriesTask::Run => {
                    let (id, plan) = (id.ok_or_else(missing)?, reviewed.ok_or_else(missing)?);
                    series::run(&root, &id, &plan).map(Reply::Submitted)
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let panel = &mut this.series;
                if panel.generation != generation {
                    return;
                }
                match result {
                    Ok(Reply::List(list)) => panel.list = Some(list),
                    Ok(Reply::Matrix(matrix)) => {
                        if creating {
                            panel.creating = None;
                            panel.list = None;
                        }
                        panel.matrix = Some(*matrix);
                        panel.addition = None;
                        panel.linking = None;
                        panel.plan = None;
                    }
                    Ok(Reply::Plan(plan)) => panel.plan = Some(*plan),
                    Ok(Reply::Addition(addition)) => panel.addition = Some(addition),
                    Ok(Reply::Submitted(text)) => {
                        panel.plan = None;
                        panel.message = Some(text);
                    }
                    Err(error) => panel.error = Some(error.message),
                }
                if this.series.list.is_none() {
                    this.series_task(SeriesTask::List, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

/// The native creation form: work title, guide and part count.
pub(super) struct NewForm {
    pub work: gpui_kit::Entity<gpui_kit::component::input::InputState>,
    pub guide: Option<String>,
    pub parts: usize,
}

impl Shell {
    pub(super) fn begin_new_series(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) {
        use gpui_kit::AppContext as _;
        let guide = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.choices.as_ref().ok())
            .and_then(|choices| choices.guides.first().map(|guide| guide.id.clone()));
        let work = cx.new(|cx| {
            gpui_kit::component::input::InputState::new(window, cx).placeholder("Work title")
        });
        self.series.creating = Some(NewForm {
            work,
            guide,
            parts: 3,
        });
        cx.notify();
    }

    pub(super) fn submit_new_series(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &self.series.creating else {
            return;
        };
        let Some(guide) = form.guide.clone() else {
            self.series.error = Some("Choose a guide first.".into());
            cx.notify();
            return;
        };
        let channel = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.choices.as_ref().ok())
            .and_then(|choices| choices.channels.first().cloned())
            .unwrap_or_else(|| "default".into());
        let new = series::NewSeries {
            work: form.work.read(cx).value().to_string(),
            guide,
            channel,
            parts: form.parts,
        };
        self.series_task(SeriesTask::Create(new), cx);
    }
}
