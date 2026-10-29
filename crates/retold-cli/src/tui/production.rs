//! A reviewed plan pins the exact authority submitted by the Spend Gate.
use super::form::Form;

#[derive(Debug, Clone)]
pub struct Review {
    pub form: Form,
    pub words: Vec<String>,
    pub scroll: u16,
}

/// Actions reviewed before running, and how each builds its read-only plan command.
/// Builds the read-only plan command for an action's form.
type Planner = fn(&Form) -> Vec<String>;

const PLANNERS: [(&str, Planner); 4] = [
    ("video continue", continue_plan),
    ("video export", export_plan),
    ("series run", series_plan),
    ("providers test", test_plan),
];

impl Review {
    pub fn new(form: Form) -> Option<Self> {
        let command = form.action.command();
        let planner = PLANNERS.iter().find(|(path, _)| *path == command)?.1;
        let words = planner(&form);
        Some(Self {
            form,
            words,
            scroll: 0,
        })
    }

    /// The form to run once the plan is clear: pinned to the plan hash that was shown.
    pub fn executable(&self, contents: &serde_json::Value) -> Option<Form> {
        let plan = plan(contents);
        let blockers = plan["blockers"].as_array()?;
        if !blockers.is_empty() {
            return None;
        }
        let hash = plan["plan_hash"].as_str()?;
        Some(with_values(
            &self.form,
            &[("plan-hash", hash), ("yes", "false")],
        ))
    }
}

/// A copy of `form` with each `(id, value)` filled in.
fn with_values(form: &Form, values: &[(&str, &str)]) -> Form {
    values
        .iter()
        .fold(form.clone(), |filled, (id, value)| filled.with(id, value))
}

/// The plan inside a result: provider tests nest theirs under `test_plan`.
#[must_use]
pub fn plan(contents: &serde_json::Value) -> &serde_json::Value {
    contents.get("test_plan").unwrap_or(contents)
}

fn continue_plan(form: &Form) -> Vec<String> {
    let mut words = vec!["video".into(), "plan".into()];
    words.extend(options(form, &["until", "spend", "max-requests"]));
    words.push(form.value("reference").unwrap_or_default());
    words
}

fn export_plan(form: &Form) -> Vec<String> {
    let preview = [
        ("dry-run", "true"),
        ("background", "false"),
        ("idempotency-key", ""),
        ("plan-hash", ""),
        ("yes", "false"),
    ];
    with_values(form, &preview).arguments()
}

fn test_plan(form: &Form) -> Vec<String> {
    with_values(form, &[("yes", "false"), ("plan-hash", "")]).arguments()
}

fn series_plan(form: &Form) -> Vec<String> {
    let mut words = vec!["series".into(), "plan".into()];
    words.extend(options(form, &["parts", "spend", "max-requests"]));
    words.push(form.value("series").unwrap_or_default());
    words
}

fn options(form: &Form, ids: &[&str]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| form.value(id).map(|value| (id, value)))
        .flat_map(|(id, value)| [format!("--{id}"), value])
        .collect()
}
