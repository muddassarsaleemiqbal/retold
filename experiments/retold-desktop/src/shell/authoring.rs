//! Guide and channel creation and single-field edits, previewed before writing.
use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, Entity, Window};
use retold_app::desktop::authoring::{self, Document, Preview, Resource};

use super::{Shell, account_admin::summary};

pub(super) struct AuthoringPanel {
    pub resource: Resource,
    pub id: Entity<InputState>,
    pub language: Entity<InputState>,
    pub field: Entity<InputState>,
    pub value: Entity<InputState>,
    pub document: Option<Document>,
    pub preview: Option<String>,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl AuthoringPanel {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Shell>) -> Self {
        Self {
            resource: Resource::Guide,
            id: cx.new(|cx| InputState::new(window, cx).placeholder("ID, such as night-ferry")),
            language: cx.new(|cx| InputState::new(window, cx).placeholder("Language, such as en")),
            field: cx.new(|cx| InputState::new(window, cx).placeholder("Field, such as /name")),
            value: cx.new(|cx| InputState::new(window, cx).placeholder("New text value")),
            document: None,
            preview: None,
            message: None,
            error: None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum AuthoringTask {
    PreviewCreate,
    Create,
    Open,
    PreviewEdit,
    Edit,
}

enum Reply {
    Preview(String),
    Opened(Box<Document>),
    Done(String, Option<Box<Document>>),
}

impl Shell {
    pub(super) fn authoring_task(&mut self, task: AuthoringTask, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        if self.busy {
            return;
        }
        let config = snapshot.context.config.clone();
        let panel = &mut self.authoring;
        panel.error = None;
        let read = |entity: &Entity<InputState>| entity.read(cx).value().trim().to_owned();
        let (id, language, field, value) = (
            read(&panel.id),
            read(&panel.language),
            read(&panel.field),
            read(&panel.value),
        );
        let (resource, document) = (panel.resource, panel.document.clone());
        self.busy = true;
        let work = cx.background_executor().spawn(async move {
            let missing = || {
                retold_app::protocol::Diagnostic::new(
                    retold_app::protocol::ErrorCode::InputRequired,
                    "Open a document first.",
                )
            };
            match task {
                AuthoringTask::PreviewCreate => {
                    authoring::create(&config, (resource, &id, &language), Preview::Only)
                        .map(|v| Reply::Preview(summary(&v)))
                }
                AuthoringTask::Create => {
                    authoring::create(&config, (resource, &id, &language), Preview::Write)
                        .map(|v| Reply::Done(summary(&v), None))
                }
                AuthoringTask::Open => {
                    authoring::open(&config, resource, &id).map(|d| Reply::Opened(Box::new(d)))
                }
                AuthoringTask::PreviewEdit => authoring::edit(
                    &config,
                    &document.ok_or_else(missing)?,
                    (&field, &value, Preview::Only),
                )
                .map(|v| Reply::Preview(summary(&v))),
                AuthoringTask::Edit => {
                    let document = document.ok_or_else(missing)?;
                    let result =
                        authoring::edit(&config, &document, (&field, &value, Preview::Write))?;
                    let reopened = authoring::open(&config, document.resource, &document.id)?;
                    Ok(Reply::Done(summary(&result), Some(Box::new(reopened))))
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let panel = &mut this.authoring;
                match result {
                    Ok(Reply::Preview(text)) => panel.preview = Some(text),
                    Ok(Reply::Opened(document)) => {
                        panel.document = Some(*document);
                        panel.preview = None;
                    }
                    Ok(Reply::Done(text, reopened)) => {
                        panel.preview = None;
                        panel.message = Some(text);
                        if let Some(document) = reopened {
                            panel.document = Some(*document);
                        }
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
