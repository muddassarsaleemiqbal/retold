//! Saved library views: save the current catalog filter by name and reapply it.
use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, Entity, Window};
use retold_app::desktop::views;
use retold_app::views::ViewRecord;

use super::Shell;

pub(super) struct ViewsPanel {
    pub name: Entity<InputState>,
    pub saved: Option<Vec<ViewRecord>>,
    pub removing: Option<String>,
    pub tag: Entity<InputState>,
    pub error: Option<String>,
}

impl ViewsPanel {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Shell>) -> Self {
        Self {
            name: cx.new(|cx| InputState::new(window, cx).placeholder("View name")),
            saved: None,
            removing: None,
            tag: cx.new(|cx| InputState::new(window, cx).placeholder("Tag")),
            error: None,
        }
    }
}

impl Shell {
    /// Load saved views, or save the current filter when `save` is set.
    pub(super) fn views_task(&mut self, save: Option<String>, cx: &mut Context<Self>) {
        self.views_change(save.map(ViewChange::Save), cx);
    }

    pub(super) fn remove_view(&mut self, name: String, cx: &mut Context<Self>) {
        self.views_change(Some(ViewChange::Remove(name)), cx);
    }

    fn views_change(&mut self, change: Option<ViewChange>, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        if self.busy {
            return;
        }
        let library = snapshot.context.library.clone();
        let options = self.catalog_options.clone();
        self.views.error = None;
        self.views.removing = None;
        let work = cx.background_executor().spawn(async move {
            match change {
                Some(ViewChange::Save(name)) => views::save(&library, &name, &options),
                Some(ViewChange::Remove(name)) => views::remove(&library, &name),
                None => views::list(&library),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(saved) => this.views.saved = Some(saved),
                    Err(error) => this.views.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn save_current_view(&mut self, cx: &mut Context<Self>) {
        let name = self.views.name.read(cx).value().trim().to_owned();
        self.views_task(Some(name), cx);
    }

    pub(super) fn apply_view(&mut self, view: &ViewRecord, cx: &mut Context<Self>) {
        self.load_video_page(views::options(view), Vec::new(), cx);
    }
}

enum ViewChange {
    Save(String),
    Remove(String),
}

impl Shell {
    pub(super) fn tag_video(&mut self, add: bool, cx: &mut Context<Self>) {
        let tag = self.views.tag.read(cx).value().trim().to_owned();
        if tag.is_empty() {
            return;
        }
        let change = if add {
            retold_app::desktop::lifecycle::Change::AddTag(tag)
        } else {
            retold_app::desktop::lifecycle::Change::RemoveTag(tag)
        };
        self.change_video(change, cx);
    }
}
