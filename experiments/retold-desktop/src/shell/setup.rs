//! Native folder choices are reviewed before initialization or non-overwriting resource imports.
use std::path::PathBuf;

use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext, Context, Entity, PathPromptOptions};

use super::{
    Destination, Shell,
    backend::{Request, ResourceKind},
};

#[derive(Debug)]
pub(super) struct ImportForm {
    pub source: PathBuf,
    pub kind: ResourceKind,
    pub id: Entity<InputState>,
}

impl Shell {
    pub(super) fn choose_new_workspace(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.has_unsaved_setup(cx) {
            return;
        }
        let chooser = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a folder for the new workspace".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = chooser.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(Some(paths))) => {
                        this.new_root = paths.into_iter().next();
                        this.destination = Destination::Home;
                    }
                    Ok(Ok(None)) => (),
                    _ => this.status = "The folder chooser could not be opened. Try again.".into(),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn initialize_workspace(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.new_root.clone() else {
            return;
        };
        let typed = self.workspace_name.read(cx).value().trim().to_owned();
        let name = if typed.is_empty() {
            folder_name(&root)
        } else {
            typed
        };
        self.request(Request::Initialize { root, name }, cx);
    }

    pub(super) fn choose_import(&mut self, kind: ResourceKind, cx: &mut Context<Self>) {
        if self.busy || self.has_unsaved_setup(cx) {
            return;
        }
        let chooser = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                match kind {
                    ResourceKind::Guide => "Choose a guide folder containing guide.toml",
                    ResourceKind::Channel => "Choose a channel folder containing channel.toml",
                }
                .into(),
            ),
        });
        cx.spawn(async move |this, cx| {
            let result = chooser.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(source) = paths.into_iter().next() {
                            this.status = "Folder selected. Give the imported resource an ID, then review and import.".into();
                            this.pending_import(source, kind, cx);
                        }
                    }
                    Ok(Ok(None)) => (),
                    _ => this.status = "The resource chooser could not be opened. Try again.".into(),
                }
                cx.notify();
            });
        }).detach();
    }

    fn pending_import(&mut self, source: PathBuf, kind: ResourceKind, cx: &mut Context<Self>) {
        let Some(handle) = cx.active_window() else {
            return;
        };
        let Ok(id) = handle.update(cx, |_, window, cx| {
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. my-story-guide"))
        }) else {
            return;
        };
        self.import = Some(ImportForm { source, kind, id });
        self.destination = Destination::Settings;
        cx.notify();
    }

    pub(super) fn import_resource(&mut self, cx: &mut Context<Self>) {
        let (Some(snapshot), Some(form)) = (&self.snapshot, &self.import) else {
            return;
        };
        let request = Request::Import {
            root: snapshot.context.root.clone(),
            config: snapshot.context.config.clone(),
            kind: form.kind,
            id: form.id.read(cx).value().to_string(),
            source: form.source.clone(),
        };
        self.request(request, cx);
    }
}

/// A readable default workspace name from its folder, such as `Noir Recaps` for `noir-recaps`.
pub(super) fn folder_name(root: &std::path::Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .map(|name| super::screen::title_case(name.trim_start_matches('.')))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "My stories".to_owned())
}
