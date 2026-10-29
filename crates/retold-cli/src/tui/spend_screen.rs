//! Projections and formatting for the Spend & Limits screen.
use serde_json::Value;

use crate::human::{amount, text, usd, usd_down};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpendRow {
    pub key: String,
    pub attempts: u64,
    pub unresolved: u64,
    pub charged_usd: String,
    pub held_usd: String,
    pub headroom: String,
}

#[must_use]
pub fn summary_lines(report: &Value) -> Vec<String> {
    let timezone = text(report, "timezone");
    let total = &report["total"];
    let charged = amount(&total["resources"]["usd_micros"]["charged"]);
    let held = amount(&total["resources"]["usd_micros"]["held"]);
    let attempts = total["attempts"].as_u64().unwrap_or(0);
    let mut lines = vec![
        format!("Account Timezone: {timezone} · Recorded Attempts: {attempts}"),
        format!(
            "Total Charged: {} · Total Held: {}",
            usd(charged),
            usd(held)
        ),
    ];
    lines.push(ceiling_line(report));
    lines
}

fn ceiling_line(report: &Value) -> String {
    let ceiling = &report["ceiling"];
    let limit = ceiling["usd_micros"]["limit"].as_u64();
    let Some(max) = limit else {
        return "No ceiling configured for this window. Paid work is blocked.".into();
    };
    let available = ceiling["usd_micros"]["available"].as_u64().unwrap_or(0);
    format!(
        "Ceiling: {} · Available Headroom: {}",
        usd(max),
        usd_down(available)
    )
}

#[must_use]
pub fn rows(report: &Value) -> Vec<SpendRow> {
    let list = elements(&report["rows"]);
    list.iter().map(row).collect()
}

fn row(item: &Value) -> SpendRow {
    let key = item["key"].as_str().unwrap_or("(none)").to_owned();
    let attempts = item["attempts"].as_u64().unwrap_or(0);
    let unresolved = item["unresolved"].as_u64().unwrap_or(0);
    let charged = amount(&item["resources"]["usd_micros"]["charged"]);
    let held = amount(&item["resources"]["usd_micros"]["held"]);
    let headroom = row_headroom(item);
    SpendRow {
        key,
        attempts,
        unresolved,
        charged_usd: usd(charged),
        held_usd: usd(held),
        headroom,
    }
}

fn row_headroom(item: &Value) -> String {
    let limit = item["ceiling"]["usd_micros"]["limit"].as_u64();
    let Some(max) = limit else {
        return "unbounded".into();
    };
    let available = item["ceiling"]["usd_micros"]["available"]
        .as_u64()
        .unwrap_or(0);
    format!("{} left of {}", usd_down(available), usd(max))
}

fn elements(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

const WINDOWS: [&str; 3] = ["month", "today", "lifetime"];
const GROUPS: [&str; 4] = ["provider", "video", "series", "day"];
const BAR_COLUMNS: u64 = 20;

#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub window: usize,
    pub group: usize,
}

impl Selection {
    pub fn want(&self) -> super::store::Want {
        super::store::Want {
            words: self.words(),
            live: true,
            interval: super::store::REFRESH_MILLIS,
        }
    }
    pub fn words(&self) -> Vec<String> {
        let mut words = vec!["spend".into(), "--by".into(), self.group().into()];
        if self.window() != "lifetime" {
            words.push(format!("--{}", self.window()));
        }
        words
    }
    pub const fn window(&self) -> &'static str {
        WINDOWS[self.window % WINDOWS.len()]
    }
    pub const fn group(&self) -> &'static str {
        GROUPS[self.group % GROUPS.len()]
    }
    pub const fn next_window(&mut self) {
        self.window = self.window.wrapping_add(1);
    }
    pub const fn next_group(&mut self) {
        self.group = self.group.wrapping_add(1);
    }
}

pub fn chart(report: &Value) -> Vec<String> {
    let rows = elements(&report["rows"]);
    let maximum = rows.iter().map(total_money).max().unwrap_or(1).max(1);
    rows.iter()
        .map(|row| {
            let total = total_money(row);
            let cells = total.saturating_mul(BAR_COLUMNS) / maximum;
            let histogram = "#".repeat(usize::try_from(cells).unwrap_or_default());
            format!(
                "{:<20} {histogram} {}",
                crate::human::text(row, "key"),
                usd(total)
            )
        })
        .collect()
}

fn total_money(row: &Value) -> u64 {
    let money = &row["resources"]["usd_micros"];
    amount(&money["charged"]).saturating_add(amount(&money["held"]))
}
