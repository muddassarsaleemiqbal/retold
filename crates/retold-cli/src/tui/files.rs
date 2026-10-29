//! File browsing and media previews run on a worker, never in the reducer or renderer.
use super::input::Input;
use retold_app::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use serde_json::json;
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 2_000;
const PREVIEW_BYTES: u64 = 16_384;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Task {
    LoadDefaults(Option<PathBuf>),
    SaveDefaults(Option<PathBuf>, String),
    Clipboard(String),
    Resource {
        root: Option<PathBuf>,
        kind: String,
        id: String,
    },
    Browse(PathBuf),
    Preview(PathBuf),
    Open(PathBuf),
    Audio(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Browser {
    pub check: Option<Vec<String>>,
    pub id: u64,
    pub path: PathBuf,
    pub field: Option<String>,
    pub query: Input,
    pub selected: usize,
    pub listing: Option<Payload>,
    pub preview: Option<Payload>,
}

/// What Enter on the selection means.
pub enum Decision {
    /// A folder: list it, keeping the field being filled.
    Browse(PathBuf, Option<String>),
    /// A file: fill the field with it, or open it when no field is waiting.
    Choose(PathBuf, Option<String>),
    /// Nothing selected: the browser stays open.
    Stay(Box<Browser>),
}

impl Browser {
    pub fn notice(&self) -> Option<String> {
        let Some(Payload::Result { contents, .. }) = &self.listing else {
            return Option::default();
        };
        let limited = contents["truncated"] == true;
        let unavailable = contents["unavailable"].as_u64().unwrap_or_default();
        let limit = contents["limit"].as_u64().unwrap_or_default();
        match (limited, unavailable) {
            (false, 0) => Option::default(),
            (true, 0) => Some(format!(
                "First {limit} entries only; filter searches this subset."
            )),
            (true, count) => Some(format!("First {limit} entries only · {count} unavailable.")),
            (false, count) => Some(format!(
                "{count} unavailable entries (unsupported names or unreadable)."
            )),
        }
    }

    /// Arrows move the selection; anything else edits the filter.
    pub fn press(&mut self, key: ratatui::crossterm::event::KeyEvent) {
        use ratatui::crossterm::event::KeyCode;
        let moving = matches!(
            key.code,
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown
        );
        if moving {
            let count = self.entries().len();
            self.selected = super::landing::moved(self.selected, count, key.code);
        } else {
            self.query.edit(key);
            self.selected = 0;
        }
        self.preview = Option::default();
    }

    #[must_use]
    pub fn decide(self) -> Decision {
        let selected = self.selected_path();
        let folder = self.selected_directory();
        match (selected, folder) {
            (Some(path), true) => Decision::Browse(path, self.field),
            (Some(path), false) => Decision::Choose(path, self.field),
            (None, _) => Decision::Stay(Box::new(self)),
        }
    }

    /// Takes a listing or preview meant for this browser; stale previews are dropped.
    pub fn accept(&mut self, task: &Task, payload: Payload) {
        match task {
            Task::Browse(_) | Task::Resource { .. } => self.list(payload),
            Task::Preview(path) => {
                let current = self.selected_path();
                if current.as_ref() == Some(path) {
                    self.preview = Some(payload);
                }
            }
            Task::Open(_)
            | Task::Audio(_)
            | Task::LoadDefaults(_)
            | Task::SaveDefaults(..)
            | Task::Clipboard(_) => {}
        }
    }

    fn list(&mut self, payload: Payload) {
        let listed = match &payload {
            Payload::Result { contents, .. } => contents["path"].as_str().map(PathBuf::from),
            Payload::Error { .. } | Payload::Event(_) => Option::default(),
        };
        if let Some(path) = listed {
            self.path = path;
        }
        self.listing = Some(payload);
    }

    pub fn new(id: u64, path: PathBuf, field: Option<String>) -> Self {
        Self {
            check: None,
            id,
            path,
            field,
            query: Input::default(),
            selected: 0,
            listing: None,
            preview: None,
        }
    }

    pub fn entries(&self) -> Vec<serde_json::Value> {
        let Some(Payload::Result { contents, .. }) = &self.listing else {
            return Vec::new();
        };
        let query = self.query.text().to_lowercase();
        contents["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| {
                entry["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&query)
            })
            .cloned()
            .collect()
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.entries().get(self.selected)?["path"]
            .as_str()
            .map(PathBuf::from)
    }

    pub fn selected_directory(&self) -> bool {
        self.entries()
            .get(self.selected)
            .is_some_and(|entry| entry["directory"] == true)
    }
}

/// Runs a file task on a worker thread: list, preview, open or play.
pub fn run_task(task: &Task) -> Payload {
    let result = match task {
        Task::LoadDefaults(root) => {
            super::defaults::load(root.as_deref()).map(|values| done(json!({"defaults":values})))
        }
        Task::SaveDefaults(root, encoded) => {
            super::defaults::save(root.as_deref(), encoded).map(done)
        }
        Task::Clipboard(text) => {
            super::clipboard::copy(text).map(|()| done(json!({"copied":true})))
        }
        Task::Resource { root, kind, id } => resource(root.as_deref(), kind, id),
        Task::Browse(path) => browse(path),
        Task::Preview(path) => preview(path),
        Task::Open(path) => open(path),
        Task::Audio(path) => {
            super::audio::play(path).map(|()| done(json!({"text": "Audio preview toggled."})))
        }
    };
    result.unwrap_or_else(|error| {
        Diagnostic::new(ErrorCode::ExecutionFailed, &error.to_string()).into_payload()
    })
}

fn browse(path: &Path) -> std::io::Result<Payload> {
    let directory = std::fs::canonicalize(path)?;
    let path = directory.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "This directory path is not valid UTF-8.",
        )
    })?;
    let mut contents = directory_listing(&directory)?;
    contents["path"] = json!(path);
    Ok(done(contents))
}

