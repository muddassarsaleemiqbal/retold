//! Pure GPUI layout. All media/process work lives behind background requests.

use gpui_kit::component::{
    Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::Textarea,
};
use gpui_kit::{
    Context, Div, FontWeight, ImageSource, IntoElement, ObjectFit, Render, Role, Styled, Window,
    div, img, prelude::*, px,
};

use super::{Studio, components::label};
use crate::theme::Palette;

impl Render for Studio {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::new(self.light);
        let narrow =
            window.viewport_size().width < px(1100.) || window.viewport_size().height < px(840.);
        let preview_height = if window.viewport_size().height < px(740.) {
            170.
        } else if narrow {
            240.
        } else {
            270.
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.canvas)
            .text_color(p.text)
            .text_size(px(14.))
            .child(self.header(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(!narrow, |body| body.child(self.sidebar(p)))
                    .child(
                        div()
                            .id("workspace")
                            .flex_1()
                            .min_w_0()
                            .overflow_y_scroll()
                            .p(if narrow { px(16.) } else { px(24.) })
                            .child(self.workspace(narrow, preview_height, cx)),
                    ),
            )
            .child(self.status())
    }
}

impl Studio {
    fn sidebar(&self, p: Palette) -> Div {
        div()
            .w(px(174.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_5()
            .p_5()
            .border_r_1()
            .border_color(p.border)
            .child(label("Coastal Stories", p.secondary))
            .child(div().rounded_md().p_3().bg(p.surface).child("Scenes"))
            .child(label("The Lighthouse Files", p.secondary))
            .child(div().text_size(px(13.)).child("A light at sea"))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.secondary)
                    .child("Native feasibility preview. Production actions are not connected."),
            )
    }

    fn workspace(&self, narrow: bool, preview_height: f32, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.light);
        div().flex().flex_col().gap(if narrow { px(12.) } else { px(16.) })
            .child(div().flex().flex_col().gap_2()
                .child(label("The Lighthouse Files / Part 1", p.secondary))
                .child(div().text_size(px(26.)).font_weight(FontWeight::SEMIBOLD).child("A light at sea"))
                .child(div().flex().items_center().justify_between().gap_3()
                    .child(self.editions(cx))
                    .when(narrow, |row| row.child(Button::new("review-panel").label(if self.show_review { "Hide review" } else { "Review scene" })
                        .on_click(cx.listener(|this, _, _, cx| { this.show_review = !this.show_review; cx.notify(); }))))))
            .child(div().flex().gap_5().when(narrow, |row| row.flex_col())
                .child(div().flex_1().min_w_0().flex().flex_col().gap(if narrow { px(8.) } else { px(12.) })
                    .child(self.media_canvas(preview_height, p))
                    .child(self.media_controls(cx))
                    .child(div().flex().justify_between().child(label("Narration", p.secondary))
                        .child(label(if self.changed(cx) { "Unsaved session edits" } else { "Kept in session" }, p.secondary)))
                    .child(Textarea::new(&self.narration).aria_label("Narration").h(if narrow { px(80.) } else { px(104.) }).text_size(px(17.)))
                    .child(label("Source: fictional story outline. Hindi and Urdu are input-testing samples.", p.secondary)))
                .when(!narrow || self.show_review, |row| row.child(div().when(!narrow, |panel| panel.w(px(238.)).flex_shrink_0()).child(self.inspector(cx)))))
            .child(self.sequence(cx))
    }

    fn media_canvas(&self, height: f32, p: Palette) -> Div {
        let source = self.frame.as_ref().map_or_else(
            || ImageSource::from("coastline.svg"),
            |frame| ImageSource::from(std::sync::Arc::clone(frame)),
        );
        let caption = self.media.as_ref().map_or_else(
            || self.session.scene().title.to_owned(),
            |media| {
                media.path.file_name().map_or_else(
                    || "Existing media".into(),
                    |name| name.to_string_lossy().into_owned(),
                )
            },
        );
        div()
            .relative()
            .h(px(height))
            .w_full()
            .bg(p.surface)
            .rounded_lg()
            .overflow_hidden()
            .child(
                img(source)
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            )
            .child(
                div()
                    .absolute()
                    .bottom_3()
                    .left_4()
                    .rounded_md()
                    .px_3()
                    .py_2()
                    .bg(p.canvas)
                    .id("media-caption")
                    .role(Role::Status)
                    .aria_label(caption.clone())
                    .child(caption),
            )
    }

    fn media_controls(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.light);
        let available = self.media.as_ref().is_some_and(|m| m.duration > 0.0) && !self.loading;
        let duration = self.media.as_ref().map_or(0.0, |m| m.duration);
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                Button::new("play")
                    .small()
                    .primary()
                    .label(if self.playback.is_some() {
                        "Pause"
                    } else {
                        "Play"
                    })
                    .disabled(!available)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_playback(cx))),
            )
            .child(
                Button::new("seek-back")
                    .small()
                    .label("−5 s")
                    .accessibility_label("Seek backward 5 seconds")
                    .disabled(!available || self.playback.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.seek(-5.0, cx))),
            )
            .child(
                Button::new("seek-forward")
                    .small()
                    .label("+5 s")
                    .accessibility_label("Seek forward 5 seconds")
                    .disabled(!available || self.playback.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.seek(5.0, cx))),
            )
            .child(label(
                &format!("{} / {}", timecode(self.position), timecode(duration)),
                p.secondary,
            ))
            .child(div().flex_1())
            .child(label(
                if self.loading {
                    "Opening…"
                } else if self.media.is_some() {
                    "Existing file preview"
                } else {
                    "Illustrated fixture · Open media to play"
                },
                p.secondary,
            ))
    }
}

fn timecode(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
