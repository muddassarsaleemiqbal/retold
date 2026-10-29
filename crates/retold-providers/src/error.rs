use std::time::Duration;

/// Why a provider call failed. The router uses the variant to decide whether to try the next
/// provider in the chain.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum ProviderError {
    /// Quota or rate limit; the provider goes on cooldown.
    #[error("rate-limited: {message}")]
    RateLimited {
        message: String,
        retry_after: Option<Duration>,
    },
    /// Not installed, not logged in, no key, no credit.
    #[error("unavailable: {0}")]
    Unavailable(String),
    /// This provider cannot do what was asked (e.g. image input).
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// A recognized OpenRouter parameter validation failed before any provider dispatch.
    #[error("request rejected before generation: {0}")]
    Unbilled(String),
    #[error("timed out: {0}")]
    Timeout(String),
    /// The reply did not match the requested shape.
    #[error("bad reply: {0}")]
    Malformed(String),
    /// The model declined the request.
    #[error("refused: {0}")]
    Refused(String),
    #[error("{0}")]
    Failed(String),
    #[error("cancelled")]
    Cancelled,
}

impl ProviderError {
    /// Whether retrying the same provider soon is likely to succeed: timeouts, garbled replies
    /// and unclassified failures. Rate limits, logins, refusals and unsupported requests are not.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Timeout(_) | Self::Malformed(_) | Self::Failed(_)
        )
    }

    /// Whether the provider should rest before being used again.
    #[must_use]
    pub const fn needs_cooldown(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Unavailable(_))
    }
}

const SECS_PER_MINUTE: u64 = 60;
const SECS_PER_HOUR: u64 = 3600;
const SECS_PER_DAY: u64 = 86_400;
const HOURS_PER_HALF_DAY: u64 = 12;
const MIN_DIGITS_TIMESTAMP: usize = 10;
const MAX_SNIPPET_LEN: usize = 400;

/// Parses retry delay from error messages (pipe timestamp, relative duration, or time of day).
#[must_use]
pub fn parse_retry_after(text: &str) -> Option<Duration> {
    parse_pipe_timestamp(text)
        .or_else(|| parse_relative_duration(text))
        .or_else(|| parse_time_of_day(text))
}

fn now_epoch_seconds() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

fn find_pipe_remainder(text: &str) -> Option<&str> {
    text.split_once('|').map(|(_, rest)| rest)
}

fn extract_digits(text: &str) -> &str {
    let count = text.bytes().take_while(u8::is_ascii_digit).count();
    &text[..count]
}

fn parse_u64(digits: &str) -> Option<u64> {
    digits.parse().ok()
}

fn parse_timestamp(digits: &str) -> Option<u64> {
    if digits.len() < MIN_DIGITS_TIMESTAMP {
        return None;
    }
    parse_u64(digits)
}

fn parse_pipe_timestamp(text: &str) -> Option<Duration> {
    let remainder = find_pipe_remainder(text)?;
    let digits = extract_digits(remainder);
    let timestamp = parse_timestamp(digits)?;
    let now = now_epoch_seconds()?;
    timestamp.checked_sub(now).map(Duration::from_secs)
}

fn find_after_marker<'a>(text: &'a str, markers: &[&str]) -> Option<&'a str> {
    markers.iter().find_map(|&marker| {
        let index = text.find(marker)?;
        let remainder = text.get(index..)?;
        remainder.strip_prefix(marker)
    })
}

fn extract_unit_after<'a>(rest: &'a str, prefix: &str) -> &'a str {
    rest.strip_prefix(prefix).unwrap_or(rest).trim_start()
}

fn parse_relative_duration(text: &str) -> Option<Duration> {
    let lower = text.to_lowercase();
    let rest = find_after_marker(
        &lower,
        &[
            "try again in ",
            "retry in ",
            "retry after ",
            "wait ",
            "resets in ",
        ],
    )?;
    let trimmed = rest.trim_start();
    let digits = extract_digits(trimmed);
    let count = parse_u64(digits)?;
    let unit = extract_unit_after(trimmed, digits);
    let factor = multiplier_seconds(unit)?;
    Some(Duration::from_secs(count.saturating_mul(factor)))
}

