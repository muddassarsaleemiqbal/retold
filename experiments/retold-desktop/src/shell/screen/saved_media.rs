//! Media context and playback stay adjacent; file presence does not imply delivery readiness.
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*};

use super::{error_notice, heading_small, muted, notice};
use crate::shell::{Shell, saved_media::FileAction};

impl Shell {
    pub(super) fn media_page(&self, cx: &mut Context<Self>) -> Div {
        let Some(panel) = &self.saved_media else {
            return div();
        };
        let p = self.palette();
        let mut body =
            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    Button::new("desktop-media-close")
                        .self_start()
                        .ghost()
                        .icon(gpui_kit::assets::IconName::ChevronLeft)
                        .ml(gpui_kit::px(-10.))
                        .label("Back to video")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_media(cx);
                            cx.notify();
                        })),
                )
                .child(super::page_header(
                    "Saved media",
                    "Everything this video has produced so far.",
                    [super::refresh_button(
                        "desktop-media-refresh",
                        "Refresh saved media",
                        self.busy,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        let edition = this
                            .saved_media
                            .as_ref()
                            .and_then(|panel| panel.edition.clone());
                        this.load_media(edition, 0, cx);
                    }))
                    .into_any_element()],
                    p,
                ))
                .when_some(panel.error.as_ref(), |body, error| {
                    body.child(error_notice(error, p))
                });
        let Some(page) = &panel.page else {
            return body.child(muted(
                if self.busy {
                    "Loading saved media…"
                } else {
                    "Refresh to load saved media."
                },
                p,
            ));
        };
        body = body
            .child(heading_small(&page.title))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("desktop-media-all")
                            .self_start()
                            .label("All editions")
                            .selected(panel.edition.is_none())
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.load_media(None, 0, cx))),
                    )
                    .children(page.editions.iter().enumerate().map(|(index, edition)| {
                        let edition = edition.clone();
                        Button::new(("desktop-media-edition", index))
                            .self_start()
                            .label(retold_app::desktop::edition_label(&edition))
                            .selected(panel.edition.as_deref() == Some(&edition))
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.load_media(Some(edition.clone()), 0, cx)
                            }))
                    })),
            )
            .child(panel.player.clone());
        if let Some(item) = panel.selected.and_then(|index| page.items.get(index)) {
            body = body
                .child(muted(&item.relative, p))
                .child(muted(
                    &format!(
                        "{} · {} bytes · {}",
                        item.kind.label(),
                        item.bytes,
                        item.edition.as_deref().unwrap_or("Shared across editions")
                    ),
                    p,
                ))
                .child(
                    Button::new("desktop-media-reveal")
                        .self_start()
                        .label("Show saved file")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_media_file(FileAction::Reveal, cx)
                        })),
                );
            if item.kind == retold_app::desktop::media::Kind::Project {
                body = body.child(
                    Button::new("desktop-media-editor")
                        .self_start()
                        .label("Open editor project")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_media_file(FileAction::Editor, cx)
                        })),
                );
            }
        }
        if page.partial {
            body = body.child(notice("This listing is partial: scan limits, symlinks or unsupported paths were encountered. Some files may be missing from this list.", p));
        }
        if page.items.is_empty() {
            body = body.child(notice("No supported media saved for this selection yet. Review a run from the video to prepare content.", p));
        }
        body.children(page.items.iter().enumerate().map(|(index, item)| {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .bg(p.surface)
                .rounded_lg()
                .child(muted(
                    &format!("{} · {} bytes", item.kind.label(), item.bytes),
                    p,
                ))
                .child(
                    Button::new(("desktop-media-item", index))
                        .self_start()
                        .label(item.relative.clone())
                        .selected(panel.selected == Some(index))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| this.select_media(index, cx))),
                )
        }))
        .when_some(
            panel.selected.and_then(|index| page.items.get(index)),
            |body, item| body.child(self.copy_controls(item, cx)),
        )
        .child(self.delivery_history(cx))
        .child(muted(
            &format!(
                "{} saved files · page {}",
                page.total,
                page.offset / retold_app::desktop::media::PAGE_SIZE + 1
            ),
            p,
        ))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("desktop-media-previous")
                        .self_start()
                        .label("Previous page")
                        .disabled(self.busy || page.offset == 0)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(panel) = &this.saved_media
                                && let Some(page) = &panel.page
                            {
                                this.load_media(
                                    panel.edition.clone(),
                                    page.offset
                                        .saturating_sub(retold_app::desktop::media::PAGE_SIZE),
                                    cx,
                                );
                            }
                        })),
                )
                .child(
                    Button::new("desktop-media-next")
                        .self_start()
                        .label("Next page")
                        .disabled(self.busy || page.next_offset.is_none())
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(panel) = &this.saved_media
                                && let Some(offset) =
                                    panel.page.as_ref().and_then(|page| page.next_offset)
                            {
                                this.load_media(panel.edition.clone(), offset, cx);
                            }
                        })),
                ),
        )
    }
}
