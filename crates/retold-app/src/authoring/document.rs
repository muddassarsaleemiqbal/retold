//! Structured manifest edits validate a copy and reject stale review hashes before publication.
use super::{failed, invalid};
use crate::protocol::{Diagnostic, Outcome, Payload};
use retold_engine::{
    Slug,
    retold_guide::{Guide, Origin},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const MAX_DOCUMENT_BYTES: u64 = 1024 * 1024;

#[derive(Debug)]
pub struct Edit<'a> {
    pub field: &'a str,
    pub value: &'a str,
    pub expected_hash: &'a str,
    pub dry_run: bool,
}

struct Document {
    directory: PathBuf,
    path: PathBuf,
    original: Vec<u8>,
    value: Value,
}

pub fn document(config: &Path, kind: &str, id: &str) -> Result<Payload, Diagnostic> {
    let document = Document::load(config, kind, id)?;
    Ok(response(
        json!({"document": {"kind":kind, "id":id, "missing":super::fields::missing(kind,&document.value), "value":document.value,
        "hash":blake3::hash(&document.original).to_hex().to_string()}}),
    ))
}

pub fn edit_document(
    config: &Path,
    subject: (&str, &str),
    edit: &Edit<'_>,
) -> Result<Payload, Diagnostic> {
    let (kind, id) = subject;
    let directory = resource_directory(config, kind, id)?;
    let _owner = if edit.dry_run {
        None
    } else {
        crate::account::activity("edit configuration")
            .map_err(|error| crate::account::commands::diagnostic(&error))?
    };
    let _lock = if edit.dry_run {
        None
    } else {
        Some(Writer::open(&directory)?)
    };
    let document = Document::load(config, kind, id)?;
    let (document, before, after) = edit.apply(document, kind)?;
    let encoded = document.validate(kind)?;
    if !edit.dry_run {
        retold_engine::fsx::write_atomic(&document.path, encoded.as_bytes())
            .map_err(|_| failed("Could not publish the validated configuration."))?;
    }
    Ok(response(
        json!({"document_changed":{"kind":kind,"id":id,"field":edit.field,
        "before":before,"after":after,"dry_run":edit.dry_run},
        "text":format!("{} {} {}: {} → {}{}",kind,id,edit.field,before,after,if edit.dry_run {" (preview)"} else {""})}),
    ))
}

impl Document {
    fn load(config: &Path, kind: &str, id: &str) -> Result<Self, Diagnostic> {
        let directory = resource_directory(config, kind, id)?;
        let filename = manifest_name(kind)?;
        let path = directory.join(filename);
        let original = retold_engine::fsx::read_bounded(&path, MAX_DOCUMENT_BYTES)
            .map_err(|_| invalid("Manifest is unavailable or too large."))?
            .ok_or_else(|| invalid("Manifest is missing."))?;
        let source =
            std::str::from_utf8(&original).map_err(|_| invalid("Manifest must be UTF-8."))?;
        let parsed: toml::Value =
            toml::from_str(source).map_err(|_| invalid("Manifest TOML is invalid."))?;
        let value = serde_json::to_value(parsed)
            .map_err(|_| invalid("Manifest cannot be represented as structured fields."))?;
        Ok(Self {
            directory,
            path,
            original,
            value,
        })
    }

    fn validate(&self, kind: &str) -> Result<String, Diagnostic> {
        let encoded =
            toml::to_string_pretty(&self.value).map_err(|_| invalid("Value is not valid TOML."))?;
        if kind == "guides" {
            let mut files = Guide::from_dir(&self.directory)
                .map_err(|_| invalid("Original guide failed validation."))?
                .files()
                .clone();
            files.insert("guide.toml".into(), encoded.clone());
            Guide::from_files(Origin::Inline, files)
                .map_err(|_| invalid("Edited guide failed schema or template validation."))?;
        } else {
            super::channel::validate_edit(&self.directory, &encoded)?;
        }
        Ok(encoded)
    }
}