fn multiplier_seconds(unit: &str) -> Option<u64> {
    match unit.chars().next()? {
        's' => Some(1),
        'm' => Some(SECS_PER_MINUTE),
        'h' => Some(SECS_PER_HOUR),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Period {
    Am,
    Pm,
    Standard,
}

fn detect_period(text: &str) -> Period {
    if text.contains("pm") {
        Period::Pm
    } else if text.contains("am") {
        Period::Am
    } else {
        Period::Standard
    }
}

fn parse_clock_time(text: &str) -> Option<u64> {
    let (hours_part, after_colon) = text.split_once(':')?;
    let hours = parse_u64(hours_part.trim())?;
    let (minutes, period) = parse_minute_and_period(after_colon)?;
    let hour_24 = compute_24h(hours, period);
    Some(
        hour_24
            .saturating_mul(SECS_PER_HOUR)
            .saturating_add(minutes.saturating_mul(SECS_PER_MINUTE)),
    )
}

fn parse_minute_and_period(after_colon: &str) -> Option<(u64, Period)> {
    let digits = extract_digits(after_colon);
    let minutes = parse_u64(digits)?;
    let remainder = extract_unit_after(after_colon, digits);
    Some((minutes, detect_period(remainder)))
}

fn parse_time_of_day(text: &str) -> Option<Duration> {
    let lower = text.to_lowercase();
    let marker = find_after_marker(&lower, &["try again at ", "retry at ", "resets at "])?;
    let target_seconds = parse_clock_time(marker.trim_start())?;
    time_of_day_diff(target_seconds)
}

fn time_of_day_diff(target_seconds: u64) -> Option<Duration> {
    let now_secs = now_epoch_seconds()?;
    let current_day_secs = now_secs % SECS_PER_DAY;
    let diff = calculate_day_diff(target_seconds, current_day_secs);
    Some(Duration::from_secs(diff))
}

const fn calculate_day_diff(target_seconds: u64, current_day_secs: u64) -> u64 {
    if target_seconds > current_day_secs {
        target_seconds - current_day_secs
    } else {
        SECS_PER_DAY - current_day_secs + target_seconds
    }
}

const fn compute_24h(hours: u64, period: Period) -> u64 {
    match period {
        Period::Pm if hours < HOURS_PER_HALF_DAY => hours + HOURS_PER_HALF_DAY,
        Period::Am if hours == HOURS_PER_HALF_DAY => 0,
        _ => hours,
    }
}

/// Recognises quota and login problems in a CLI's error text.
pub(crate) fn classify_cli_failure(provider: &str, text: &str) -> ProviderError {
    let snippet = format_snippet(text);
    categorize_failure(provider, &snippet, text)
}

fn format_snippet(text: &str) -> String {
    text.trim().chars().take(MAX_SNIPPET_LEN).collect()
}

fn categorize_failure(provider: &str, snippet: &str, raw_text: &str) -> ProviderError {
    let lower = raw_text.to_lowercase();
    if is_rate_limit(&lower) {
        ProviderError::RateLimited {
            message: format!("{provider}: {snippet}"),
            retry_after: parse_retry_after(raw_text),
        }
    } else if is_login_problem(&lower) {
        ProviderError::Unavailable(format!("{provider} is not logged in ({snippet})"))
    } else {
        ProviderError::Failed(format!("{provider}: {snippet}"))
    }
}

fn is_rate_limit(lower: &str) -> bool {
    let any = |words: &[&str]| words.iter().any(|word| lower.contains(word));
    any(&[
        "rate limit",
        "rate_limit",
        "usage limit",
        "quota",
        "429",
        "too many requests",
        "overloaded",
        "limit reached",
    ])
}

fn is_login_problem(lower: &str) -> bool {
    let any = |words: &[&str]| words.iter().any(|word| lower.contains(word));
    any(&[
        "not logged in",
        "please log in",
        "/login",
        "unauthorized",
        "authentication",
        "invalid api key",
        "401",
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_failures() {
        assert!(matches!(
            classify_cli_failure("x", "Claude AI usage limit reached|1760000000"),
            ProviderError::RateLimited { .. }
        ));
        assert!(matches!(
            classify_cli_failure("x", "Invalid API key · Please run /login"),
            ProviderError::Unavailable(_)
        ));
        assert!(matches!(
            classify_cli_failure("x", "segfault"),
            ProviderError::Failed(_)
        ));
    }

    #[test]
    fn parses_retry_after_formats() {
        let relative = parse_retry_after("Rate limit reached. Try again in 45s.");
        assert_eq!(relative, Some(Duration::from_secs(45)));
        let minutes = parse_retry_after("overloaded: retry in 2 minutes");
        assert_eq!(minutes, Some(Duration::from_secs(120)));
        let time_of_day = parse_retry_after("Please try again at 11:30 pm");
        assert!(time_of_day.is_some());
    }
}
