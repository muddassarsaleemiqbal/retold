//! A safe projection of configured providers, without constructing adapters or loading secrets.
use std::path::Path;

use retold_core::Tier;
use retold_providers::config::ProviderConfig;
use serde::Serialize;

use crate::provider_settings::{self, AddError};

#[derive(Debug, Serialize)]
pub struct ProviderSummary {
    pub id: String,
    pub kind: &'static str,
    pub max_parallel: usize,
    pub supported_tiers: Vec<Tier>,
    pub routes: Vec<Assignment>,
    pub health: &'static str,
    pub authentication: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Assignment {
    pub tier: Tier,
    /// One-based position in the configured fallback chain; no selectors are exposed.
    pub position: usize,
}

pub fn list(root: &Path) -> Result<Vec<ProviderSummary>, AddError> {
    let config = provider_settings::load_inventory(root)?;
    Ok(project(&config))
}

pub(crate) fn project(config: &retold_providers::ProvidersConfig) -> Vec<ProviderSummary> {
    config
        .providers
        .iter()
        .map(|(id, provider)| {
            let supported_tiers = Tier::ALL
                .into_iter()
                .filter(|tier| provider.supports_tier(*tier))
                .collect();
            ProviderSummary {
                id: id.clone(),
                kind: kind(provider),
                max_parallel: provider.max_parallel(),
                supported_tiers,
                routes: assignments(config, id),
                health: "not_checked",
                authentication: "not_checked",
            }
        })
        .collect()
}

fn assignments(config: &retold_providers::ProvidersConfig, id: &str) -> Vec<Assignment> {
    let mut assignments = Vec::new();
    for (tier, chain) in &config.routing {
        for (index, route) in chain.iter().enumerate() {
            if route.provider == id {
                assignments.push(Assignment {
                    tier: *tier,
                    position: index + 1,
                });
            }
        }
    }
    assignments
}

const fn kind(provider: &ProviderConfig) -> &'static str {
    match provider {
        ProviderConfig::LocalText { .. } => "local_text",
        ProviderConfig::ClaudeCli { .. } => "claude_cli",
        ProviderConfig::CodexCli { .. } => "codex_cli",
        ProviderConfig::GeminiCli { .. } => "gemini_cli",
        ProviderConfig::LocalVoice { .. } => "local_voice",
        ProviderConfig::LocalWhisper { .. } => "local_whisper",
        ProviderConfig::OpenRouter { .. } => "openrouter",
    }
}
