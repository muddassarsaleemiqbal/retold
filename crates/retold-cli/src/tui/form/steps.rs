//! Named steps for common tasks. Every field still belongs to the shared CLI contract.
use super::Form;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub title: &'static str,
    pub fields: Vec<usize>,
}

type Group = (&'static str, &'static [&'static str]);

const CREATE: &[Group] = &[
    ("Guide and channel", &["guide", "channel"]),
    (
        "Story",
        &["work", "kind", "year", "scope", "input", "source-file"],
    ),
    ("Editions and quality", &["lang", "quality", "budget"]),
];
const GUIDE: &[Group] = &[
    ("Identity", &["id", "name", "example", "language", "from"]),
    (
        "Creative direction",
        &["purpose", "style", "delivery", "writing"],
    ),
    ("Length and packaging", &["seconds", "packaging"]),
];
const CHANNEL: &[Group] = &[
    ("Identity", &["id", "name", "language", "handle", "from"]),
    ("Voice", &["voice-provider", "voice-model", "voice"]),
    ("Brand", &["accent", "font-family", "font-style"]),
];
const CONTINUE: &[Group] = &[
    ("Production", &["reference", "until"]),
    ("Allowance", &["spend", "max-requests"]),
    (
        "Execution",
        &["background", "idempotency-key", "plan-hash", "yes"],
    ),
];
const EXPORT: &[Group] = &[
    (
        "Deliveries",
        &[
            "reference",
            "edition",
            "format",
            "preset",
            "aspect",
            "project-only",
        ],
    ),
    ("Allowance", &["spend", "max-requests"]),
    (
        "Execution",
        &["background", "idempotency-key", "plan-hash", "yes"],
    ),
];

const PROVIDER: &[Group] = &[
    ("Provider", &["id", "kind"]),
    (
        "Models and login",
        &[
            "tier",
            "profile-dir",
            "key-env",
            "base-url",
            "model",
            "model-file",
        ],
    ),
    (
        "Programs and limits",
        &["binary", "espeak", "ffmpeg", "max-parallel", "timeout"],
    ),
];

/// Which actions are stepped, and their steps.
const STEPPED: [(&str, &[Group]); 12] = [
    ("providers add", PROVIDER),
    ("providers edit", PROVIDER),
    ("new", CREATE),
    ("video new", CREATE),
    ("series new", CREATE),
    ("guides create", GUIDE),
    ("channels create", CHANNEL),
    ("video continue", CONTINUE),
    ("video export", EXPORT),
    (
        "storage restore",
        &[
            ("Backup and destination", &["archive", "into"]),
            ("Review", &["dry-run"]),
        ],
    ),
    (
        "storage activate",
        &[
            ("Restored workspace", &["root"]),
            ("Ownership", &["reconcile", "attest-prior-stopped"]),
            ("Reviewed plan", &["dry-run", "plan-hash"]),
        ],
    ),
    (
        "storage backup",
        &[
            ("Scope", &["all", "video"]),
            ("Destination", &["output", "dry-run"]),
        ],
    ),
];

pub fn for_form(form: &Form) -> Vec<Step> {
    let command = form.action.command();
    STEPPED
        .iter()
        .find(|(path, _)| *path == command)
        .map(|(_, groups)| grouped(form, groups))
        .unwrap_or_default()
}

fn grouped(form: &Form, groups: &[Group]) -> Vec<Step> {
    let mut steps: Vec<Step> = groups
        .iter()
        .map(|(title, names)| Step {
            title,
            fields: form
                .action
                .fields
                .iter()
                .enumerate()
                .filter(|(_, field)| names.contains(&field.id.as_str()))
                .map(|(index, _)| index)
                .collect(),
        })
        .filter(|step| !step.fields.is_empty())
        .collect();
    let rest = (0..form.entries.len())
        .filter(|index| !steps.iter().any(|step| step.fields.contains(index)))
        .collect();
    steps.push(Step {
        title: "Review options",
        fields: rest,
    });
    steps
}

impl Form {
    pub fn visible_fields(&self) -> Vec<usize> {
        self.steps.get(self.stage).map_or_else(
            || (0..self.entries.len()).collect(),
            |step| step.fields.clone(),
        )
    }

    pub fn focus_row(&self) -> usize {
        self.visible_fields()
            .iter()
            .position(|index| *index == self.focus)
            .unwrap_or_else(|| self.visible_fields().len())
    }

    pub const fn last_step(&self) -> bool {
        self.stage.saturating_add(1) >= self.steps.len()
    }

    pub fn previous_step(&mut self) {
        self.stage = self.stage.saturating_sub(1);
        self.focus_first();
    }

    pub(super) fn next_step(&mut self) {
        self.stage = self.stage.saturating_add(1);
        self.focus_first();
    }

    fn focus_first(&mut self) {
        self.focus = self
            .visible_fields()
            .first()
            .copied()
            .unwrap_or_else(|| self.button());
    }

    pub fn step_title(&self) -> Option<String> {
        self.steps.get(self.stage).map(|step| {
            format!(
                "Step {} of {} · {}",
                self.stage + 1,
                self.steps.len(),
                step.title
            )
        })
    }
}

impl Form {
    pub fn field_at_row(&self, row: usize) -> usize {
        self.visible_fields()
            .get(row)
            .copied()
            .unwrap_or_else(|| self.button())
    }
}
