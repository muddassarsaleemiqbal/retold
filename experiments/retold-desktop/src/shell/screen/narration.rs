//! Narration waveform, playback and aligned lines for one recorded edition.
use gpui_kit::component::{
    Selectable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*, px};

use super::{error_notice, heading, heading_small, muted};
use crate::shell::Shell;

/// Height of the tallest waveform column.
const WAVEFORM_HEIGHT: f32 = 72.;

impl Shell {
    pub(super) fn narration_page(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(panel) = &self.narration else {
            return div();
        };
        let body = div().flex().flex_col().gap_4()
            .child(Button::new("desktop-narration-back").self_start().ghost().icon(gpui_kit::assets::IconName::ChevronLeft).ml(gpui_kit::px(-10.)).label("Back to video")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.narration = None;
                    cx.notify();
                })))
            .child(heading("Narration"))
            .child(muted("Listen to each edition's recorded narration. Edit what it says in Language versions; a reviewed run records it again.", p))
            .when_some(panel.error.as_ref(), |body, error| body.child(error_notice(error, p)));
        let Some(document) = &panel.document else {
            return body.child(super::skeleton(4, p));
        };
        if document.editions.is_empty() {
            return body.child(muted("No edition has recorded narration yet. Review a run through Captions to record it.", p));
        }
        let tabs = document
            .editions
            .iter()
            .enumerate()
            .map(|(index, edition)| {
                let chosen = edition.edition.clone();
                Button::new(("desktop-narration-edition", index))
                    .self_start()
                    .label(retold_app::desktop::edition_label(&edition.edition))
                    .selected(panel.edition.as_deref() == Some(edition.edition.as_str()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.narration_request(Some(chosen.clone()), cx)
                    }))
            })
            .collect::<Vec<_>>();
        let selected = document
            .editions
            .iter()
            .find(|edition| Some(edition.edition.as_str()) == panel.edition.as_deref());
        let several = document.editions.len() > 1;
        let body = body.when(several, |body| {
            body.child(div().flex().flex_wrap().gap_2().children(tabs))
        });
        let Some(edition) = selected else {
            return body;
        };
        let columns = panel.waveform.as_ref().map(|waveform| {
            waveform
                .envelope
                .peaks
                .iter()
                .map(|peak| {
                    div()
                        .w(px(2.))
                        .h(px((peak * WAVEFORM_HEIGHT).max(3.)))
                        .rounded_full()
                        .bg(p.accent)
                })
                .collect::<Vec<_>>()
        });
        let lines = edition
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let start = line.start;
                let player = panel.player.clone();
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child(
                        Button::new(("desktop-narration-line", index))
                            .self_start()
                            .ghost()
                            .w(px(76.))
                            .icon(gpui_kit::assets::IconName::Play)
                            .label(clock(line.start))
                            .tooltip("Play from here")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                player.update(cx, |player, cx| player.play_from(start, cx))
                            })),
                    )
                    .child(div().flex_1().child(line.text.clone()))
            })
            .collect::<Vec<_>>();
        body.child(heading_small(&format!(
            "{} · {}",
            retold_app::desktop::edition_label(&edition.edition),
            duration_label(edition.duration)
        )))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(match columns {
                    Some(columns) => div()
                        .flex()
                        .items_center()
                        .gap(px(1.))
                        .h(px(WAVEFORM_HEIGHT))
                        .children(columns)
                        .into_any_element(),
                    None => super::skeleton(1, p),
                })
                .child(panel.player.clone()),
        )
        .child(self.retake_controls(
            retold_app::retake::Retake::Narration,
            "Record narration again",
            cx,
        ))
        .child(heading_small("Lines"))
        .children(lines)
    }
}

/// A line start such as `0:03` or `1:42`, matching the player clock.
fn clock(seconds: f64) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "line offsets are short, non-negative durations"
    )]
    let whole = seconds.max(0.).floor() as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// A duration such as `1 min 40 s` or `45 s`.
fn duration_label(seconds: f64) -> String {
    let whole = seconds.round().max(0.);
    let minutes = (whole / 60.).floor();
    let rest = whole % 60.;
    if minutes > 0. {
        format!("{minutes} min {rest} s")
    } else {
        format!("{rest} s")
    }
}
