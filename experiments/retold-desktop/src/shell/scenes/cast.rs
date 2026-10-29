//! A local cast draft: add, remove or change the expression of people in one scene.
use gpui_kit::Context;
use retold_app::scenes::{CastDraft, CastPose, MAX_CAST, PROFILE_EXPRESSIONS};

use super::SceneEditor;
use crate::shell::Shell;

impl SceneEditor {
    pub(in crate::shell) fn saved_cast(&self) -> Vec<CastPose> {
        self.content()
            .and_then(|content| {
                content
                    .board
                    .direction
                    .shots
                    .iter()
                    .find(|shot| shot.unit == self.unit)
            })
            .map(|shot| {
                shot.characters
                    .iter()
                    .map(|name| CastPose {
                        name: name.clone(),
                        expression: shot
                            .poses
                            .iter()
                            .find(|pose| &pose.name == name)
                            .map(|pose| pose.expression.clone())
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The cast shown in the editor: the local draft when present, otherwise the saved cast.
    pub(in crate::shell) fn shown_cast(&self) -> Vec<CastPose> {
        self.cast.clone().unwrap_or_else(|| self.saved_cast())
    }

    pub(in crate::shell) fn cast_dirty(&self) -> bool {
        self.cast
            .as_ref()
            .is_some_and(|cast| *cast != self.saved_cast())
    }

    pub(in crate::shell) fn cast_draft(&self) -> Option<CastDraft> {
        let cast = self.cast.clone().filter(|_| self.cast_dirty())?;
        Some(CastDraft {
            expected_version: self.content()?.version.clone(),
            unit: self.unit,
            cast,
        })
    }

    /// Profile expressions in their saved order; the engine accepts the first five.
    pub(in crate::shell) fn expressions(&self, name: &str) -> Vec<String> {
        self.content()
            .and_then(|content| {
                content
                    .board
                    .plan
                    .cast
                    .iter()
                    .find(|profile| profile.name == name)
            })
            .map(|profile| {
                profile
                    .emotions
                    .iter()
                    .take(PROFILE_EXPRESSIONS)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(in crate::shell) fn available_cast(&self) -> Vec<String> {
        let shown = self.shown_cast();
        self.content()
            .map(|content| {
                content
                    .board
                    .plan
                    .cast
                    .iter()
                    .filter(|profile| !shown.iter().any(|pose| pose.name == profile.name))
                    .map(|profile| profile.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub(in crate::shell) enum CastChange {
    Add(String),
    Remove(String),
    NextExpression(String),
    Discard,
}

impl SceneEditor {
    fn next_expression(&self, cast: &mut [CastPose], name: &str) {
        let options = self.expressions(name);
        let Some(pose) = cast.iter_mut().find(|pose| pose.name == name) else {
            return;
        };
        let current = options
            .iter()
            .position(|option| option.eq_ignore_ascii_case(&pose.expression));
        let next = current.map_or(0, |index| (index + 1) % options.len());
        if let Some(option) = options.get(next) {
            pose.expression.clone_from(option);
        }
    }

    fn apply_cast(&mut self, change: CastChange) {
        let mut cast = self.shown_cast();
        match change {
            CastChange::Add(name) if cast.len() < MAX_CAST => {
                let expression = self.expressions(&name).first().cloned().unwrap_or_default();
                cast.push(CastPose { name, expression });
            }
            CastChange::Add(_) => {
                self.error = Some(format!("A scene can show at most {MAX_CAST} characters."));
            }
            CastChange::Remove(name) => cast.retain(|pose| pose.name != name),
            CastChange::NextExpression(name) => self.next_expression(&mut cast, &name),
            CastChange::Discard => {
                self.cast = None;
                self.cast_review = None;
                return;
            }
        }
        self.cast = Some(cast);
        self.cast_review = None;
    }
}

impl Shell {
    pub(in crate::shell) fn change_cast(&mut self, change: CastChange, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(editor) = &mut self.scenes {
            editor.apply_cast(change);
            self.checkpoint_scene(cx);
            cx.notify();
        }
    }
}
