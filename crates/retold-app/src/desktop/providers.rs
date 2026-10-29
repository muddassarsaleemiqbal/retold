//! Offline configuration reads and exact-source reviewed writes for native clients.
use crate::protocol::{Diagnostic, ErrorCode};
pub use retold_engine::provider_discovery::Inventory;
pub use retold_engine::provider_settings::reviewed::{Change, Review, Snapshot};
mod observations;
pub use observations::{ObservationSet, observations};

use std::path::{Path, PathBuf};

pub fn inspect(root: &Path) -> Result<Snapshot, Diagnostic> {
    retold_engine::provider_settings::reviewed::inspect(root)
        .map_err(|error| configuration_error(&error))
}

pub fn preview(root: &Path, version: &str, change: &Change) -> Result<Review, Diagnostic> {
    retold_engine::provider_settings::reviewed::preview(root, version, change)
        .map_err(|error| configuration_error(&error))
}

pub fn apply(review: &Review) -> Result<Snapshot, Diagnostic> {
    let _owner = crate::account::activity("save reviewed providers")
        .map_err(|error| crate::account::commands::diagnostic(&error))?;
    retold_engine::recovery::ensure_active(review.root())
        .map_err(|error| Diagnostic::new(ErrorCode::InputRequired, &error.to_string()))?;
    retold_engine::provider_settings::reviewed::apply(review)
        .map_err(|error| configuration_error(&error))?;
    inspect(review.root())
}

pub fn default_directory() -> Result<PathBuf, Diagnostic> {
    retold_engine::Paths::discover()
        .map(|paths| paths.config)
        .map_err(|error| Diagnostic::new(ErrorCode::InputRequired, &error.to_string()))
}

pub fn scan() -> Result<Inventory, Diagnostic> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| Diagnostic::new(ErrorCode::InputRequired, "Home folder is unavailable."))?;
    let search_path = std::env::var_os("PATH").unwrap_or_default();
    retold_engine::provider_discovery::scan(base.home_dir(), &search_path)
        .map_err(|message| Diagnostic::new(ErrorCode::DataInvalid, &message))
}

fn configuration_error(error: &retold_engine::provider_settings::AddError) -> Diagnostic {
    use retold_engine::provider_settings::AddError;
    let code = match error {
        AddError::Changed => ErrorCode::PlanChanged,
        AddError::Invalid(_) => ErrorCode::InputRequired,
        AddError::Busy | AddError::Storage(_) => ErrorCode::ExecutionFailed,
    };
    Diagnostic::new(code, &error.to_string())
}

const PROVIDER_KIND_COUNT: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Kind {
    Codex,
    Claude,
    Gemini,
    OpenRouter,
    LocalText,
    LocalVoice,
    LocalWhisper,
}
impl Kind {
    pub const ALL: [Self; PROVIDER_KIND_COUNT] = [
        Self::Codex,
        Self::Claude,
        Self::Gemini,
        Self::OpenRouter,
        Self::LocalText,
        Self::LocalVoice,
        Self::LocalWhisper,
    ];
    #[must_use]
    pub const fn label(self) -> &'static str {
        KIND_DETAILS[self as usize].0
    }
    const fn key(self) -> &'static str {
        KIND_DETAILS[self as usize].1
    }
}

const KIND_DETAILS: [(&str, &str); PROVIDER_KIND_COUNT] = [
    ("Codex CLI", "codex_cli"),
    ("Claude CLI", "claude_cli"),
    ("Gemini CLI", "gemini_cli"),
    ("OpenRouter", "openrouter"),
    ("Local text", "local_text"),
    ("Local speech", "local_voice"),
    ("Local transcription", "local_whisper"),
];

/// Guided fields build the same typed definition as advanced TOML; no secret values belong here.
#[derive(Debug)]
pub struct DefinitionDraft<'a> {
    pub kind: Kind,
    pub parallel: &'a str,
    pub binary: &'a str,
    pub profile: &'a str,
    pub key_variable: &'a str,
    pub selector: &'a str,
    pub model: &'a str,
    pub ffmpeg: &'a str,
}

