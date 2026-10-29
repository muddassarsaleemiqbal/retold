//! Source preview belongs to offline creation and does not grant run authority.
use super::{heading_small, muted};
use crate::shell::Shell;
use gpui_kit::component::{Disableable as _, button::Button, input::Input};
use gpui_kit::{Context, Div, div, prelude::*, px};

impl Shell {
    pub(super) fn draft_source(&self, cx: &mut Context<Self>) -> Div {
        let Some(form) = &self.draft else {
            return div();
        };
        let p = self.palette();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Source text (optional)"))
            .child(muted(
                "Choose a text file with the story, then preview it.",
                p,
            ))
            .child(
                Input::new(&form.source)
                    .aria_label("Story source file")
                    .disabled(self.busy || self.prompting),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("desktop-source-choose")
                            .self_start()
                            .label("Choose source file…")
                            .disabled(self.busy || self.prompting)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.choose_source(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("desktop-source-preview")
                            .self_start()
                            .label("Preview source")
                            .disabled(self.busy || form.source_path(cx).is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.preview_source(cx))),
                    ),
            );
        if let Some(preview) = &form.preview {
            body = body
                .child(muted(
                    &format!(
                        "{} bytes{}",
                        preview.bytes,
                        if preview.truncated {
                            " · preview shortened; the full source will be saved"
                        } else {
                            ""
                        }
                    ),
                    p,
                ))
                .child(
                    div()
                        .id("desktop-source-text")
                        .max_h(px(200.))
                        .overflow_y_scroll()
                        .p_3()
                        .bg(p.surface)
                        .rounded_lg()
                        .child(preview.text.clone()),
                );
        }
        body
    }
}
