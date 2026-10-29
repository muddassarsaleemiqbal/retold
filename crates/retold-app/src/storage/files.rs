//! Bounded inventory of engine-owned run folders and shared cast assets.
use super::{Entry, FILE_LIMIT, MAX_TOTAL_BYTES, digest, invalid, regular_file, validate_path};
use crate::library::Catalog;
use crate::protocol::Diagnostic;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 64;

#[derive(Debug, serde::Serialize)]
pub(super) struct Selection {
    pub files: Vec<Entry>,
    pub omitted: Vec<PathBuf>,
    pub total_bytes: u64,
    #[serde(skip)]
    root: PathBuf,
    #[serde(skip)]
    remaining: usize,
}

impl Selection {
    pub fn scan(catalog: &Catalog) -> Result<Self, Diagnostic> {
        let mut selection = Self {
            files: Vec::new(),
            omitted: Vec::new(),
            total_bytes: 0,
            root: catalog.root.clone(),
            remaining: FILE_LIMIT,
        };
        for path in managed_roots(catalog) {
            selection = selection.walk(&path)?;
        }
        selection
            .files
            .sort_by(|left, right| left.path.cmp(&right.path));
        selection.omitted.sort();
        Ok(selection)
    }

    fn walk(mut self, path: &Path) -> Result<Self, Diagnostic> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(invalid)?;
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| invalid())?
            .to_path_buf();
        let depth = relative.components().count();
        if depth > MAX_DEPTH {
            return Err(invalid());
        }
        if excluded(path) {
            self.omitted.push(relative);
            return Ok(self);
        }
        let metadata = std::fs::symlink_metadata(path).map_err(|_| invalid())?;
        if metadata.is_dir() {
            return self.walk_directory(path);
        }
        if !metadata.is_file() || !supported(path) {
            return Err(invalid());
        }
        validate_permissions(&metadata)?;
        let relative = portable_path(&relative)?;
        validate_path(&relative)?;
        self.record(path, &relative, metadata.len())
    }

    fn walk_directory(mut self, path: &Path) -> Result<Self, Diagnostic> {
        for child in std::fs::read_dir(path).map_err(|_| invalid())? {
            let child = child.map_err(|_| invalid())?;
            self = self.walk(&child.path())?;
        }
        Ok(self)
    }

    fn record(mut self, path: &Path, relative: &str, bytes: u64) -> Result<Self, Diagnostic> {
        self.total_bytes = self.total_bytes.checked_add(bytes).ok_or_else(invalid)?;
        if self.total_bytes > MAX_TOTAL_BYTES {
            return Err(invalid());
        }
        let blake3 = digest(path, bytes)?;
        self.files.push(Entry {
            path: relative.into(),
            bytes,
            blake3,
        });
        Ok(self)
    }

    pub fn copy_to(&self, target: &Path) -> Result<(), Diagnostic> {
        for entry in &self.files {
            copy_entry(&self.root, target, entry)?;
        }
        Ok(())
    }
}

fn managed_roots(catalog: &Catalog) -> BTreeSet<PathBuf> {
    let mut paths = BTreeSet::new();
    for video in &catalog.videos {
        paths.insert(video.layout.meta());
        paths.extend(optional_roots(&video.layout));
    }
    paths
        .iter()
        .filter(|path| {
            !path
                .ancestors()
                .skip(1)
                .any(|parent| paths.contains(parent))
        })
        .cloned()
        .collect()
}

fn optional_roots(layout: &retold_engine::RunLayout) -> Vec<PathBuf> {
    [
        layout.work(),
        layout.deliverables(),
        layout.cast_dir(),
        layout.review_page(),
    ]
    .into_iter()
    .filter(|path| path.try_exists().unwrap_or(true))
    .collect()
}

fn excluded(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    name.starts_with(".env")
        || matches!(
            name.as_str(),
            "lock"
                | "locks"
                | "owners"
                | "providers.toml"
                | "credentials.json"
                | "secrets.json"
                | "auth.json"
                | "settings.json"
                | "models"
                | "hooks"
                | ".git"
                | ".codex"
                | ".claude"
        )
}

fn supported(path: &Path) -> bool {
    const EXTENSIONS: &[&str] = &[
        "json", "jsonl", "toml", "txt", "md", "log", "png", "jpg", "jpeg", "webp", "mp4", "wav",
        "mp3", "m4a", "ogg", "flac", "srt", "vtt", "ass", "html", "csv", "svg",
    ];
    let recovery = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(retold_engine::store::is_recovery_filename);
    if recovery {
        return true;
    }
    let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();
    EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
}

#[cfg(unix)]
fn validate_permissions(metadata: &std::fs::Metadata) -> Result<(), Diagnostic> {
    use std::os::unix::fs::PermissionsExt;
    const EXECUTE_MASK: u32 = 0o111;
    let mode = metadata.permissions().mode();
    if mode & EXECUTE_MASK != 0 {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_permissions(_metadata: &std::fs::Metadata) -> Result<(), Diagnostic> {
    Ok(())
}

pub(super) fn copy_entry(root: &Path, target: &Path, entry: &Entry) -> Result<(), Diagnostic> {
    let source = root.join(&entry.path);
    if regular_file(&source)? != entry.bytes || digest(&source, entry.bytes)? != entry.blake3 {
        return Err(invalid());
    }
    let destination = target.join(&entry.path);
    let parent = destination.parent().ok_or_else(invalid)?;
    std::fs::create_dir_all(parent).map_err(|_| invalid())?;
    copy_bounded(&source, &destination, entry.bytes)?;
    if digest(&destination, entry.bytes)? != entry.blake3 {
        return Err(invalid());
    }
    Ok(())
}

fn copy_bounded(source: &Path, destination: &Path, expected: u64) -> Result<(), Diagnostic> {
    use std::io::Read;
    let input = std::fs::File::open(source).map_err(|_| invalid())?;
    let mut input = input.take(expected.saturating_add(1));
    let mut output = std::fs::File::create_new(destination).map_err(|_| invalid())?;
    let copied = std::io::copy(&mut input, &mut output).map_err(|_| invalid())?;
    if copied != expected {
        return Err(invalid());
    }
    output.sync_all().map_err(|_| invalid())
}

fn portable_path(path: &Path) -> Result<String, Diagnostic> {
    let parts = path
        .components()
        .map(|part| part.as_os_str().to_str().ok_or_else(invalid))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(parts.join("/"))
}