impl DefinitionDraft<'_> {
    pub fn definition(&self) -> Result<String, Diagnostic> {
        let parallel = parallelism(self.parallel)?;
        let fields = definition_table(self, parallel);
        toml::to_string_pretty(&fields).map_err(|_| {
            Diagnostic::new(
                ErrorCode::DataInvalid,
                "Could not build the provider definition.",
            )
        })
    }
}

fn parallelism(text: &str) -> Result<u32, Diagnostic> {
    text.trim().parse().map_err(|_| {
        Diagnostic::new(
            ErrorCode::InputRequired,
            "Parallel requests must be a positive integer.",
        )
    })
}

fn definition_table(draft: &DefinitionDraft<'_>, parallel: u32) -> toml::Table {
    scalar_fields(draft)
        .into_iter()
        .chain(common_fields(draft.kind, parallel))
        .chain(model_fields(draft).map(model_entry))
        .collect()
}

fn common_fields(kind: Kind, parallel: u32) -> [(String, toml::Value); 2] {
    [
        ("kind".into(), kind.key().into()),
        ("max_parallel".into(), i64::from(parallel).into()),
    ]
}

fn model_entry((name, table): (&str, toml::Table)) -> (String, toml::Value) {
    (name.into(), table.into())
}

fn scalar_fields(draft: &DefinitionDraft<'_>) -> toml::Table {
    scalar_pairs(draft)
        .into_iter()
        .filter_map(|(key, value)| optional_field(key, value))
        .collect()
}

fn scalar_pairs<'a>(draft: &DefinitionDraft<'a>) -> Vec<(&'static str, &'a str)> {
    let tool = match draft.kind {
        Kind::OpenRouter => ("key_env", draft.key_variable),
        Kind::LocalVoice => ("espeak", draft.binary),
        _ => ("binary", draft.binary),
    };
    let cli_profile = matches!(draft.kind, Kind::Codex | Kind::Claude);
    let whisper = draft.kind == Kind::LocalWhisper;
    [
        Some(tool),
        cli_profile.then_some(("config_dir", draft.profile)),
        whisper.then_some(("model", draft.model)),
        whisper.then_some(("ffmpeg", draft.ffmpeg)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn optional_field(name: &str, text: &str) -> Option<(String, toml::Value)> {
    let text = text.trim();
    (!text.is_empty()).then(|| (name.trim().into(), text.into()))
}

fn model_fields(draft: &DefinitionDraft<'_>) -> Option<(&'static str, toml::Table)> {
    let name = model_table(draft.kind)?;
    let model = optional_field(draft.selector, draft.model);
    Some((name, model.into_iter().collect()))
}

fn model_table(kind: Kind) -> Option<&'static str> {
    if kind == Kind::LocalText {
        return Some("models");
    }
    let cli = matches!(kind, Kind::Codex | Kind::Claude | Kind::Gemini);
    cli.then_some("tiers")
}

#[cfg(test)]
mod tests {
    use super::{Change, DefinitionDraft, Kind, inspect, preview};
    #[test]
    fn guided_adapters_match_engine_schema_without_touching_disk() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("missing-config");
        let snapshot = inspect(&root).unwrap();
        for kind in Kind::ALL {
            let draft = DefinitionDraft {
                kind,
                parallel: "2",
                binary: "/tools/اردو tool",
                profile: "/profiles/हिन्दी",
                key_variable: "MY_PROVIDER_KEY",
                selector: "smart",
                model: "/models/quoted \" model",
                ffmpeg: "ffmpeg",
            };
            let definition = draft.definition().unwrap();
            let change = Change::Add {
                id: "fixture".into(),
                definition,
            };
            preview(&root, &snapshot.version, &change).unwrap();
        }
        assert!(!root.exists());
    }
    #[test]
    fn malformed_parallelism_never_becomes_an_accepted_definition() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("missing-config");
        let snapshot = inspect(&root).unwrap();
        for parallel in ["0", "-1", "hello", "18446744073709551616"] {
            let draft = DefinitionDraft {
                kind: Kind::LocalVoice,
                parallel,
                binary: "",
                profile: "",
                key_variable: "",
                selector: "",
                model: "",
                ffmpeg: "",
            };
            if let Ok(definition) = draft.definition() {
                let change = Change::Add {
                    id: "fixture".into(),
                    definition,
                };
                assert!(preview(&root, &snapshot.version, &change).is_err());
            }
        }
        assert!(!root.exists());
    }
}
