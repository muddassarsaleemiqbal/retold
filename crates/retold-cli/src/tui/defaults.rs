//! Only successful draft choices are remembered; execution authority is never persisted.
use super::form::Form;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::Path;
const FIELDS: [&str; 3] = ["lang", "quality", "budget"];
const MAX_BYTES: u64 = 64 * 1024;
const READ_BYTES: u64 = MAX_BYTES + 1;
const SETTINGS_NAME: &str = ".tui-defaults.json";

pub fn applies(form: &Form) -> bool {
    matches!(form.action.command().as_str(), "new" | "video new")
}
pub fn fill(mut form: Form, saved: &Value) -> Form {
    if !applies(&form) {
        return form;
    }
    for field in ["guide", "channel"] {
        let value = saved["last"][field].as_str();
        let applicable = form.permits_default(field);
        if let Some(value) = value.filter(|_| applicable) {
            form = form.with(field, value);
        }
    }
    let key = key(&form);
    for field in FIELDS {
        let value = saved["pairs"][&key][field].as_str();
        let applicable = form.permits_default(field);
        if let Some(value) = value.filter(|_| applicable) {
            form = form.with(field, value);
        }
    }
    form
}
pub fn remember(saved: &Value, form: &Form) -> Value {
    let mut saved = saved.clone();
    if !saved.is_object() {
        saved = json!({"pairs":{}});
    }
    let invalid_pairs = !saved["pairs"].is_object();
    if invalid_pairs {
        saved["pairs"] = json!({});
    }
    let pair = json!({"guide":form.value("guide"), "channel":form.value("channel")});
    saved["last"] = pair;
    let mut values = json!({});
    for field in FIELDS {
        values[field] = remembered_value(form, field);
    }
    saved["pairs"][key(form)] = values;
    saved
}
fn key(form: &Form) -> String {
    json!([form.value("guide"), form.value("channel")]).to_string()
}
pub fn load(root: Option<&Path>) -> std::io::Result<Value> {
    let path = directory(root)?.join(SETTINGS_NAME);
    let bytes = read_choices(&path)?;
    bytes.map_or_else(|| Ok(json!({})), |bytes| decode(&bytes))
}
fn read_choices(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Option::default()),
        Err(error) => return Err(error),
    };
    let valid = metadata.is_file() && metadata.len() <= MAX_BYTES;
    if !valid {
        return Err(std::io::Error::other(
            "Saved draft choices require a bounded regular file.",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(READ_BYTES)
        .read_to_end(&mut bytes)?;
    let oversized = u64::try_from(bytes.len()).unwrap_or_default() > MAX_BYTES;
    if oversized {
        return Err(std::io::Error::other("Saved draft choices are too large."));
    }
    Ok(Some(bytes))
}
fn decode(bytes: &[u8]) -> std::io::Result<Value> {
    let value: Value = serde_json::from_slice(bytes).map_err(std::io::Error::other)?;
    let object = value.is_object();
    if !object {
        return Err(std::io::Error::other(
            "Saved draft choices must be a JSON object.",
        ));
    }
    Ok(value)
}
pub fn save(root: Option<&Path>, encoded: &str) -> std::io::Result<Value> {
    validate_size(encoded)?;
    let directory = directory(root)?;
    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
    file.write_all(encoded.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(directory.join(SETTINGS_NAME))
        .map_err(std::io::Error::other)?;
    Ok(json!({"text":"Draft choices remembered."}))
}
fn directory(root: Option<&Path>) -> std::io::Result<std::path::PathBuf> {
    let context = retold_app::workspace::context::Context::resolve(root)
        .map_err(|error| std::io::Error::other(error.message))?;
    Ok(context.config)
}

pub fn successful_form(outcome: &super::page::Outcome) -> Option<Form> {
    let form = &outcome.form;
    let preview = form.value("dry-run").as_deref() == Some("true");
    if !applies(form) || preview {
        return None;
    }
    let Some(retold_app::protocol::Payload::Result {
        outcome: retold_app::protocol::Outcome::Completed,
        contents,
    }) = &outcome.payload
    else {
        return None;
    };
    let mut form = form.clone();
    for field in ["guide", "channel"] {
        let value = contents["created"][field].as_str();
        if let Some(value) = value {
            form = form.with(field, value);
        }
    }
    Some(form)
}

fn remembered_value(form: &Form, field: &str) -> Value {
    if field != "lang" {
        return json!(form.value(field));
    }
    let editions = form.list_values(field);
    if editions.is_empty() {
        Value::Null
    } else {
        json!(editions.join(";"))
    }
}

fn validate_size(encoded: &str) -> std::io::Result<()> {
    let bytes = u64::try_from(encoded.len()).unwrap_or(u64::MAX);
    if bytes > MAX_BYTES {
        return Err(std::io::Error::other(
            "Saved draft choices exceed the storage limit.",
        ));
    }
    Ok(())
}
pub fn choose(form: &Form, selection: (&str, &str), saved: &Value) -> Form {
    let (field, value) = selection;
    let mut next = form.clone().with(field, value);
    next.mark_edited(field);
    if !["guide", "channel"].contains(&field) || !applies(form) {
        return next;
    }
    let previous = &saved["pairs"][key(form)];
    for setting in FIELDS {
        let unchanged = previous[setting] == remembered_value(form, setting);
        let reset = unchanged && !form.edited.contains(setting);
        if reset {
            next = next.with(setting, "");
        }
    }
    fill(next, saved)
}