fn directory_listing(directory: &Path) -> std::io::Result<serde_json::Value> {
    let scanned = scan_directory(directory)?;
    let count = scanned.len();
    let truncated = count > MAX_ENTRIES;
    let considered = count.min(MAX_ENTRIES);
    let entries: Vec<_> = scanned
        .into_iter()
        .take(MAX_ENTRIES)
        .filter_map(Result::ok)
        .filter_map(|entry| file_entry(&entry))
        .collect();
    let unavailable = considered.saturating_sub(entries.len());
    let entries = sorted_entries(entries);
    Ok(
        json!({"entries": entries, "limit": MAX_ENTRIES, "truncated": truncated,
        "unavailable": unavailable}),
    )
}

fn scan_directory(directory: &Path) -> std::io::Result<Vec<std::io::Result<std::fs::DirEntry>>> {
    Ok(std::fs::read_dir(directory)?
        .take(MAX_ENTRIES + 1)
        .collect())
}

fn sorted_entries(mut entries: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    entries.sort_by_key(|entry| {
        (
            !entry["directory"].as_bool().unwrap_or_default(),
            entry["name"].as_str().unwrap_or_default().to_lowercase(),
        )
    });
    entries
}

fn file_entry(entry: &std::fs::DirEntry) -> Option<serde_json::Value> {
    let path = entry.path();
    let path = path.to_str()?;
    let name = entry.file_name();
    let name = name.to_str()?;
    let metadata = entry.metadata().ok()?;
    Some(json!({"name": name, "path": path,
        "directory": metadata.is_dir(), "bytes": metadata.len()}))
}

fn preview(path: &Path) -> std::io::Result<Payload> {
    let resolved = std::fs::canonicalize(path)?;
    if sensitive(path) || sensitive(&resolved) {
        return Ok(done(
            json!({"text": "Sensitive file · preview hidden. Open in your editor to inspect it."}),
        ));
    }
    let metadata = std::fs::metadata(&resolved)?;
    if !metadata.is_file() {
        let text = if metadata.is_dir() {
            "Directory · enter to browse"
        } else {
            "Special file · preview unavailable"
        };
        return Ok(done(json!({"text": text})));
    }
    if let Some(image) = super::raster::decode(&resolved)? {
        return Ok(done(
            json!({"image": image, "text": path.file_name().unwrap_or_default().to_string_lossy()}),
        ));
    }
    text_preview(&resolved, metadata.len())
}

fn text_preview(path: &Path, size: u64) -> std::io::Result<Payload> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(PREVIEW_BYTES)
        .read_to_end(&mut bytes)?;
    let truncated = size > PREVIEW_BYTES;
    let text = readable_text(&bytes, size);
    Ok(done(
        json!({"text": text, "bytes": size, "truncated": truncated}),
    ))
}

fn readable_text(bytes: &[u8], size: u64) -> &str {
    let truncated = size > PREVIEW_BYTES;
    let decoded = match std::str::from_utf8(bytes) {
        Ok(text) => Some(text),
        Err(error) if truncated && error.error_len().is_none() => {
            std::str::from_utf8(&bytes[..error.valid_up_to()]).ok()
        }
        Err(_) => None,
    };
    decoded
        .filter(|text| !text.contains('\0'))
        .unwrap_or("Media or binary file · o opens it in your viewer; space previews audio")
}

fn sensitive(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let private_parent = path.ancestors().skip(1).any(credential_directory);
    private_parent
        || name.starts_with('.')
        || [
            "secret",
            "credential",
            "token",
            "auth.json",
            "private",
            ".pem",
            ".key",
        ]
        .iter()
        .any(|needle| name.contains(needle))
}

fn credential_directory(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    credential_directory_name(&name)
}

fn credential_directory_name(name: &str) -> bool {
    let cli_profile = name.starts_with(".codex") || name.starts_with(".claude");
    let credentials = matches!(
        name,
        ".ssh" | ".aws" | ".gnupg" | ".secrets" | ".credentials" | "secrets" | "credentials"
    );
    cli_profile || credentials
}

/// Opens `path` in the system viewer and waits for it to hand back.
fn open(path: &Path) -> std::io::Result<Payload> {
    let path = std::fs::canonicalize(path)?;
    let mut command = std::process::Command::new(open_program());
    let quiet = command
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    let status = quiet.stderr(std::process::Stdio::null()).status()?;
    let opened = status.success();
    if !opened {
        return Err(std::io::Error::other(
            "The viewer or player did not complete successfully.",
        ));
    }
    Ok(done(json!({"text": "Preview completed.", "path": path})))
}

const fn open_program() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}
const fn done(contents: serde_json::Value) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents,
    }
}

fn resource(root: Option<&Path>, kind: &str, id: &str) -> std::io::Result<Payload> {
    let context = retold_app::workspace::context::Context::resolve(root)
        .map_err(|error| std::io::Error::other(error.message))?;
    browse(&context.config.join(kind).join(id))
}
