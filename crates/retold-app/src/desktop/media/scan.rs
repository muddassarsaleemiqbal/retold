//! Bounded artifact traversal never follows symlinks or hides an incomplete result.
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use super::{Diagnostic, Item, Kind, invalid};

/// A scan examines at most 4096 filesystem entries and descends eight directories.
const SCAN_LIMIT: usize = 4096;
const DEPTH_LIMIT: usize = 8;

const TYPES: &[(&[&str], Kind)] = &[
    (&["png", "jpg", "jpeg", "webp"], Kind::Image),
    (&["wav", "mp3", "flac", "m4a", "ogg"], Kind::Audio),
    (&["mp4", "mov", "mkv", "webm"], Kind::Video),
    (&["srt", "vtt", "ass"], Kind::Captions),
    (&["mlt", "otio", "fcpxml"], Kind::Project),
];

struct Scan {
    root: PathBuf,
    pending: VecDeque<(PathBuf, usize)>,
    editions: Vec<(String, String)>,
    items: Vec<Item>,
    visited: usize,
    partial: bool,
}

pub(super) fn collect(root: &Path, editions: &[String]) -> Result<(Vec<Item>, bool), Diagnostic> {
    let root = root.canonicalize().map_err(invalid)?;
    let scan = Scan {
        pending: VecDeque::from([(root.clone(), 0)]),
        root,
        editions: edition_paths(editions)?,
        items: Vec::new(),
        visited: 0,
        partial: false,
    };
    let scan = scan.walk()?;
    Ok((scan.items, scan.partial))
}

fn edition_paths(editions: &[String]) -> Result<Vec<(String, String)>, Diagnostic> {
    editions
        .iter()
        .map(|specification| {
            let variant = specification
                .parse::<retold_engine::Variant>()
                .map_err(invalid)?;
            Ok((
                retold_engine::languages::edition_id(&variant),
                specification.clone(),
            ))
        })
        .collect()
}

impl Scan {
    fn walk(mut self) -> Result<Self, Diagnostic> {
        loop {
            let next = self.pending.pop_front();
            let Some((directory, depth)) = next else {
                break;
            };
            let entries = std::fs::read_dir(directory).map_err(invalid)?;
            for entry in entries {
                if self.visited == SCAN_LIMIT {
                    self.partial = true;
                    return Ok(self);
                }
                self.visited += 1;
                self = self.visit(&entry.map_err(invalid)?, depth)?;
            }
        }
        Ok(self)
    }

    fn visit(mut self, entry: &std::fs::DirEntry, depth: usize) -> Result<Self, Diagnostic> {
        let kind = entry.file_type().map_err(invalid)?;
        let symlink = kind.is_symlink();
        if symlink {
            self.partial = true;
            return Ok(self);
        }
        let path = entry.path();
        let Some(relative) = path
            .strip_prefix(&self.root)
            .ok()
            .and_then(Path::to_str)
            .map(str::to_owned)
        else {
            self.partial = true;
            return Ok(self);
        };
        let hidden = relative
            .split(std::path::MAIN_SEPARATOR)
            .any(|part| part.starts_with('.'));
        if hidden {
            return Ok(self);
        }
        if kind.is_dir() {
            self = self.directory(path, depth);
        } else if kind.is_file() {
            self = self.file(entry, &relative)?;
        }
        Ok(self)
    }

    fn directory(mut self, path: PathBuf, depth: usize) -> Self {
        if depth < DEPTH_LIMIT {
            self.pending.push_back((path, depth + 1));
        } else {
            self.partial = true;
        }
        self
    }

    fn file(mut self, entry: &std::fs::DirEntry, relative: &str) -> Result<Self, Diagnostic> {
        let path = entry.path();
        let Some(kind) = file_kind(&path) else {
            return Ok(self);
        };
        let edition = relative.split(std::path::MAIN_SEPARATOR).find_map(|part| {
            self.editions
                .iter()
                .find(|(id, _)| id == part)
                .map(|(_, specification)| specification.clone())
        });
        let identity = super::identity(&entry.metadata().map_err(invalid)?)?;
        self.items.push(Item {
            relative: relative.into(),
            bytes: identity.bytes,
            identity,
            path,
            kind,
            edition,
            expected_hash: None,
        });
        Ok(self)
    }
}

fn file_kind(path: &Path) -> Option<Kind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    TYPES
        .iter()
        .find(|(extensions, _)| extensions.contains(&extension.as_str()))
        .map(|(_, kind)| *kind)
}
