//! Offline credential profile inspection and configuration for registered providers.
use crate::provider_settings::{self, AddError, Edit};
use retold_providers::config::ProviderConfig;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoginStatus {
    pub id: String,
    pub kind: String,
    pub profile_dir: Option<PathBuf>,
    pub credential_variable: Option<String>,
    pub status: String,
    /// Offline presence inspection cannot establish authentication.
    pub authentication_checked: bool,
    pub authenticated: bool,
    pub detail: String,
}

struct ProfileEvaluation {
    status: String,
    detail: String,
    authenticated: bool,
}

impl ProfileEvaluation {
    fn into_status(self, spec: ProfileSpec<'_>, dir: PathBuf) -> LoginStatus {
        LoginStatus {
            id: spec.id.to_owned(),
            kind: spec.kind.to_owned(),
            profile_dir: Some(dir),
            credential_variable: None,
            status: self.status,
            authentication_checked: false,
            authenticated: self.authenticated,
            detail: self.detail,
        }
    }
}

struct KeyEvaluation {
    status: String,
    detail: String,
    authenticated: bool,
}

impl KeyEvaluation {
    fn into_status(self, id: &str, key_env: &str) -> LoginStatus {
        LoginStatus {
            id: id.to_owned(),
            kind: "openrouter".to_owned(),
            profile_dir: None,
            credential_variable: Some(key_env.to_owned()),
            status: self.status,
            authentication_checked: false,
            authenticated: self.authenticated,
            detail: self.detail,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoginConfigured {
    pub id: String,
    pub kind: String,
    pub profile_dir: Option<PathBuf>,
    pub credential_variable: Option<String>,
    pub status: LoginStatus,
    pub updated_file: Option<PathBuf>,
    pub dry_run: bool,
}

struct ConfiguredDetails {
    kind: String,
    profile_dir: Option<PathBuf>,
    credential_variable: Option<String>,
}

struct ConfiguredOutcome {
    status: LoginStatus,
    updated_file: Option<PathBuf>,
}

impl ConfiguredOutcome {
    fn assemble(self, request: &LoginRequest<'_>, details: ConfiguredDetails) -> LoginConfigured {
        LoginConfigured {
            id: request.provider_id.to_owned(),
            kind: details.kind,
            profile_dir: details.profile_dir,
            credential_variable: details.credential_variable,
            status: self.status,
            updated_file: self.updated_file,
            dry_run: request.dry_run,
        }
    }
}

pub type LoginResult = LoginConfigured;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginRequest<'a> {
    pub root: &'a Path,
    pub provider_id: &'a str,
    pub profile_dir: Option<&'a Path>,
    pub key_env: Option<&'a str>,
    pub dry_run: bool,
}

impl<'a> LoginRequest<'a> {
    #[must_use]
    pub const fn new(root: &'a Path, provider_id: &'a str) -> Self {
        Self {
            root,
            provider_id,
            profile_dir: None,
            key_env: None,
            dry_run: false,
        }
    }
}

#[derive(Clone, Copy)]
struct ProfileSpec<'a> {
    id: &'a str,
    kind: &'a str,
    dir: Option<&'a PathBuf>,
    default_dirname: &'a str,
    environment: &'a str,
}

pub fn status(root: &Path, provider_id: &str) -> Result<LoginStatus, AddError> {
    inspect_status(root, provider_id)
}

pub fn configure(request: &LoginRequest<'_>) -> Result<LoginResult, AddError> {
    configure_login(request.root, request)
}

pub fn inspect_status(root: &Path, provider_id: &str) -> Result<LoginStatus, AddError> {
    let existing = provider_settings::load(root)?;
    let provider = existing
        .providers
        .get(provider_id)
        .ok_or(AddError::Invalid("Provider not found."))?;
    inspect_provider(provider, provider_id)
}

fn inspect_provider(provider: &ProviderConfig, provider_id: &str) -> Result<LoginStatus, AddError> {
    match provider {
        ProviderConfig::ClaudeCli { config_dir, .. } => {
            let spec = ProfileSpec {
                id: provider_id,
                kind: "claude_cli",
                dir: config_dir.as_ref(),
                default_dirname: ".claude",
                environment: "CLAUDE_CONFIG_DIR",
            };
            Ok(inspect_cli_profile(spec))
        }
        ProviderConfig::CodexCli { config_dir, .. } => {
            let spec = ProfileSpec {
                id: provider_id,
                kind: "codex_cli",
                dir: config_dir.as_ref(),
                default_dirname: ".codex",
                environment: "CODEX_HOME",
            };
            Ok(inspect_cli_profile(spec))
        }
        ProviderConfig::OpenRouter { key_env, .. } => Ok(inspect_openrouter(provider_id, key_env)),
        _ => Err(AddError::Invalid(
            "This provider does not use credential profiles or API keys.",
        )),
    }
}

pub fn configure_login(
    root: &Path,
    request: &LoginRequest<'_>,
) -> Result<LoginConfigured, AddError> {
    let original = load_target_provider(root, request.provider_id)?;
    let kind = provider_kind(&original).to_owned();
    let updated_provider = apply_configuration(&original, request)?;
    let (profile_dir, credential_variable) = extract_applied_spec(&updated_provider);
    let post_status = inspect_provider(&updated_provider, request.provider_id)?;
    let updated_file = save_login_update(root, request, updated_provider)?;

    let outcome = ConfiguredOutcome {
        status: post_status,
        updated_file,
    };
    let details = ConfiguredDetails {
        kind,
        profile_dir,
        credential_variable,
    };
    Ok(outcome.assemble(request, details))
}

fn load_target_provider(root: &Path, provider_id: &str) -> Result<ProviderConfig, AddError> {
    let original_config = provider_settings::load(root)?;
    original_config
        .providers
        .get(provider_id)
        .cloned()
        .ok_or(AddError::Invalid("Provider not found."))
}

fn save_login_update(
    root: &Path,
    request: &LoginRequest<'_>,
    updated: ProviderConfig,
) -> Result<Option<PathBuf>, AddError> {
    let is_dry = request.dry_run;
    if is_dry {
        let empty_path: Option<PathBuf> = None;
        return Ok(empty_path);
    }
    let edit = Edit {
        root,
        dry_run: request.dry_run,
    };
    let mutation = |original_bytes: Option<&[u8]>| {
        let mut config = provider_settings::existing(original_bytes)?;
        config
            .providers
            .insert(request.provider_id.to_owned(), updated);
        provider_settings::serialize(&config)
    };
    let backup = provider_settings::update(&edit, mutation)?;
    let fallback = root.join("providers.toml");
    Ok(backup.or(Some(fallback)))
}

const fn provider_kind(provider: &ProviderConfig) -> &'static str {
    match provider {
        ProviderConfig::ClaudeCli { .. } => "claude_cli",
        ProviderConfig::CodexCli { .. } => "codex_cli",
        ProviderConfig::OpenRouter { .. } => "openrouter",
        ProviderConfig::LocalText { .. } => "local_text",
        ProviderConfig::GeminiCli { .. } => "gemini_cli",
        ProviderConfig::LocalVoice { .. } => "local_voice",
        ProviderConfig::LocalWhisper { .. } => "local_whisper",
    }
}

