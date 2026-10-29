//! Subscription usage that a CLI already caches locally. Read-only and offline: nothing here
//! makes a network call or touches login tokens.
//!
//! Claude Code keeps its last-fetched rolling-window utilization in `.claude.json`
//! (`cachedUsageUtilization.utilization.{five_hour,seven_day}`). The values are whole percentages
//! as of `fetchedAtMs`, so they can lag; `doctor` labels them as such.

use std::path::{Path, PathBuf};

use retold_core::num;
use serde::Deserialize;

/// One rolling usage window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    /// Percent of the window used, 0–100.
    pub used_percent: u8,
    /// When the window resets, as Claude reported it (RFC 3339).
    pub resets_at: Option<String>,
}

/// Claude Code's cached subscription usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeUsage {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    /// Milliseconds since the Unix epoch when Claude fetched these numbers.
    pub fetched_at_ms: Option<u64>,
}

impl ClaudeUsage {
    /// Whether any window is fully used, so routing to this login will fail until it resets.
    #[must_use]
    pub fn exhausted(&self) -> bool {
        [&self.five_hour, &self.seven_day]
            .into_iter()
            .flatten()
            .any(|w| w.used_percent >= 100)
    }
}

#[derive(Deserialize)]
struct State {
    #[serde(rename = "cachedUsageUtilization")]
    cached: Option<Cached>,
}

#[derive(Deserialize)]
struct Cached {
    #[serde(rename = "fetchedAtMs")]
    fetched_at_ms: Option<u64>,
    utilization: Option<Utilization>,
}

#[derive(Deserialize)]
struct Utilization {
    five_hour: Option<RawWindow>,
    seven_day: Option<RawWindow>,
}

#[derive(Deserialize)]
struct RawWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

impl RawWindow {
    fn window(self) -> Option<Window> {
        let used = self.utilization?;
        used.is_finite().then(|| Window {
            used_percent: num::trunc_u8(used.clamp(0.0, 100.0).round()),
            resets_at: self.resets_at,
        })
    }
}

/// Reads cached usage for the Claude login in `config_dir` (`CLAUDE_CONFIG_DIR`), or the
/// default login when `None`. Returns `None` when Claude has not cached any usage yet.
#[must_use]
pub fn claude_usage(config_dir: Option<&Path>) -> Option<ClaudeUsage> {
    let bytes = std::fs::read(state_file(config_dir)?).ok()?;
    parse(&bytes)
}

fn state_file(config_dir: Option<&Path>) -> Option<PathBuf> {
    match config_dir {
        Some(dir) => Some(dir.join(".claude.json")),
        None => directories::BaseDirs::new().map(|b| b.home_dir().join(".claude.json")),
    }
}

fn parse(bytes: &[u8]) -> Option<ClaudeUsage> {
    let cached = serde_json::from_slice::<State>(bytes).ok()?.cached?;
    let utilization = cached.utilization?;
    Some(ClaudeUsage {
        five_hour: utilization.five_hour.and_then(RawWindow::window),
        seven_day: utilization.seven_day.and_then(RawWindow::window),
        fetched_at_ms: cached.fetched_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cached_windows_and_ignores_unknown_fields() {
        let usage = parse(
            br#"{"cachedUsageUtilization":{"fetchedAtMs":1760000000000,"accountUuid":"x",
                "utilization":{"five_hour":{"utilization":42,"resets_at":"2026-10-14T18:00:00Z"},
                "seven_day":{"utilization":100.4,"resets_at":null},"spend":{}}}}"#,
        )
        .unwrap();
        assert_eq!(usage.five_hour.as_ref().unwrap().used_percent, 42);
        assert_eq!(usage.seven_day.as_ref().unwrap().used_percent, 100);
        assert!(usage.exhausted());
        assert_eq!(usage.fetched_at_ms, Some(1_760_000_000_000));
    }

    #[test]
    fn missing_cache_is_none_not_an_error() {
        assert!(parse(br#"{"numStartups":3}"#).is_none());
        assert!(parse(b"not json").is_none());
        assert!(claude_usage(Some(Path::new("/nonexistent/claude"))).is_none());
    }
}
