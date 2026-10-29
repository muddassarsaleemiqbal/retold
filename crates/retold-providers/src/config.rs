//! `providers.toml`: which providers exist and which chain each tier uses.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use indexmap::IndexMap;
use retold_core::Tier;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The `providers.toml` format this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

const fn current_schema() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvidersConfig {
    /// Format version of this file. Absent means 1; newer files are refused, not misread.
    #[serde(default = "current_schema")]
    pub schema_version: u32,
    pub providers: IndexMap<String, ProviderConfig>,
    /// Tier → providers to try, in order.
    pub routing: IndexMap<Tier, Vec<RouteSpec>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderConfig {
    /// Offline llama.cpp completion using explicitly installed GGUF files.
    LocalText {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        binary: Option<PathBuf>,
        models: IndexMap<String, PathBuf>,
        max_parallel: usize,
        #[serde(default = "default_timeout")]
        timeout_secs: u64,
        #[serde(default = "default_text_tokens")]
        max_tokens: u32,
        #[serde(default = "default_text_context")]
        context_size: u32,
    },
    ClaudeCli {
        /// Path to `claude`; found on `PATH` when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        binary: Option<PathBuf>,
        /// A separate Claude login (`CLAUDE_CONFIG_DIR`). Add one provider per account and
        /// list them in a route; the router fails over when one hits its limit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        config_dir: Option<PathBuf>,
        /// Selector → model, e.g. `smart = "sonnet"`.
        tiers: IndexMap<String, String>,
        max_parallel: usize,
        #[serde(default = "default_timeout")]
        timeout_secs: u64,
    },
    CodexCli {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        binary: Option<PathBuf>,
        /// A separate Codex login (`CODEX_HOME`), as for `claude_cli.config_dir`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        config_dir: Option<PathBuf>,
        #[serde(default)]
        tiers: IndexMap<String, String>,
        max_parallel: usize,
        #[serde(default = "default_timeout")]
        timeout_secs: u64,
    },
    GeminiCli {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        binary: Option<PathBuf>,
        #[serde(default)]
        tiers: IndexMap<String, String>,
        max_parallel: usize,
        #[serde(default = "default_timeout")]
        timeout_secs: u64,
    },
    LocalVoice {
        #[serde(default = "default_espeak")]
        espeak: PathBuf,
        max_parallel: usize,
        #[serde(default = "default_timeout")]
        timeout_secs: u64,
    },
    LocalWhisper {
        binary: PathBuf,
        model: PathBuf,
        ffmpeg: PathBuf,
        max_parallel: usize,
        #[serde(default = "default_timeout")]
        timeout_secs: u64,
    },
    #[serde(rename = "openrouter")]
    OpenRouter {
        /// Environment variable holding the key (also read from `.env`).
        key_env: String,
        max_parallel: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_url: Option<String>,
        #[serde(default = "default_timeout")]
        timeout_secs: u64,
    },
}

fn default_espeak() -> PathBuf {
    "espeak-ng".into()
}

const fn default_text_tokens() -> u32 {
    2048
}
const fn default_text_context() -> u32 {
    8192
}

/// Seconds a provider call may take when `timeout_secs` is not set.
pub const DEFAULT_TIMEOUT_SECS: u64 = 600;

const fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

impl ProviderConfig {
    #[must_use]
    pub const fn max_parallel(&self) -> usize {
        match self {
            Self::LocalText { max_parallel, .. }
            | Self::ClaudeCli { max_parallel, .. }
            | Self::CodexCli { max_parallel, .. }
            | Self::GeminiCli { max_parallel, .. }
            | Self::LocalWhisper { max_parallel, .. }
            | Self::LocalVoice { max_parallel, .. }
            | Self::OpenRouter { max_parallel, .. } => *max_parallel,
        }
    }

    /// Adapter-level support only; does not prove that a selected remote model is capable.
    #[must_use]
    pub fn supports_tier(&self, tier: Tier) -> bool {
        if tier == Tier::TextLocal {
            return matches!(self, Self::LocalText { .. });
        }
        self.supports_capability(tier.capability())
    }

    const fn supports_capability(&self, capability: retold_core::Capability) -> bool {
        use retold_core::Capability;
        match capability {
            Capability::Text => matches!(
                self,
                Self::LocalText { .. }
                    | Self::ClaudeCli { .. }
                    | Self::CodexCli { .. }
                    | Self::GeminiCli { .. }
                    | Self::OpenRouter { .. }
            ),
            Capability::Vision => matches!(
                self,
                Self::ClaudeCli { .. } | Self::CodexCli { .. } | Self::OpenRouter { .. }
            ),
            other => self.supports_media(other),
        }
    }

    const fn supports_media(&self, capability: retold_core::Capability) -> bool {
        use retold_core::Capability;
        matches!(
            (self, capability),
            (
                Self::CodexCli { .. } | Self::OpenRouter { .. },
                Capability::Image
            ) | (
                Self::LocalVoice { .. } | Self::OpenRouter { .. },
                Capability::Tts
            ) | (
                Self::LocalWhisper { .. } | Self::OpenRouter { .. },
                Capability::Transcribe
            )
        )
    }

    /// Environment selecting this provider's login, if it uses a non-default one.
    #[must_use]
    pub fn login_env(&self) -> Vec<(String, String)> {
        let var = match self {
            Self::ClaudeCli {
                config_dir: Some(dir),
                ..
            } => ("CLAUDE_CONFIG_DIR", dir),
            Self::CodexCli {
                config_dir: Some(dir),
                ..
            } => ("CODEX_HOME", dir),
            _ => return Vec::new(),
        };
        vec![(var.0.to_owned(), var.1.display().to_string())]
    }

    /// Short description for `doctor` and `providers` listings.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::LocalText { .. } => "Local Text (llama.cpp)",
            Self::ClaudeCli { .. } => "Claude Code CLI",
            Self::CodexCli { .. } => "Codex CLI",
            Self::OpenRouter { .. } => "OpenRouter",
            Self::GeminiCli { .. } => "Gemini CLI",
            Self::LocalWhisper { .. } => "Local Whisper",
            Self::LocalVoice { .. } => "Local Voice",
        }
    }
}

