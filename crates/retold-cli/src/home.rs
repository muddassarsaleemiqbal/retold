//! Home snapshot projections shared by plain output and the full-screen app, so both show the
//! same values. Labels are sanitized by `text`; nothing here touches the terminal.
use serde_json::Value;

use crate::human::{amount, text, usd, usd_down};

const SECONDS_PER_MINUTE: u64 = 60;
const MINUTES_PER_HOUR: u64 = 60;

/// One selectable Home row: what it shows and the command that does the same thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub group: String,
    pub label: String,
    pub command: String,
    pub target: Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Video(String),
    Videos,
    Spend,
    Providers,
    Account,
    Command,
    Unavailable,
}

/// Provider health in one line, such as `codex ✓  claude ⏳ 42m  openrouter ✗ logged out`.
pub fn providers(home: &Value, now: u64) -> String {
    let section = &home["providers"];
    let Some(entries) = section["data"]["health"].as_array() else {
        return format!("unavailable ({})", text(&section["error"], "code"));
    };
    if entries.is_empty() {
        return "none configured".into();
    }
    let badges: Vec<String> = entries.iter().map(|entry| badge(entry, now)).collect();
    badges.join("  ")
}

/// Fixed marks for statuses that carry no count or countdown.
const MARKS: [(&str, &str); 2] = [("ok", "✓"), ("logged_out", "✗ logged out")];

fn badge(entry: &Value, now: u64) -> String {
    format!("{} {}", text(entry, "id"), mark(entry, now))
}

fn mark(entry: &Value, now: u64) -> String {
    let status = text(entry, "status");
    let observation = &entry["observation"];
    let fixed = MARKS.iter().find(|(name, _)| *name == status);
    if let Some((_, mark)) = fixed {
        return (*mark).into();
    }
    let failures = amount(&observation["consecutive_failures"]).max(1);
    let wait = amount(&observation["retry_at"]).saturating_sub(now);
    match status.as_str() {
        "failing" => format!("✗ failing ×{failures}"),
        "limited" | "cooldown" | "unavailable" => format!("⏳ {}", duration(wait)),
        "cooldown_elapsed" => "? cooldown elapsed · not retested".into(),
        _ => "? not checked".into(),
    }
}

fn duration(seconds: u64) -> String {
    let minutes = seconds.div_ceil(SECONDS_PER_MINUTE);
    if minutes < MINUTES_PER_HOUR {
        format!("{minutes}m")
    } else {
        format!(
            "{}h{:02}m",
            minutes / MINUTES_PER_HOUR,
            minutes % MINUTES_PER_HOUR
        )
    }
}

/// Today's and this month's money, with the month limit when one is set.
pub fn spend(home: &Value) -> String {
    let today = &home["today"];
    let month = &home["month"];
    let (Some(today_report), Some(month_report)) =
        (today["data"].get("spend"), month["data"].get("spend"))
    else {
        let today_failed = today["data"].is_null();
        let failed = if today_failed { today } else { month };
        return unavailable("Spend", failed);
    };
    let held = amount(&today_report["total"]["resources"]["usd_micros"]["held"]);
    let held = if held > 0 {
        format!(" · {} held", usd(held))
    } else {
        String::new()
    };
    format!(
        "Today {}{held} · Month {}",
        charged(today_report),
        month_ceiling(month_report)
    )
}

fn charged(report: &Value) -> String {
    usd(amount(
        &report["total"]["resources"]["usd_micros"]["charged"],
    ))
}

fn month_ceiling(report: &Value) -> String {
    let spent = charged(report);
    report["ceiling"]["usd_micros"].as_object().map_or_else(
        || format!("{spent} · no month limit; paid work is blocked"),
        |money| {
            let limit = amount(&money["limit"]);
            let left = amount(&money["available"]);
            format!("{spent} of {} · {} left", usd(limit), usd_down(left))
        },
    )
}

fn unavailable(name: &str, section: &Value) -> String {
    format!("{name}: unavailable ({})", text(&section["error"], "code"))
}

/// Recent videos first, then the actions, in display order.
pub fn entries(home: &Value) -> Vec<Entry> {
    let videos = home["videos"]["data"]["items"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut entries: Vec<Entry> = videos.iter().map(video).collect();
    let actions = home["actions"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    entries.extend(actions.iter().map(action));
    entries
}

fn video(item: &Value) -> Entry {
    let reference = text(item, "ref");
    Entry {
        group: "continue".into(),
        label: format!("{reference} · {}", text(item, "title")),
        command: format!("retold video plan {reference}"),
        target: Target::Video(reference),
    }
}

fn action(item: &Value) -> Entry {
    let label = text(item, "label");
    let available = item["available"].as_bool().unwrap_or(false);
    let target = match (available, label.as_str()) {
        (true, "Videos") => Target::Videos,
        (true, "Spend") => Target::Spend,
        (true, "Providers") => Target::Providers,
        (true, "Account") => Target::Account,
        (true, _) => Target::Command,
        _ => Target::Unavailable,
    };
    Entry {
        group: text(item, "group"),
        label,
        command: text(item, "command"),
        target,
    }
}

/// How many videos the library holds, or why it could not be read.
pub fn library(home: &Value) -> String {
    let section = &home["videos"];
    section["data"]["total"].as_u64().map_or_else(
        || unavailable("Library", section),
        |total| {
            let shown = section["data"]["items"].as_array().map_or(0, Vec::len);
            format!("Videos: {total} · showing the {shown} most recent")
        },
    )
}

/// The linear, non-interactive Home: the same values the full-screen app shows.
pub fn plain(home: &Value, now: u64) -> String {
    let mut rows = vec![
        "Retold".into(),
        format!("Providers: {}", providers(home, now)),
        spend(home),
        library(home),
    ];
    rows.extend(entries(home).iter().map(entry_row));
    rows.push("Read only; nothing was changed.".into());
    rows.join("\n")
}

fn entry_row(entry: &Entry) -> String {
    let suffix = if entry.target == Target::Unavailable {
        " (not available yet)"
    } else {
        ""
    };
    format!(
        "  [{}] {}{suffix} — {}",
        entry.group, entry.label, entry.command
    )
}

/// Seconds since the Unix epoch, for cooldown countdowns.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn cooldown_health_is_distinct_from_an_unchecked_provider() {
        const CURRENT: u64 = 100;
        let record = json!({"status": "cooldown", "observation": {"retry_at": 160}});
        assert_eq!(super::mark(&record, CURRENT), "⏳ 1m");
        let elapsed = json!({"status": "cooldown_elapsed", "observation": {"retry_at": 1}});
        assert_eq!(
            super::mark(&elapsed, CURRENT),
            "? cooldown elapsed · not retested"
        );
    }
}
