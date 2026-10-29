//! Route suggestions from the providers a person configured, whatever they named them. Only
//! tiers without a route are filled, subscription CLIs come before anything paid, and only
//! selectors the provider defines are used. Paid models, voices and transcription stay a
//! person's choice, so they are never suggested.
use indexmap::IndexMap;
use retold_core::Tier;

use crate::config::{ProviderConfig, ProvidersConfig, RouteSpec};

/// The selector Codex image generation expects.
const CODEX_IMAGE_SELECTOR: &str = "imagegen";
/// The selector CLI providers treat as their own default model.
const DEFAULT_SELECTOR: &str = "default";

/// Ordered chains for every tier that has no route yet and at least one suitable provider.
#[must_use]
pub fn suggested_routes(config: &ProvidersConfig) -> IndexMap<Tier, Vec<RouteSpec>> {
    Tier::ALL
        .into_iter()
        .filter(|tier| !config.routing.contains_key(tier))
        .map(|tier| (tier, chain(config, tier)))
        .filter(|(_, chain)| !chain.is_empty())
        .collect()
}

fn chain(config: &ProvidersConfig, tier: Tier) -> Vec<RouteSpec> {
    let mut ranked: Vec<(u8, RouteSpec)> = config
        .providers
        .iter()
        .filter_map(|(id, provider)| {
            let rank = rank(provider, tier)?;
            let selector = selector(provider, tier)?;
            Some((rank, RouteSpec::new(id.clone(), selector)))
        })
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().map(|(_, route)| route).collect()
}

/// What kind of work a tier asks for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Text,
    LocalText,
    Vision,
    Image,
    /// Voice and transcription: models are always a person's choice.
    Other,
}

fn family(tier: Tier) -> Family {
    let name = tier.as_str();
    let starts = |prefix: &str| name.starts_with(prefix);
    match (tier, starts("text"), starts("vision"), starts("image")) {
        (Tier::TextLocal, ..) => Family::LocalText,
        (_, true, _, _) => Family::Text,
        (_, _, true, _) => Family::Vision,
        (_, _, _, true) => Family::Image,
        _ => Family::Other,
    }
}

/// Lower ranks are tried first; `None` means this kind is never suggested for the tier.
fn rank(provider: &ProviderConfig, tier: Tier) -> Option<u8> {
    match (provider, family(tier)) {
        (ProviderConfig::LocalText { .. }, Family::LocalText)
        | (ProviderConfig::ClaudeCli { .. }, Family::Text)
        | (ProviderConfig::CodexCli { .. }, Family::Vision | Family::Image) => Some(0),
        (ProviderConfig::ClaudeCli { .. }, Family::Vision)
        | (ProviderConfig::CodexCli { .. }, Family::Text) => Some(1),
        (ProviderConfig::GeminiCli { .. }, Family::Text) => Some(2),
        _ => Option::default(),
    }
}

/// `smart` for `text.smart`, `fast` for `vision.fast`.
fn quality(tier: Tier) -> &'static str {
    tier.as_str().rsplit('.').next().unwrap_or_default()
}

fn selector(provider: &ProviderConfig, tier: Tier) -> Option<String> {
    let wanted = quality(tier);
    let image = family(tier) == Family::Image;
    match provider {
        ProviderConfig::ClaudeCli { tiers, .. } => named_tier(tiers, wanted),
        ProviderConfig::CodexCli { .. } if image => Some(CODEX_IMAGE_SELECTOR.to_owned()),
        ProviderConfig::CodexCli { tiers, .. } | ProviderConfig::GeminiCli { tiers, .. } => {
            Some(named_tier(tiers, wanted).unwrap_or_else(|| DEFAULT_SELECTOR.to_owned()))
        }
        ProviderConfig::LocalText { models, .. } => models.keys().next().cloned(),
        ProviderConfig::OpenRouter { .. }
        | ProviderConfig::LocalVoice { .. }
        | ProviderConfig::LocalWhisper { .. } => Option::default(),
    }
}

/// For each quality, the nearest others to fall back to, closest first.
const NEAREST_QUALITIES: [(&str, [&str; 2]); 3] = [
    ("best", ["smart", "fast"]),
    ("smart", ["best", "fast"]),
    ("fast", ["smart", "best"]),
];

/// The tier named like `wanted`, else the nearest quality the provider names, else its first
/// named tier; Claude has no default model, so it may name none.
fn named_tier(tiers: &IndexMap<String, String>, wanted: &str) -> Option<String> {
    let nearest = NEAREST_QUALITIES
        .iter()
        .find(|(quality, _)| *quality == wanted)
        .map(|(_, others)| *others)
        .unwrap_or_default();
    std::iter::once(wanted)
        .chain(nearest)
        .find(|name| tiers.contains_key(*name))
        .map(str::to_owned)
        .or_else(|| tiers.keys().next().cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(text: &str) -> ProvidersConfig {
        toml::from_str(text).unwrap()
    }

    const MIXED: &str = r#"
        [providers.work]
        kind = "claude_cli"
        max_parallel = 1
        tiers = { fast = "haiku", smart = "sonnet" }
        [providers.mine]
        kind = "codex_cli"
        max_parallel = 1
        [providers.paid]
        kind = "openrouter"
        key_env = "KEY"
        max_parallel = 1
        [routing]
    "#;

    fn chain_of(routes: &IndexMap<Tier, Vec<RouteSpec>>, tier: Tier) -> Vec<String> {
        routes[&tier].iter().map(ToString::to_string).collect()
    }

    #[test]
    fn suggests_cli_chains_by_kind_for_any_provider_names() {
        let routes = suggested_routes(&config(MIXED));
        assert_eq!(
            chain_of(&routes, Tier::TextSmart),
            ["work:smart", "mine:default"]
        );
        assert_eq!(
            chain_of(&routes, Tier::TextBest),
            ["work:smart", "mine:default"]
        );
        assert_eq!(chain_of(&routes, Tier::Image), ["mine:imagegen"]);
        assert!(!routes.contains_key(&Tier::Tts));
        assert!(!routes.contains_key(&Tier::TextLocal));
    }

    #[test]
    fn existing_routes_are_never_replaced() {
        let config = config(
            r#"
            [providers.work]
            kind = "codex_cli"
            max_parallel = 1
            [routing]
            "text.smart" = ["work:custom"]
        "#,
        );
        let routes = suggested_routes(&config);
        assert!(!routes.contains_key(&Tier::TextSmart));
        assert!(routes.contains_key(&Tier::TextFast));
    }

    #[test]
    fn claude_without_named_tiers_is_left_out() {
        let config = config(
            r#"
            [providers.bare]
            kind = "claude_cli"
            max_parallel = 1
            tiers = {}
            [routing]
        "#,
        );
        assert!(suggested_routes(&config).is_empty());
    }
}