/// `provider:selector`, e.g. `claude-code:smart` or `openrouter:anthropic/claude-haiku-5.5`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteSpec {
    pub provider: String,
    pub selector: String,
}

impl RouteSpec {
    #[must_use]
    pub fn new(provider: impl Into<String>, selector: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            selector: selector.into(),
        }
    }
}

impl fmt::Display for RouteSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.provider, self.selector)
    }
}

impl FromStr for RouteSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once(':') {
            Some((p, sel)) if !p.is_empty() && !sel.is_empty() => Ok(Self {
                provider: p.to_owned(),
                selector: sel.to_owned(),
            }),
            _ => Err(format!(
                "`{s}` should look like `provider:model`, e.g. `claude-code:smart`"
            )),
        }
    }
}

impl Serialize for RouteSpec {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for RouteSpec {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Information from the environment about what is installed.
#[derive(Debug, Clone, Copy, Default)]
pub struct Found {
    pub claude: bool,
    pub codex: bool,
}

impl ProvidersConfig {
    /// No providers and no routes: what a fresh installation starts with.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            providers: IndexMap::new(),
            routing: IndexMap::new(),
        }
    }

    /// A sensible starting configuration built from what is available.
    #[must_use]
    pub fn suggested(found: Found) -> Self {
        let mut providers = IndexMap::new();
        if found.claude {
            providers.insert(
                "claude-code".to_owned(),
                ProviderConfig::ClaudeCli {
                    binary: None,
                    config_dir: None,
                    tiers: [("fast", "haiku"), ("smart", "sonnet"), ("best", "opus")]
                        .into_iter()
                        .map(|(k, v)| (k.to_owned(), v.to_owned()))
                        .collect(),
                    max_parallel: 3,
                    timeout_secs: default_timeout(),
                },
            );
        }
        if found.codex {
            providers.insert(
                "codex".to_owned(),
                ProviderConfig::CodexCli {
                    binary: None,
                    config_dir: None,
                    tiers: IndexMap::new(),
                    max_parallel: 2,
                    timeout_secs: default_timeout(),
                },
            );
        }
        providers.insert(
            "openrouter".to_owned(),
            ProviderConfig::OpenRouter {
                key_env: "OPENROUTER_API_KEY".to_owned(),
                max_parallel: 8,
                base_url: None,
                timeout_secs: default_timeout(),
            },
        );

        let routing = build_suggested_routing(&providers);
        Self {
            schema_version: SCHEMA_VERSION,
            providers,
            routing,
        }
    }

    /// Problems that make a route unusable (unknown provider ids).
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        for (tier, chain) in &self.routing {
            if chain.is_empty() {
                p.push(format!("routing.\"{tier}\" is empty"));
            }
            for r in chain {
                if !self.providers.contains_key(&r.provider) {
                    p.push(format!(
                        "routing.\"{tier}\": `{}` is not a configured provider",
                        r.provider
                    ));
                }
            }
        }
        for (id, cfg) in &self.providers {
            if cfg.max_parallel() == 0 {
                p.push(format!("providers.{id}.max_parallel must be at least 1"));
            }
        }
        p
    }

    /// The file text, with a header explaining it.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        let body = toml::to_string_pretty(self)?;
        Ok(format!(
            "# Retold providers. Written by `retold providers` and `retold setup`; safe to edit.\n\
             # Each tier tries its providers in order and falls back on rate limits or errors.\n\
             # Paid providers (openrouter) are only used within a run's explicit --spend.\n\n{body}"
        ))
    }
}

