//! Saved views below the video list.
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
    input::Input,
};
use gpui_kit::{Context, Div, div, prelude::*};

use super::{error_notice, heading_small, muted};
use crate::shell::Shell;

impl Shell {
    pub(super) fn saved_views(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.views;
        let Some(saved) = &panel.saved else {
            return div().flex().child(
                Button::new("desktop-views-show")
                    .ml(gpui_kit::px(-10.))
                    .self_start()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::Bookmark)
                    .label("Saved views…")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.views_task(None, cx))),
            );
        };
        let buttons = saved
            .iter()
            .enumerate()
            .map(|(index, view)| {
                let (chosen, ask, name) = (view.clone(), view.name.clone(), view.name.clone());
                let confirming = panel.removing.as_deref() == Some(view.name.as_str());
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new(("desktop-view-apply", index))
                            .self_start()
                            .label(view.name.clone())
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.apply_view(&chosen, cx)),
                            ),
                    )
                    .when(!confirming, |row| {
                        row.child(
                            Button::new(("desktop-view-remove", index))
                                .self_start()
                                .ghost()
                                .label("Remove")
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.views.removing = Some(ask.clone());
                                    cx.notify();
                                })),
                        )
                    })
                    .when(confirming, |row| {
                        row.child(
                            Button::new(("desktop-view-remove-confirm", index))
                                .self_start()
                                .primary()
                                .label("Confirm remove")
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remove_view(name.clone(), cx)
                                })),
                        )
                    })
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Saved views"))
            .when(saved.is_empty(), |body| {
                body.child(muted("No saved views yet.", p))
            })
            .child(div().flex().flex_wrap().gap_2().children(buttons))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(Input::new(&panel.name).aria_label("View name"))
                    .child(
                        Button::new("desktop-view-save")
                            .self_start()
                            .label("Save current filter")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.save_current_view(cx))),
                    ),
            )
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            })
    }
}