fn extract_applied_spec(provider: &ProviderConfig) -> (Option<PathBuf>, Option<String>) {
    match provider {
        ProviderConfig::ClaudeCli { config_dir, .. }
        | ProviderConfig::CodexCli { config_dir, .. } => (config_dir.clone(), None),
        ProviderConfig::OpenRouter { key_env, .. } => (None, Some(key_env.clone())),
        _ => (None, None),
    }
}

fn inspect_cli_profile(spec: ProfileSpec<'_>) -> LoginStatus {
    let target_dir = resolve_profile_dir(spec);
    let evaluation = evaluate_profile(&target_dir, spec.kind);
    evaluation.into_status(spec, target_dir)
}

fn evaluate_profile(dir: &Path, kind: &str) -> ProfileEvaluation {
    let is_dir = dir.is_dir();
    if is_dir {
        let display_path = dir.display();
        ProfileEvaluation {
            status: "profile_found".to_owned(),
            detail: format!(
                "{kind} profile directory exists at {display_path}. This offline inspection does not verify authentication."
            ),
            authenticated: false,
        }
    } else {
        ProfileEvaluation {
            status: "missing_profile".to_owned(),
            detail: format!("{kind} profile directory is missing or inaccessible."),
            authenticated: false,
        }
    }
}

fn resolve_profile_dir(spec: ProfileSpec<'_>) -> PathBuf {
    let env_home = std::env::var_os("HOME");
    spec.dir
        .cloned()
        .or_else(|| std::env::var_os(spec.environment).map(PathBuf::from))
        .or_else(|| env_home.map(|home| PathBuf::from(home).join(spec.default_dirname)))
        .unwrap_or_else(|| PathBuf::from(spec.default_dirname))
}

