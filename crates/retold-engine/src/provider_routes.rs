//! Explicit route updates reuse the provider-file writer transaction; no adapter is built.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use retold_core::Tier;
use retold_providers::{ProvidersConfig, RouteSpec};
use serde::Serialize;

use crate::provider_settings::{self, AddError, Edit};

const MAX_ROUTE_ENTRIES: usize = 32;
const MAX_ORDER_BYTES: usize = 8192;

#[derive(Debug)]
pub struct RouteRequest<'a> {
    pub root: &'a Path,
    pub tier: &'a str,
    pub order: &'a str,
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
pub struct RouteUpdate {
    pub tier: Tier,
    pub order: Vec<String>,
    pub dry_run: bool,
    pub backup: Option<PathBuf>,
}

/// Task tiers accepted by the route writer, including unconfigured ones.
#[must_use]
pub fn tiers() -> Vec<&'static str> {
    Tier::ALL.into_iter().map(Tier::as_str).collect()
}

pub fn show(root: &Path) -> Result<BTreeMap<String, Vec<String>>, AddError> {
    let config = provider_settings::load(root)?;
    Ok(config
        .routing
        .iter()
        .map(|(tier, chain)| {
            (
                tier.to_string(),
                chain.iter().map(ToString::to_string).collect(),
            )
        })
        .collect())
}

pub fn set(request: &RouteRequest<'_>) -> Result<RouteUpdate, AddError> {
    let tier = request
        .tier
        .parse::<Tier>()
        .map_err(|_| AddError::Invalid("Unknown routing tier."))?;
    let chain = parse_order(request.order)?;
    let order = chain.iter().map(ToString::to_string).collect();
    let edit = Edit {
        root: request.root,
        dry_run: request.dry_run,
    };
    let backup = provider_settings::update(&edit, |original| {
        let config = provider_settings::existing(original)?;
        let config = replace(config, tier, chain)?;
        provider_settings::serialize(&config)
    })?;
    Ok(RouteUpdate {
        tier,
        order,
        dry_run: request.dry_run,
        backup,
    })
}

/// Routes added for tiers that had none, as `provider:selector` lists by tier.
#[derive(Debug, Serialize)]
pub struct Suggested {
    pub routes: BTreeMap<String, Vec<String>>,
    pub dry_run: bool,
    pub backup: Option<PathBuf>,
}

/// Which configuration to fill, and whether to only preview the result.
#[derive(Debug, Clone, Copy)]
pub struct SuggestRequest<'a> {
    pub root: &'a Path,
    pub dry_run: bool,
}

/// Fills every unrouted tier with a chain built from the configured providers' kinds.
/// Existing routes are kept exactly; a dry run shows the chains without saving them.
pub fn suggest(request: SuggestRequest<'_>) -> Result<Suggested, AddError> {
    let SuggestRequest { root, dry_run } = request;
    let mut routes = BTreeMap::new();
    let edit = Edit { root, dry_run };
    let backup = provider_settings::update(&edit, |original| {
        let mut config = provider_settings::existing(original)?;
        let suggested = retold_providers::suggest::suggested_routes(&config);
        routes = listed(&suggested);
        config.routing.extend(suggested);
        provider_settings::serialize(&config)
    })?;
    Ok(Suggested {
        routes,
        dry_run,
        backup,
    })
}

fn listed<'a>(
    routes: impl IntoIterator<Item = (&'a Tier, &'a Vec<RouteSpec>)>,
) -> BTreeMap<String, Vec<String>> {
    routes
        .into_iter()
        .map(|(tier, chain)| {
            (
                tier.to_string(),
                chain.iter().map(ToString::to_string).collect(),
            )
        })
        .collect()
}

pub(crate) fn parse_order(order: &str) -> Result<Vec<RouteSpec>, AddError> {
    let entries = route_entries(order)?;
    let mut unique = BTreeSet::new();
    entries
        .into_iter()
        .map(|entry| {
            let invalid = entry.is_empty()
                || entry.chars().any(char::is_whitespace)
                || entry.chars().any(char::is_control)
                || !unique.insert(entry);
            if invalid {
                return Err(AddError::Invalid(
                    "Route entries must be unique, nonempty provider:selector pairs.",
                ));
            }
            entry
                .parse()
                .map_err(|_| AddError::Invalid("Use provider:selector for every route entry."))
        })
        .collect()
}

pub(crate) fn replace(
    mut config: ProvidersConfig,
    tier: Tier,
    chain: Vec<RouteSpec>,
) -> Result<ProvidersConfig, AddError> {
    for route in &chain {
        let provider = config
            .providers
            .get(&route.provider)
            .ok_or(AddError::Invalid("Route references an unknown provider."))?;
        if !provider.supports_tier(tier) {
            return Err(AddError::Invalid(
                "Provider adapter does not support this tier; text.local accepts only local text providers.",
            ));
        }
    }
    config.routing.insert(tier, chain);
    Ok(config)
}

fn route_entries(order: &str) -> Result<Vec<&str>, AddError> {
    validate_order_size(order)?;
    let entries: Vec<_> = order.split(',').map(str::trim).collect();
    if entries.len() > MAX_ROUTE_ENTRIES {
        return Err(AddError::Invalid("A route may have at most 32 entries."));
    }
    Ok(entries)
}

const fn validate_order_size(order: &str) -> Result<(), AddError> {
    if order.len() > MAX_ORDER_BYTES {
        return Err(AddError::Invalid(
            "Route order exceeds the input size limit.",
        ));
    }
    Ok(())
}
