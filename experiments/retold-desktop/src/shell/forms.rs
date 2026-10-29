//! Operation forms: every Retold operation as real controls, generated from its definition.
//! Fields that name the current video or series are filled in from the page that opened them.
//! Previewable operations go Preview → Apply; other changes are summarised and confirmed first.
use std::collections::BTreeMap;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, PathPromptOptions, Subscription, Window};
use retold_app::protocol::Payload;
use retold_cli::forms::{self, FieldKind, Operation, Value};

use super::Shell;

/// Field names that mean "the video this page shows".
const VIDEO_FIELDS: [&str; 3] = ["reference", "video", "ref"];
/// Confirmation flags the form itself replaces with its review step.
const CONSENT_FIELDS: [&str; 1] = ["yes"];

/// Verbs whose operations only read; everything else is summarised and confirmed first.
const READING_VERBS: [&str; 31] = [
    "list",
    "show",
    "check",
    "verify",
    "schema",
    "usage",
    "explain",
    "health",
    "scan",
    "status",
    "attempts",
    "processes",
    "costs",
    "plan",
    "history",
    "paths",
    "uses",
    "review",
    "home",
    "spend",
    "diagnostics",
    "completions",
    "man",
    "document",
    "validate",
    "audit",
    "prompt",
    "optimize",
    "beats",
    "benchmark",
    "coverage",
];

/// Switches that turn a reading action into one that changes saved work.
const CHANGING_SWITCHES: [&str; 5] = ["reconcile", "fix", "remove", "force", "apply"];

impl OperationForm {
    /// Whether running now only reads: a reading operation with no changing switch on.
    pub(super) fn reads_only(&self) -> bool {
        let switched = self
            .toggles
            .iter()
            .any(|(id, on)| *on && CHANGING_SWITCHES.contains(&id.as_str()));
        reads_only(&self.operation.path) && !switched
    }
}

/// What the form should do next.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    /// Read-only operations run at once.
    Run,
    /// Run with the operation's own dry-run switch.
    Preview,
    /// Show the entered values for confirmation.
    Review,
    /// Run for real after review or preview.
    Apply,
}

pub(super) struct OperationForm {
    pub operation: Operation,
    pub inputs: BTreeMap<String, Entity<InputState>>,
    pub toggles: BTreeMap<String, bool>,
    pub choices: BTreeMap<String, String>,
    pub previewed: bool,
    pub reviewing: bool,
    pub result: Option<Payload>,
    pub error: Option<String>,
    pub choices_for: BTreeMap<String, super::form_choices::Choices>,
    _changes: Vec<Subscription>,
}

/// Every operation, derived from the command definitions once and reused; building the
/// command tree is too costly to repeat on each frame.
pub(super) fn all_operations() -> &'static [Operation] {
    static OPERATIONS: std::sync::OnceLock<Vec<Operation>> = std::sync::OnceLock::new();
    OPERATIONS.get_or_init(forms::operations)
}

/// Whether an operation only reads saved work.
pub(super) fn reads_only(path: &str) -> bool {
    let verb = path.rsplit(' ').next().unwrap_or_default();
    path.starts_with("creative ") || READING_VERBS.contains(&verb)
}

/// Fields the user sees: consent flags and dry-run are handled by the form's buttons.
pub(super) fn shown(operation: &Operation) -> impl Iterator<Item = &forms::Field> {
    operation
        .fields
        .iter()
        .filter(|field| !CONSENT_FIELDS.contains(&field.id.as_str()) && field.id != "dry-run")
}

impl OperationForm {
    pub(super) fn new(
        operation: Operation,
        prefill: &BTreeMap<String, String>,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) -> Self {
        let mut inputs = BTreeMap::new();
        let mut changes = Vec::new();
        let mut choices = BTreeMap::new();
        for field in shown(&operation) {
            match &field.kind {
                FieldKind::Toggle => {}
                FieldKind::Choice(options) => {
                    let initial = field
                        .default
                        .clone()
                        .or_else(|| field.required.then(|| options[0].clone()));
                    if let Some(initial) = initial {
                        choices.insert(field.id.clone(), initial);
                    }
                }
                _ => {
                    let value = prefill
                        .get(&field.id)
                        .cloned()
                        .or_else(|| field.default.clone())
                        .unwrap_or_default();
                    let placeholder = if field.repeated {
                        "Separate several values with commas"
                    } else {
                        ""
                    };
                    let input = cx.new(|cx| {
                        let mut state = InputState::new(window, cx).placeholder(placeholder);
                        state.set_value(value, window, cx);
                        state
                    });
                    changes.push(cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            if let Some(form) = &mut this.form {
                                form.previewed = false;
                                form.reviewing = false;
                            }
                            cx.notify();
                        }
                    }));
                    inputs.insert(field.id.clone(), input);
                }
            }
        }
        Self {
            operation,
            inputs,
            toggles: BTreeMap::new(),
            choices,
            previewed: false,
            reviewing: false,
            result: None,
            error: None,
            choices_for: BTreeMap::new(),
            _changes: changes,
        }
    }

    fn values(&self, cx: &Context<Shell>) -> Vec<(String, Value)> {
        let mut values: Vec<(String, Value)> = self
            .toggles
            .iter()
            .filter(|(_, on)| **on)
            .map(|(id, _)| (id.clone(), Value::On))
            .collect();
        values.extend(
            self.choices
                .iter()
                .map(|(id, choice)| (id.clone(), Value::Text(choice.clone()))),
        );
        for field in shown(&self.operation) {
            let Some(input) = self.inputs.get(&field.id) else {
                continue;
            };
            let text = input.read(cx).value().trim().to_owned();
            let value = if field.repeated {
                Value::Many(
                    text.split(',')
                        .map(|part| part.trim().to_owned())
                        .filter(|part| !part.is_empty())
                        .collect(),
                )
            } else {
                Value::Text(text)
            };
            values.push((field.id.clone(), value));
        }
        values
    }

    /// Plain-language summary of the entered values, for the review step.
    pub(super) fn summary(&self, cx: &Context<Shell>) -> Vec<String> {
        self.values(cx)
            .into_iter()
            .filter_map(|(id, value)| {
                let label = self
                    .operation
                    .fields
                    .iter()
                    .find(|field| field.id == id)?
                    .label
                    .clone();
                let shown = match value {
                    Value::On => "on".to_owned(),
                    Value::Text(text) if text.is_empty() => return None,
                    Value::Text(text) => text,
                    Value::Many(values) if values.is_empty() => return None,
                    Value::Many(values) => values.join(", "),
                };
                Some(format!("{label}: {shown}"))
            })
            .collect()
    }

    fn arguments(&self, step: Step, cx: &Context<Shell>) -> Result<Vec<String>, String> {
        let mut values = self.values(cx);
        if step == Step::Preview {
            values.push(("dry-run".into(), Value::On));
        }
        let consents = self
            .operation
            .fields
            .iter()
            .filter(|field| CONSENT_FIELDS.contains(&field.id.as_str()));
        if step == Step::Apply {
            values.extend(consents.map(|field| (field.id.clone(), Value::On)));
        }
        let numbers = shown(&self.operation).filter(|field| field.kind == FieldKind::Number);
        for field in numbers {
            let entered = values.iter().find(|(id, _)| *id == field.id);
            let invalid = matches!(entered, Some((_, Value::Text(text))) if !text.is_empty() && text.parse::<u64>().is_err());
            if invalid {
                return Err(format!("{} must be a whole number.", field.label));
            }
        }
        forms::arguments(&self.operation, &values)
    }
}