fn inspect_openrouter(id: &str, key_env: &str) -> LoginStatus {
    let evaluation = evaluate_openrouter(key_env);
    evaluation.into_status(id, key_env)
}

fn evaluate_openrouter(key_env: &str) -> KeyEvaluation {
    let is_set = std::env::var_os(key_env).is_some_and(|env_value| !env_value.is_empty());
    if is_set {
        KeyEvaluation {
            status: "configured".to_owned(),
            detail: format!(
                "Environment variable {key_env} is set. This offline inspection does not verify authentication."
            ),
            authenticated: false,
        }
    } else {
        KeyEvaluation {
            status: "missing_credential".to_owned(),
            detail: format!("Environment variable {key_env} is not set or empty."),
            authenticated: false,
        }
    }
}

fn apply_claude_profile(provider: &ProviderConfig, dir: PathBuf) -> Option<ProviderConfig> {
    if let ProviderConfig::ClaudeCli {
        binary,
        tiers,
        max_parallel,
        timeout_secs,
        ..
    } = provider
    {
        Some(ProviderConfig::ClaudeCli {
            binary: binary.clone(),
            config_dir: Some(dir),
            tiers: tiers.clone(),
            max_parallel: *max_parallel,
            timeout_secs: *timeout_secs,
        })
    } else {
        None
    }
}

fn apply_codex_profile(provider: &ProviderConfig, dir: PathBuf) -> Option<ProviderConfig> {
    if let ProviderConfig::CodexCli {
        binary,
        tiers,
        max_parallel,
        timeout_secs,
        ..
    } = provider
    {
        Some(ProviderConfig::CodexCli {
            binary: binary.clone(),
            config_dir: Some(dir),
            tiers: tiers.clone(),
            max_parallel: *max_parallel,
            timeout_secs: *timeout_secs,
        })
    } else {
        None
    }
}

fn apply_cli_profile(provider: &ProviderConfig, dir: PathBuf) -> Result<ProviderConfig, AddError> {
    if let Some(claude) = apply_claude_profile(provider, dir.clone()) {
        return Ok(claude);
    }
    if let Some(codex) = apply_codex_profile(provider, dir) {
        return Ok(codex);
    }
    Err(AddError::Invalid("Provider is not a CLI profile provider."))
}

fn apply_openrouter_key(
    provider: &ProviderConfig,
    key_env: String,
) -> Result<ProviderConfig, AddError> {
    match provider {
        ProviderConfig::OpenRouter {
            max_parallel,
            base_url,
            timeout_secs,
            ..
        } => Ok(ProviderConfig::OpenRouter {
            key_env,
            max_parallel: *max_parallel,
            base_url: base_url.clone(),
            timeout_secs: *timeout_secs,
        }),
        _ => Err(AddError::Invalid("Provider is not an API key provider.")),
    }
}

fn apply_configuration(
    provider: &ProviderConfig,
    request: &LoginRequest<'_>,
) -> Result<ProviderConfig, AddError> {
    let has_profile = request.profile_dir.is_some();
    if has_profile {
        let dir = extract_profile_dir(request)?;
        return apply_cli_profile(provider, dir);
    }
    let has_key = request.key_env.is_some();
    if has_key {
        let key_env = extract_key_env(request)?;
        return apply_openrouter_key(provider, key_env);
    }
    Err(AddError::Invalid(
        "Specify --profile-dir or --key-env to configure credentials.",
    ))
}

fn extract_profile_dir(request: &LoginRequest<'_>) -> Result<PathBuf, AddError> {
    let dir = request.profile_dir.ok_or(AddError::Invalid(
        "Supply --profile-dir to configure profile location for this CLI provider.",
    ))?;
    let is_dir = dir.is_dir();
    if !is_dir {
        return Err(AddError::Invalid("Specified profile directory not found."));
    }
    Ok(dir.to_path_buf())
}

fn extract_key_env(request: &LoginRequest<'_>) -> Result<String, AddError> {
    let key_var = request.key_env.ok_or(AddError::Invalid(
        "Supply --key-env to configure credential environment variable.",
    ))?;
    let is_empty = key_var.trim().is_empty();
    if is_empty {
        return Err(AddError::Invalid(
            "Environment variable name cannot be empty.",
        ));
    }
    Ok(key_var.to_owned())
}
