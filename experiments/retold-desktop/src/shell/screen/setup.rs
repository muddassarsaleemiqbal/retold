use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
    input::Input,
};
use gpui_kit::{Context, Div, Styled, div, prelude::*, px};

use super::{card, empty, muted};
use crate::shell::{Shell, backend::ResourceKind};

impl Shell {
    pub(super) fn new_workspace_form(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(root) = &self.new_root else {
            return div();
        };
        let suggested = crate::shell::setup::folder_name(root);
        empty("Create a workspace", "A workspace keeps a production's videos, guides, settings and queue together in one folder.", p)
            .child(div().flex().items_center().gap_3().p_3().rounded_lg().border_1().border_color(p.border).bg(p.surface)
                .child(gpui_kit::component::Icon::new(gpui_kit::assets::IconName::FolderOpen).text_color(p.secondary))
                .child(div().flex_1().min_w_0().truncate().child(super::middle_truncated(&super::home_relative(root), 60)))
                .child(Button::new("desktop-change-workspace-folder").ghost().label("Change…").disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.choose_new_workspace(cx)))))
            .child(div().flex().flex_col().gap_1()
                .child(div().text_sm().font_weight(gpui_kit::FontWeight::MEDIUM).child("Name"))
                .child(div().w(gpui_kit::px(360.)).child(Input::new(&self.workspace_name).aria_label("Workspace name").disabled(self.busy)))
                .child(muted(&format!("Leave empty to use \"{suggested}\"."), p)))
            .child(muted("Nothing already in the folder is replaced.", p))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("desktop-cancel-workspace").ghost().label("Cancel").disabled(self.busy).on_click(cx.listener(|this, _, _, cx| { this.new_root = None; cx.notify(); })))
                .child(Button::new("desktop-create-workspace").primary().icon(gpui_kit::assets::IconName::Plus).label("Create workspace").disabled(self.busy).on_click(cx.listener(|this, _, _, cx| this.initialize_workspace(cx)))))
    }

    pub(super) fn import_buttons(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(
                Button::new("desktop-import-guide")
                    .self_start()
                    .label("Import guide…")
                    .disabled(self.busy || self.snapshot.is_none())
                    .on_click(
                        cx.listener(|this, _, _, cx| this.choose_import(ResourceKind::Guide, cx)),
                    ),
            )
            .child(
                Button::new("desktop-import-channel")
                    .self_start()
                    .label("Import channel…")
                    .disabled(self.busy || self.snapshot.is_none())
                    .on_click(
                        cx.listener(|this, _, _, cx| this.choose_import(ResourceKind::Channel, cx)),
                    ),
            )
    }

    pub(super) fn resource_import_form(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(form) = &self.import else {
            return div();
        };
        let kind = match form.kind {
            ResourceKind::Guide => "guide",
            ResourceKind::Channel => "channel",
        };
        empty(&format!("Import {kind}"), "The source is validated before copying. An existing resource with the same ID is preserved.", p)
            .child(card("Source folder", &form.source.display().to_string(), p))
            .child(muted("Resource ID · lowercase letters, digits and single dashes", p))
            .child(Input::new(&form.id).aria_label("Imported resource ID").disabled(self.busy).max_w(px(420.)))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("desktop-confirm-import").self_start().primary().label(format!("Validate and import {kind}")).disabled(self.busy).on_click(cx.listener(|this, _, _, cx| this.import_resource(cx))))
                .child(Button::new("desktop-cancel-import").self_start().label("Cancel").disabled(self.busy).on_click(cx.listener(|this, _, _, cx| { this.import = None; cx.notify(); }))))
    }
}
