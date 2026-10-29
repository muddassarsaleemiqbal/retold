//! The scene picker reuses the virtualized native command dialog and preserves current scope.
use std::cell::Cell;

use gpui_kit::component::{
    WindowExt as _,
    command::{Command, CommandItem, CommandState},
};
use gpui_kit::{AppContext as _, Context, ParentElement as _, Window, px};

use super::super::Shell;

const NARRATION_LABEL_CHARS: usize = 72;

impl Shell {
    pub(in crate::shell) fn show_scene_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || window.has_active_dialog(cx) {
            return;
        }
        let Some(editor) = &self.scenes else {
            return;
        };
        if editor.dirty(cx) || editor.incoming.is_some() {
            return;
        }
        let Some(content) = editor.content() else {
            return;
        };
        let scope = (
            editor.library.clone(),
            editor.reference.clone(),
            editor.player.entity_id(),
        );
        let entries: Vec<_> = content
            .board
            .units
            .iter()
            .map(|unit| {
                let excerpt: String = unit.narration.chars().take(NARRATION_LABEL_CHARS).collect();
                (
                    unit.id,
                    format!(
                        "Scene {} · {} · {excerpt}",
                        unit.id.saturating_add(1),
                        unit.section
                    ),
                )
            })
            .collect();
        let state = cx.new(|cx| CommandState::new(window, cx));
        let focus = Cell::new(true);
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            if focus.replace(false) {
                state.update(cx, |state, cx| state.focus(window, cx));
            }
            let entries = entries.clone();
            let choices = entries.clone();
            let scope = scope.clone();
            let weak = weak.clone();
            dialog.title("Choose a scene").w(px(640.)).child(
                Command::new(&state)
                    .placeholder("Find a scene, section or narration…")
                    .items(
                        entries
                            .iter()
                            .map(|(_, label)| CommandItem::new().label(label.clone())),
                    )
                    .on_confirm(move |index, window, cx| {
                        let Some((unit, _)) = choices.get(index.row) else {
                            return;
                        };
                        window.close_dialog(cx);
                        let _ = weak.update(cx, |this, cx| {
                            let same = this.scenes.as_ref().is_some_and(|editor| {
                                editor.library == scope.0
                                    && editor.reference == scope.1
                                    && editor.player.entity_id() == scope.2
                            });
                            if same {
                                this.select_scene(*unit, window, cx);
                            }
                        });
                    }),
            )
        });
    }
}