fn build_suggested_routing(
    providers: &IndexMap<String, ProviderConfig>,
) -> IndexMap<Tier, Vec<RouteSpec>> {
    let route = |s: &str| s.parse::<RouteSpec>().ok();
    let chain = |items: &[&str]| -> Vec<RouteSpec> {
        items
            .iter()
            .filter_map(|s| route(s))
            .filter(|r| providers.contains_key(&r.provider))
            .collect()
    };
    let mut routing = IndexMap::new();
    routing.insert(
        Tier::TextSmart,
        chain(&[
            "claude-code:smart",
            "codex:default",
            "openrouter:anthropic/claude-sonnet-5.5",
        ]),
    );
    routing.insert(
        Tier::TextFast,
        chain(&[
            "claude-code:fast",
            "codex:default",
            "openrouter:anthropic/claude-haiku-5.5",
        ]),
    );
    routing.insert(
        Tier::TextBest,
        chain(&[
            "claude-code:best",
            "codex:default",
            "openrouter:anthropic/claude-opus-5.5",
        ]),
    );
    routing.insert(
        Tier::VisionFast,
        chain(&[
            "codex:default",
            "claude-code:fast",
            "openrouter:anthropic/claude-haiku-5.5",
        ]),
    );
    routing.insert(
        Tier::VisionSmart,
        chain(&[
            "codex:default",
            "claude-code:smart",
            "openrouter:anthropic/claude-sonnet-5.5",
        ]),
    );
    routing.insert(
        Tier::Image,
        chain(&[
            "codex:imagegen",
            "openrouter:bytedance-seed/seedream-5-0-flash@2K",
        ]),
    );
    routing.insert(
        Tier::ImageNoRefs,
        chain(&[
            "codex:imagegen",
            "openrouter:bytedance-seed/seedream-5-0-flash@2K",
        ]),
    );
    routing.insert(
        Tier::Tts,
        chain(&[
            "openrouter:elevenlabs/eleven-v4",
            "openrouter:openai/gpt-4o-mini-tts",
        ]),
    );
    routing.insert(
        Tier::Transcribe,
        chain(&["openrouter:openai/whisper-large-v3"]),
    );
    routing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_text_configuration_round_trips_without_replacing_existing_routes() {
        let cfg: ProvidersConfig = toml::from_str(
            r#"
            [providers.offline]
            kind = "local_text"
            models = { metadata = "/models/writer.gguf" }
            max_parallel = 1
            [routing]
            "text.local" = ["offline:metadata"]
            "text.fast" = ["codex:default"]
        "#,
        )
        .unwrap();
        assert_eq!(
            cfg.providers["offline"].kind_name(),
            "Local Text (llama.cpp)"
        );
        let ProviderConfig::LocalText {
            max_tokens,
            context_size,
            ..
        } = &cfg.providers["offline"]
        else {
            panic!("wrong provider")
        };
        assert_eq!((*max_tokens, *context_size), (2048, 8192));
        let restored: ProvidersConfig = toml::from_str(&cfg.to_toml().unwrap()).unwrap();
        assert_eq!(restored.routing[&Tier::TextLocal][0].selector, "metadata");
        assert_eq!(restored.routing[&Tier::TextFast][0].provider, "codex");
    }

    #[test]
    fn second_logins_select_their_config_dir() {
        let cfg: ProvidersConfig = toml::from_str(
            r#"
            [providers.claude-work]
            kind = "claude_cli"
            config_dir = "/Users/me/.claude-work"
            tiers = { smart = "sonnet" }
            max_parallel = 2

            [providers.codex-second]
            kind = "codex_cli"
            config_dir = "/Users/me/.codex-second"
            max_parallel = 1

            [routing]
            "#,
        )
        .unwrap();
        assert_eq!(
            cfg.providers["claude-work"].login_env(),
            vec![(
                "CLAUDE_CONFIG_DIR".to_owned(),
                "/Users/me/.claude-work".to_owned()
            )]
        );
        assert_eq!(
            cfg.providers["codex-second"].login_env(),
            vec![(
                "CODEX_HOME".to_owned(),
                "/Users/me/.codex-second".to_owned()
            )]
        );
    }

    #[test]
    fn suggested_config_round_trips_and_is_valid() {
        let cfg = ProvidersConfig::suggested(Found {
            claude: true,
            codex: true,
        });
        assert!(cfg.problems().is_empty(), "{:?}", cfg.problems());
        let text = cfg.to_toml().unwrap();
        let back: ProvidersConfig = toml::from_str(&text).unwrap();
        assert_eq!(
            back.routing[&Tier::TextSmart][0].to_string(),
            "claude-code:smart"
        );
    }

    #[test]
    fn missing_clis_are_left_out_of_routes() {
        let cfg = ProvidersConfig::suggested(Found::default());
        assert_eq!(cfg.problems(), [] as [std::string::String; 0]);
        assert_eq!(cfg.routing[&Tier::TextSmart].len(), 1);
        assert_eq!(cfg.routing[&Tier::TextSmart][0].provider, "openrouter");
    }

    #[test]
    fn route_spec_keeps_colons_in_model() {
        let r: RouteSpec = "openrouter:x/y:free".parse().unwrap();
        assert_eq!(
            (r.provider.as_str(), r.selector.as_str()),
            ("openrouter", "x/y:free")
        );
        assert!("nocolon".parse::<RouteSpec>().is_err());
    }
}