fn resource_directory(config: &Path, kind: &str, id: &str) -> Result<PathBuf, Diagnostic> {
    manifest_name(kind)?;
    let slug: Slug = id
        .parse()
        .map_err(|_| invalid("Choose a valid resource identifier."))?;
    let parent = config.join(kind);
    let directory = resource_path(&parent, &slug);
    check_resource_directories(&parent, &directory)?;
    Ok(directory)
}

fn resource_path(parent: &Path, slug: &Slug) -> PathBuf {
    parent.join(slug.as_str())
}

fn check_resource_directories(parent: &Path, directory: &Path) -> Result<(), Diagnostic> {
    check_directory(parent)?;
    check_directory(directory)
}

fn check_directory(path: &Path) -> Result<(), Diagnostic> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| invalid("Resource directory is missing."))?;
    let regular = metadata.is_dir() && !metadata.file_type().is_symlink();
    if !regular {
        return Err(invalid("Resource folders must be regular directories."));
    }
    Ok(())
}

fn manifest_name(kind: &str) -> Result<&'static str, Diagnostic> {
    match kind {
        "guides" => Ok("guide.toml"),
        "channels" => Ok("channel.toml"),
        _ => Err(invalid("Choose guides or channels.")),
    }
}

struct Writer(std::fs::File);
impl Writer {
    fn open(directory: &Path) -> Result<Self, Diagnostic> {
        let path = directory.join(".edit.lock");
        if path.is_symlink() {
            return Err(invalid("Configuration writer lock must not be a symlink."));
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(false);
        let file = options
            .open(path)
            .map_err(|_| failed("Could not open the configuration writer lock."))?;
        file.try_lock().map_err(|_| {
            failed("Another configuration edit is running. Retry after it finishes.")
        })?;
        Ok(Self(file))
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.0.unlock().ok();
    }
}
const fn response(contents: Value) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents,
    }
}

impl Edit<'_> {
    fn apply(
        &self,
        mut document: Document,
        kind: &str,
    ) -> Result<(Document, Value, Value), Diagnostic> {
        let current_hash = blake3::hash(&document.original).to_hex().to_string();
        if current_hash != self.expected_hash {
            return Err(invalid(
                "Configuration changed since review. Refresh and edit again.",
            ));
        }
        let immutable = ["/id", "/schema", ""].contains(&self.field);
        if immutable {
            return Err(invalid(
                "Resource identity and schema cannot be edited here.",
            ));
        }
        let replacement: Value =
            serde_json::from_str(self.value).map_err(|_| invalid("Supply a JSON field value."))?;
        let missing = document.value.pointer(self.field).is_none();
        if missing {
            let document = self.insert_optional(document, kind, &replacement)?;
            return Ok((document, Value::Null, replacement));
        }
        let selected = document
            .value
            .pointer_mut(self.field)
            .ok_or_else(|| invalid("Choose an existing JSON pointer field."))?;
        let before = std::mem::replace(selected, replacement.clone());
        Ok((document, before, replacement))
    }
}

impl Edit<'_> {
    fn insert_optional(
        &self,
        mut document: Document,
        kind: &str,
        replacement: &Value,
    ) -> Result<Document, Diagnostic> {
        let declared = super::fields::missing(kind, &document.value)
            .iter()
            .any(|field| field["path"].as_str() == Some(self.field));
        if !declared {
            return Err(invalid(
                "Choose a declared optional field with an existing parent.",
            ));
        }
        let (parent, key) = self
            .field
            .rsplit_once('/')
            .ok_or_else(|| invalid("Choose a schema field."))?;
        let parent = document
            .value
            .pointer_mut(parent)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| invalid("Field parent is unavailable."))?;
        let key = decode_pointer_key(key);
        parent.insert(key, replacement.clone());
        Ok(document)
    }
}

fn decode_pointer_key(key: &str) -> String {
    key.replace("~1", "/").replace("~0", "~")
}
