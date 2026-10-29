//! Typed financial rows keep scope identifiers and windows intact through editing.
use super::collection::Item;
type Fill = Vec<(String, String)>;
use serde_json::Value;

pub fn limits(contents: &Value) -> Vec<Item> {
    contents["limits"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|raw| {
            let scope = scope_text(&raw["scope"]);
            let window = raw["window"].as_str().unwrap_or_default();
            let resources = &raw["resources"];
            let usd = crate::human::usd(crate::human::amount(&resources["usd_micros"]));
            Item {
                key: scope.clone(),
                label: format!("{scope} · {window}"),
                note: format!("{usd} · {} requests", resources["requests"]),
                raw: raw.clone(),
            }
        })
        .collect()
}

fn scope_text(scope: &Value) -> String {
    let kind = scope["kind"].as_str().unwrap_or_default();
    scope["id"]
        .as_str()
        .map_or_else(|| kind.to_owned(), |id| format!("{kind}:{id}"))
}

pub fn limit_fill(raw: &Value) -> Fill {
    vec![
        ("scope".into(), scope_text(&raw["scope"])),
        (
            "window".into(),
            raw["window"].as_str().unwrap_or_default().into(),
        ),
    ]
}

impl super::collection::Collection {
    pub fn money_lines(&self, item: &Item) -> Vec<String> {
        if self == &super::collection::LIMITS {
            return limit_details(&item.raw);
        }
        if self == &super::collection::ATTEMPTS {
            return attempt_details(item);
        }
        Vec::new()
    }
}
fn attempt_details(item: &Item) -> Vec<String> {
    let mut lines = vec![
        format!("Attempt {}", item.key),
        format!("State: {}", crate::human::text(&item.raw, "state")),
        format!("Video: {}", crate::human::text(&item.raw, "video_id")),
        "Quoted resources".into(),
    ];
    lines.extend(resources(&item.raw["quoted"]));
    lines.push("Observed resources".into());
    lines.extend(resources(&item.raw["observed"]));
    lines.push("Enter reviews reconciliation. It never permits another request.".into());
    lines
}
fn limit_details(raw: &Value) -> Vec<String> {
    let mut lines = vec![
        format!("Scope: {}", scope_text(&raw["scope"])),
        format!("Window: {}", crate::human::text(raw, "window")),
        String::new(),
    ];
    lines.extend(resources(&raw["resources"]));
    lines.push(String::new());
    lines.push("Enter edits this ceiling. Charges and holds are preserved.".into());
    lines
}
fn resources(value: &Value) -> Vec<String> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .map(|(unit, amount)| {
            let rendered = if amount.is_null() {
                "unknown; hold retained".into()
            } else if unit == "usd_micros" {
                crate::human::usd(crate::human::amount(amount))
            } else {
                amount.to_string()
            };
            format!("{}: {rendered}", unit.replace('_', " "))
        })
        .collect()
}
