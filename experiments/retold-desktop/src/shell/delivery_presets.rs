//! Export presets for a delivery review: loaded in the background, chosen with one aspect.
use gpui_kit::Context;
use retold_app::desktop::admin;

use super::Shell;

impl Shell {
    pub(super) fn load_delivery_presets(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = &self.delivery else {
            return;
        };
        let (library, reference) = (panel.library.clone(), panel.reference.clone());
        let work = cx
            .background_executor()
            .spawn(async move { admin::presets(&library) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                let Some(panel) = this
                    .delivery
                    .as_mut()
                    .filter(|panel| panel.reference == reference)
                else {
                    return;
                };
                panel.presets = Some(result.unwrap_or_default());
                cx.notify();
            });
        })
        .detach();
    }

    /// Choose a preset (or none) and its first supported aspect; any review is discarded.
    pub(super) fn choose_delivery_preset(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.delivery else {
            return;
        };
        let aspect = name.as_ref().and_then(|name| {
            panel
                .presets
                .iter()
                .flatten()
                .find(|preset| &preset.name == name)
                .and_then(|preset| preset.aspects.first().cloned())
        });
        panel.options.preset = name;
        panel.options.aspect = aspect;
        panel.review = None;
        cx.notify();
    }

    pub(super) fn choose_delivery_aspect(&mut self, aspect: String, cx: &mut Context<Self>) {
        if let Some(panel) = &mut self.delivery {
            panel.options.aspect = Some(aspect);
            panel.review = None;
        }
        cx.notify();
    }
}
