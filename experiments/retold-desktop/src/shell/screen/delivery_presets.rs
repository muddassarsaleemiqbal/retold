//! Preset and aspect choice inside the delivery review.
use gpui_kit::component::{Disableable as _, Selectable as _, button::Button};
use gpui_kit::{Context, Div, div, prelude::*};

use super::{heading_small, muted};
use crate::shell::Shell;

impl Shell {
    pub(super) fn delivery_presets(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(panel) = &self.delivery else {
            return div();
        };
        let Some(presets) = &panel.presets else {
            return div().child(muted("Reading export presets…", p));
        };
        if presets.is_empty() {
            return div();
        }
        let chosen = panel.options.preset.as_deref();
        let mut choices = vec![
            Button::new(("desktop-export-preset", 0usize))
                .self_start()
                .label("Guide defaults")
                .selected(chosen.is_none())
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| this.choose_delivery_preset(None, cx))),
        ];
        choices.extend(presets.iter().enumerate().map(|(index, preset)| {
            let name = preset.name.clone();
            Button::new(("desktop-export-preset", index + 1))
                .self_start()
                .label(format!("{} · {}", preset.name, preset.container))
                .selected(chosen == Some(preset.name.as_str()))
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.choose_delivery_preset(Some(name.clone()), cx)
                }))
        }));
        let aspects = presets
            .iter()
            .find(|preset| Some(preset.name.as_str()) == chosen)
            .map(|preset| {
                preset
                    .aspects
                    .iter()
                    .enumerate()
                    .map(|(index, aspect)| {
                        let value = aspect.clone();
                        Button::new(("desktop-export-aspect", index))
                            .self_start()
                            .label(aspect.clone())
                            .selected(panel.options.aspect.as_deref() == Some(aspect.as_str()))
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.choose_delivery_aspect(value.clone(), cx)
                            }))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Preset"))
            .child(div().flex().flex_wrap().gap_2().children(choices))
            .when(!aspects.is_empty(), |body| {
                body.child(div().flex().flex_wrap().gap_2().children(aspects))
            })
    }
}
