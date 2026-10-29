//! Offline guide questions, with per-guide edits retained while exploring choices.
use std::collections::BTreeMap;

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{AppContext, Context, Entity, Subscription, Window};
use retold_app::creation::Options;
use retold_app::desktop::{DraftChoices, DraftField};

use super::Shell;

#[derive(Debug)]
pub(super) struct DraftForm {
    pub choices: DraftChoices,
    pub guide: usize,
    pub channel: usize,
    pub languages: Vec<String>,
    pub fields: Vec<(DraftField, Entity<TextareaState>)>,
    pub budget: Entity<InputState>,
    pub source: Entity<InputState>,
    pub preview: Option<retold_app::creation::SourcePreview>,
    _source_subscription: Subscription,
    retained: BTreeMap<String, BTreeMap<String, String>>,
}

impl DraftForm {
    pub fn new(choices: DraftChoices, window: &mut Window, cx: &mut Context<Shell>) -> Self {
        let budget = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value("0.00", window, cx);
            state
        });
        let source =
            cx.new(|cx| InputState::new(window, cx).placeholder("Optional UTF-8 source file"));
        let source_subscription = cx.subscribe(&source, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                if let Some(form) = &mut this.draft {
                    form.preview = None;
                }
                cx.notify();
            }
        });
        let mut form = Self {
            choices,
            guide: 0,
            channel: 0,
            languages: Vec::new(),
            fields: Vec::new(),
            budget,
            source,
            preview: None,
            _source_subscription: source_subscription,
            retained: BTreeMap::new(),
        };
        form.select_guide(0, window, cx);
        form
    }

    pub fn select_guide(&mut self, index: usize, window: &mut Window, cx: &mut Context<Shell>) {
        if let Some(previous) = self.choices.guides.get(self.guide) {
            let values = self
                .fields
                .iter()
                .map(|(field, state)| (field.name.clone(), state.read(cx).value().to_string()))
                .collect();
            self.retained.insert(previous.id.clone(), values);
        }
        let Some(guide) = self.choices.guides.get(index) else {
            return;
        };
        self.guide = index;
        self.languages = guide.languages.first().cloned().into_iter().collect();
        self.fields = guide
            .fields
            .iter()
            .map(|field| {
                let saved = self
                    .retained
                    .get(&guide.id)
                    .and_then(|values| values.get(&field.name))
                    .unwrap_or(&field.default)
                    .clone();
                let state = cx.new(|cx| {
                    let mut state = TextareaState::new(window, cx);
                    state.set_value(saved, window, cx);
                    state
                });
                (field.clone(), state)
            })
            .collect();
    }

    pub fn options(&self, cx: &Context<Shell>) -> Options {
        Options {
            guide: self
                .choices
                .guides
                .get(self.guide)
                .map(|guide| guide.id.clone()),
            channel: self.choices.channels.get(self.channel).cloned(),
            languages: self.languages.clone(),
            inputs: self
                .fields
                .iter()
                .map(|(field, state)| (field.name.clone(), state.read(cx).value().to_string()))
                .filter(|(_, value)| !value.is_empty())
                .collect(),
            budget: Some(self.budget.read(cx).value().to_string()),
            source_file: self.source_path(cx),
            ..Options::default()
        }
    }

    pub fn source_path(&self, cx: &Context<Shell>) -> Option<std::path::PathBuf> {
        let value = self.source.read(cx).value();
        (!value.trim().is_empty()).then(|| std::path::PathBuf::from(value.as_ref()))
    }

    pub fn source_ready(&self, cx: &Context<Shell>) -> bool {
        self.source_path(cx).is_none_or(|path| {
            self.preview
                .as_ref()
                .is_some_and(|preview| preview.path == path)
        })
    }
}
