//! Read-only projection of recorded supervision and live identity evidence.
use super::collection::Item;
use crate::human::text;
use serde_json::Value;

pub fn items(contents: &Value) -> Vec<Item> {
    contents["processes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|raw| Item {
            key: text(raw, "id"),
            label: text(&raw["owner"], "purpose"),
            note: format!("{} · pid {}", text(raw, "status"), raw["owner"]["pid"]),
            raw: raw.clone(),
        })
        .collect()
}

pub fn details(raw: &Value) -> Vec<String> {
    let mut lines = vec![
        "Inspection only; no process is signalled.".into(),
        format!("Record: {}", text(raw, "id")),
        format!("Status: {}", text(raw, "status")),
        format!("Owner PID: {}", raw["owner"]["pid"]),
        format!("Purpose: {}", text(&raw["owner"], "purpose")),
        format!("Since: {}", text(&raw["owner"], "since")),
    ];
    for role in ["supervisor", "worker"] {
        lines.extend(identity(raw, role));
    }
    lines
}

fn identity(raw: &Value, role: &str) -> Vec<String> {
    let evidence = text(raw, &format!("{role}_observation"));
    let saved = &raw[role];
    let identity = if saved.is_null() {
        "not recorded".into()
    } else {
        saved.to_string()
    };
    vec![
        format!("{role}: {identity}"),
        format!("{role} evidence: {evidence}"),
    ]
}
