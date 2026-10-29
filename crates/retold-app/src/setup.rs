//! Initial workspace onboarding and setup without implicit generation or spend.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupReport {
    pub library_root: String,
    pub initialized_dirs: Vec<String>,
    pub detected_providers: Vec<String>,
    pub next_steps: Vec<String>,
}

fn initialize_folder(target: &Path) -> Result<bool, Diagnostic> {
    if target.is_dir() {
        return Ok(false);
    }
    fs::create_dir_all(target)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    Ok(true)
}

fn initialize_workspace_dirs(
    context: &crate::workspace::context::Context,
) -> Result<Vec<String>, Diagnostic> {
    fs::create_dir_all(&context.library)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    let mut initialized = Vec::new();
    for (folder, target) in context.directories() {
        let created = initialize_folder(&target)?;
        if created {
            initialized.push(folder.to_string());
        }
    }
    Ok(initialized)
}

fn scan_host_tools() -> Vec<String> {
    let search_path = std::env::var_os("PATH").unwrap_or_default();
    let home_opt = directories::BaseDirs::new().map(|base| base.home_dir().to_path_buf());
    let home_dir = home_opt.unwrap_or_else(|| PathBuf::from("."));
    retold_engine::provider_discovery::scan(&home_dir, &search_path).map_or_else(
        |_| Vec::new(),
        |inventory| {
            inventory
                .tools
                .into_iter()
                .map(|tool| tool.name.to_string())
                .collect()
        },
    )
}

fn default_next_steps() -> Vec<String> {
    vec![
        "Create your guide: retold guides create <id>".to_string(),
        "Create your channel: retold channels create <id>".to_string(),
        "Configure providers: retold providers add <id> --file <definition>".to_string(),
        "Create your first video: retold new <guide> --work <title> --channel <id>".to_string(),
    ]
}

/// Creates the configuration folder and an empty `providers.toml` when missing; the full-screen
/// app calls this on start so a fresh installation never begins with configuration errors.
pub fn ensure_config(context: &crate::workspace::context::Context) -> Result<(), Diagnostic> {
    initialize_config(&context.config).map(drop)
}

fn initialize_config(config: &Path) -> Result<Vec<String>, Diagnostic> {
    let created = retold_engine::provider_settings::ensure(config)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    Ok(if created {
        vec![config.join("providers.toml").display().to_string()]
    } else {
        Vec::new()
    })
}

pub fn run_setup(context: &crate::workspace::context::Context) -> Result<Payload, Diagnostic> {
    let mut initialized_dirs = initialize_workspace_dirs(context)?;
    initialized_dirs.extend(initialize_config(&context.config)?);
    let detected_providers = scan_host_tools();
    let next_steps = default_next_steps();
    let report = SetupReport {
        library_root: context.library.display().to_string(),
        initialized_dirs,
        detected_providers,
        next_steps,
    };
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": "Workspace setup completed successfully. Zero requests sent.",
            "report": report,
        }),
    })
}
