//! Offline discoveries become editable provider definitions, never dispatch authority.
use serde_json::{Value, json};
use std::path::Path;

use super::collection::Item;

const ADAPTERS: [(&str, &str); 4] = [
    ("claude", "claude_cli"),
    ("codex", "codex_cli"),
    ("gemini", "gemini_cli"),
    ("llama-completion", "local_text"),
];
const FIELDS: [&str; 5] = ["id", "kind", "binary", "profile-dir", "key-env"];
const MAX_ALIAS_LENGTH: usize = 64;

pub fn items(contents: &Value) -> Vec<Item> {
    let inventory = &contents["inventory"];
    let tools = array(inventory, "tools").iter().filter_map(tool);
    let profiles = array(inventory, "profiles")
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|path| profile(inventory, path));
    tools.chain(profiles).chain(credential(inventory)).collect()
}

fn array<'a>(value: &'a Value, field: &str) -> &'a [Value] {
    value[field].as_array().map_or(&[], Vec::as_slice)
}

fn tool(record: &Value) -> Option<Item> {
    let name = record["name"].as_str()?;
    let kind = ADAPTERS.iter().find(|(tool, _)| *tool == name)?.1;
    let binary = record["path"].as_str().filter(|path| !path.is_empty())?;
    Some(candidate(
        name,
        "Program on PATH",
        json!({"id": name, "kind": kind, "binary": binary}),
    ))
}

fn profile(inventory: &Value, path: &str) -> Option<Item> {
    let name = Path::new(path).file_name()?.to_str()?;
    let (tool, kind) = ADAPTERS
        .iter()
        .take(2)
        .find(|(tool, _)| name.starts_with(&format!(".{tool}")))?;
    let binary = array(inventory, "tools")
        .iter()
        .find(|record| record["name"].as_str() == Some(tool))
        .and_then(|record| record["path"].as_str());
    let id = profile_alias(name);
    Some(candidate(
        name.trim_start_matches('.'),
        "Profile folder · login unchecked",
        json!({"id": id, "kind": kind, "binary": binary, "profile-dir": path}),
    ))
}

fn profile_alias(name: &str) -> String {
    name.trim_start_matches('.')
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() || letter == '_' {
                letter
            } else {
                '-'
            }
        })
        .take(MAX_ALIAS_LENGTH)
        .collect()
}

fn credential(inventory: &Value) -> Option<Item> {
    let present = array(inventory, "credential_variables")
        .iter()
        .any(|record| record["name"] == "OPENROUTER_API_KEY" && record["present"] == true);
    present.then(|| {
        candidate(
            "openrouter",
            "Key variable present · login unchecked",
            json!({"id": "openrouter", "kind": "openrouter", "key-env": "OPENROUTER_API_KEY"}),
        )
    })
}

fn candidate(label: &str, note: &str, raw: Value) -> Item {
    Item {
        key: label.to_owned(),
        label: label.to_owned(),
        note: note.to_owned(),
        raw,
    }
}

pub fn fill(item: &Item) -> Vec<(String, String)> {
    FIELDS
        .into_iter()
        .filter_map(|field| {
            item.raw[field]
                .as_str()
                .map(|value| (field.to_owned(), value.to_owned()))
        })
        .collect()
}