impl Shell {
    /// Open the form for `path`, filling fields that name this page's video or series.
    pub(super) fn open_form(&mut self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(operation) = all_operations()
            .iter()
            .find(|operation| operation.path == path)
            .cloned()
        else {
            return;
        };
        let mut prefill = BTreeMap::new();
        if let Some(reference) = &self.selected_video {
            for field in VIDEO_FIELDS {
                prefill.insert(field.to_owned(), reference.clone());
            }
        }
        if path.starts_with("account ") {
            prefill.insert("scope".into(), "account".into());
        }
        if path.starts_with("series ")
            && let Some(matrix) = &self.series.matrix
        {
            prefill.insert("id".into(), matrix.manifest.id.clone());
            prefill.insert("series".into(), matrix.manifest.id.clone());
        }
        let suggestions = self.field_choices(path);
        let mut form = OperationForm::new(operation, &prefill, window, cx);
        form.choices_for = suggestions;
        self.form = Some(form);
        cx.notify();
    }

    pub(super) fn toggle_field(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.form {
            let on = form.toggles.entry(id.to_owned()).or_default();
            *on = !*on;
            form.previewed = false;
            form.reviewing = false;
        }
        cx.notify();
    }

    pub(super) fn choose_field(&mut self, id: &str, choice: &str, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.form {
            form.choices.insert(id.to_owned(), choice.to_owned());
            form.previewed = false;
            form.reviewing = false;
        }
        cx.notify();
    }

    /// Fill a path field from the system file chooser.
    pub(super) fn choose_path(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let chooser = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let chosen = chooser
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            let _ = this.update_in(cx, |this, window, cx| {
                let input = this
                    .form
                    .as_ref()
                    .and_then(|form| form.inputs.get(&id))
                    .cloned();
                if let (Some(path), Some(input)) = (chosen, input) {
                    input.update(cx, |state, cx| {
                        state.set_value(path.display().to_string(), window, cx)
                    });
                }
            });
        })
        .detach();
    }

    pub(super) fn form_step(&mut self, step: Step, cx: &mut Context<Self>) {
        let Some(form) = &mut self.form else {
            return;
        };
        if step == Step::Review {
            form.reviewing = true;
            cx.notify();
            return;
        }
        let arguments = match form.arguments(step, cx) {
            Ok(arguments) => arguments,
            Err(error) => {
                form.error = Some(error);
                cx.notify();
                return;
            }
        };
        form.error = None;
        let mut command = arguments;
        if let Some(snapshot) = &self.snapshot {
            command.splice(
                0..0,
                [
                    "--workspace".to_owned(),
                    snapshot.context.root.display().to_string(),
                ],
            );
        }
        self.busy = true;
        let work = cx
            .background_executor()
            .spawn(async move { retold_cli::run_operation(&command).0 });
        cx.spawn(async move |this, cx| {
            let payload = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let succeeded = !matches!(payload, Payload::Error { .. });
                if let Some(form) = &mut this.form {
                    form.previewed = step == Step::Preview && succeeded;
                    form.reviewing = false;
                    form.result = Some(payload);
                }
                if step == Step::Apply && succeeded {
                    this.refresh(cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Shell {
    /// Fill a text field from one of its pick-list entries.
    pub(super) fn pick_value(
        &mut self,
        id: &str,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self
            .form
            .as_ref()
            .and_then(|form| form.inputs.get(id))
            .cloned();
        if let Some(input) = input {
            input.update(cx, |state, cx| state.set_value(value, window, cx));
        }
    }
}
