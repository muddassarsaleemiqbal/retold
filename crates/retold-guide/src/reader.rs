//! Guide text loading is bounded and never follows symlinks inside a resource.
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use crate::LoadError;
use crate::file_set::{MAX_BYTES, MAX_DEPTH, MAX_ENTRIES};

pub fn read_guide(root: &Path) -> Result<BTreeMap<String, String>, LoadError> {
    let reader = Reader {
        root,
        files: BTreeMap::new(),
        entries: MAX_ENTRIES,
        bytes: MAX_BYTES,
    };
    let reader = reader.walk(Path::new(""))?;
    Ok(reader.files)
}

struct Reader<'a> {
    root: &'a Path,
    files: BTreeMap<String, String>,
    entries: usize,
    bytes: u64,
}

impl Reader<'_> {
    fn walk(mut self, relative: &Path) -> Result<Self, LoadError> {
        let depth = relative.components().count();
        if depth > MAX_DEPTH {
            return Err(invalid(
                self.root,
                "Guide nesting exceeds the inspection limit.",
            ));
        }
        let directory = self.root.join(relative);
        let entries =
            std::fs::read_dir(&directory).map_err(|error| read_error(&directory, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| read_error(&directory, error))?;
            self.entries = self
                .entries
                .checked_sub(1)
                .ok_or_else(|| invalid(self.root, "Guide has too many entries."))?;
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if hidden {
                continue;
            }
            self = self.read_entry(&entry, relative)?;
        }
        Ok(self)
    }

    fn read_entry(self, entry: &std::fs::DirEntry, relative: &Path) -> Result<Self, LoadError> {
        let (path, kind) = entry_details(entry, relative)?;
        if kind.is_symlink() {
            return Err(invalid(
                &entry.path(),
                "Guide files and directories must not be symlinks.",
            ));
        }
        if kind.is_dir() {
            return self.walk(&path);
        }
        if kind.is_file() && crate::guide::is_text(&path) {
            return self.read_text(&path);
        }
        Ok(self)
    }

    fn read_text(mut self, relative: &Path) -> Result<Self, LoadError> {
        let path = self.root.join(relative);
        let text = bounded_text(&path, self.bytes)?;
        self.bytes = remaining_bytes(self.bytes, &text, &path)?;
        self.files.insert(relative_name(relative)?, text);
        Ok(self)
    }
}

fn remaining_bytes(ceiling: u64, text: &str, path: &Path) -> Result<u64, LoadError> {
    let size = u64::try_from(text.len()).unwrap_or(u64::MAX);
    ceiling
        .checked_sub(size)
        .ok_or_else(|| invalid(path, "Guide grew beyond the text limit."))
}

fn relative_name(path: &Path) -> Result<String, LoadError> {
    let components = path
        .components()
        .map(|part| part.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid(path, "Guide filenames must be valid UTF-8."))?;
    Ok(components.join("/"))
}

fn invalid(path: &Path, message: &str) -> LoadError {
    LoadError::invalid(&path.display().to_string(), vec![message.into()])
}

fn read_error(path: &Path, source: std::io::Error) -> LoadError {
    LoadError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn bounded_text(path: &Path, ceiling: u64) -> Result<String, LoadError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| read_error(path, error))?;
    if !metadata.is_file() || metadata.len() > ceiling {
        return Err(invalid(path, "Guide exceeds the 16 MB text limit."));
    }
    let file = std::fs::File::open(path).map_err(|error| read_error(path, error))?;
    let mut text = String::new();
    file.take(ceiling.saturating_add(1))
        .read_to_string(&mut text)
        .map_err(|error| read_error(path, error))?;
    Ok(text)
}

fn entry_details(
    entry: &std::fs::DirEntry,
    relative: &Path,
) -> Result<(std::path::PathBuf, std::fs::FileType), LoadError> {
    let path = relative.join(entry.file_name());
    let kind = entry
        .file_type()
        .map_err(|error| read_error(&entry.path(), error))?;
    Ok((path, kind))
}
