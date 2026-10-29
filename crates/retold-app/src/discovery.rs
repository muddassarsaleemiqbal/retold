//! Bounded discovery of engine-owned run manifests. No indexes, locks or backups are written.

use crate::library::{Catalog, CatalogIssue, Video};
use crate::protocol::{Diagnostic, ErrorCode};
use retold_engine::{RunLayout, RunMeta, fsx};
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 10_000;
const MAX_DEPTH: usize = 12;
const MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const TOTAL_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
enum Manifest {
    Absent,
    Video(Box<Video>),
    Invalid(CatalogIssue),
}

pub fn scan(root: &Path) -> Result<Catalog, Diagnostic> {
    let context = crate::workspace::context::Context::resolve(Some(root))?;
    let root = absolute_root(&context.library)?;
    let mut catalog = Catalog {
        root: root.clone(),
        videos: Vec::new(),
        issues: Vec::new(),
    };
    if !root.exists() {
        return Ok(catalog);
    }
    let mut pending = vec![(root, 0)];
    let mut remaining = MAX_ENTRIES;
    let mut bytes_left = TOTAL_MANIFEST_BYTES;
    while let Some((directory, depth)) = pending.pop() {
        let (found, consumed) = manifest(&catalog.root, &directory, bytes_left)?;
        bytes_left = bytes_left.checked_sub(consumed).ok_or_else(limit_reached)?;
        if matches!(found, Manifest::Absent) {
            let (directories, available) = children(&directory, depth, remaining)?;
            remaining = available;
            pending.extend(directories);
        } else {
            catalog.record(found);
        }
    }
    catalog.sorted()
}

impl Catalog {
    fn record(&mut self, found: Manifest) {
        match found {
            Manifest::Video(video) => self.videos.push(*video),
            Manifest::Invalid(issue) => self.issues.push(issue),
            Manifest::Absent => {}
        }
    }
}

fn absolute_root(root: &Path) -> Result<PathBuf, Diagnostic> {
    match root.canonicalize() {
        Ok(root) => Ok(root),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let current = std::env::current_dir().map_err(|_| invalid_library())?;
            Ok(current.join(root))
        }
        Err(_) => Err(invalid_library()),
    }
}

fn manifest(root: &Path, directory: &Path, remaining: u64) -> Result<(Manifest, u64), Diagnostic> {
    let path = manifest_path(directory);
    let metadata = path.parent().ok_or_else(invalid_library)?;
    match std::fs::symlink_metadata(metadata) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Manifest::Absent, 0));
        }
        Err(_) => return Ok((Manifest::Invalid(issue(directory)), 0)),
        Ok(found) if !found.is_dir() => return Ok((Manifest::Invalid(issue(directory)), 0)),
        Ok(_) => {}
    }
    if directory == root && !path.exists() {
        return Ok((Manifest::Absent, 0));
    }
    if remaining == 0 {
        return Err(limit_reached());
    }
    let bytes = fsx::read_bounded(&path, remaining.min(MANIFEST_BYTES));
    let Ok(Some(bytes)) = bytes else {
        return Ok((Manifest::Invalid(issue(directory)), 0));
    };
    let consumed = bytes.len() as u64;
    let found = video(root, directory, &bytes).map_or_else(
        |()| Manifest::Invalid(issue(directory)),
        |video| Manifest::Video(Box::new(video)),
    );
    Ok((found, consumed))
}

fn manifest_path(directory: &Path) -> PathBuf {
    directory.join(".retold").join("run.json")
}

fn check_video_archived(directory: &Path) -> bool {
    directory.join(".archived").exists()
}

fn load_video_tags(directory: &Path) -> Vec<String> {
    let tags_path = directory.join(".retold").join("tags.json");
    let Ok(bytes) = std::fs::read(tags_path) else {
        return Vec::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn video(root: &Path, directory: &Path, bytes: &[u8]) -> Result<Video, ()> {
    let meta = parse_manifest(bytes)?;
    let created: jiff::Timestamp = meta.created.parse().map_err(|_| ())?;
    let layout =
        RunLayout::from_video_dir(root, &meta.channel, directory, meta.cast_levels).ok_or(())?;
    let actual: Vec<_> = layout.levels().iter().map(ToString::to_string).collect();
    if meta.folder != actual {
        return Err(());
    }
    let is_archived = check_video_archived(directory);
    Ok(Video {
        timestamp: created,
        id: meta.id,
        reference: format!("{}/{}", meta.channel, layout.display_path()),
        title: meta.title,
        channel: meta.channel,
        guide: meta.guide.id,
        created: created.to_string(),
        editions: meta.variants.iter().map(ToString::to_string).collect(),
        metadata_hash: retold_engine::store::hash(&bytes).map_err(|_| ())?,
        is_archived,
        tags: load_video_tags(directory),
        directory: PathBuf::from(directory),
        layout,
        budget: meta.budget,
    })
}

fn parse_manifest(bytes: &[u8]) -> Result<RunMeta, ()> {
    let meta: RunMeta = serde_json::from_slice(bytes).map_err(|_| ())?;
    let supported = meta.schema == retold_engine::run::RUN_SCHEMA;
    let identified = !meta.id.trim().is_empty();
    let owner_valid = meta.cast_levels <= meta.folder.len();
    if supported && identified && owner_valid {
        Ok(meta)
    } else {
        Err(())
    }
}

fn children(
    directory: &Path,
    depth: usize,
    mut remaining: usize,
) -> Result<(Vec<(PathBuf, usize)>, usize), Diagnostic> {
    let entries = std::fs::read_dir(directory).map_err(|_| invalid_library())?;
    let mut directories = Vec::new();
    for entry in entries {
        remaining = remaining.checked_sub(1).ok_or_else(limit_reached)?;
        let entry = entry.map_err(|_| invalid_library())?;
        if let Some(directory) = nested_directory(&entry)? {
            if depth >= MAX_DEPTH {
                return Err(limit_reached());
            }
            directories.push((directory, depth + 1));
        }
    }
    Ok((directories, remaining))
}

fn nested_directory(entry: &std::fs::DirEntry) -> Result<Option<PathBuf>, Diagnostic> {
    let kind = entry.file_type().map_err(|_| invalid_library())?;
    let name = entry.file_name();
    let name = name.to_string_lossy();
    let excluded = !kind.is_dir() || name.starts_with('.');
    Ok((!excluded).then(|| entry.path()))
}

fn issue(directory: &Path) -> CatalogIssue {
    CatalogIssue {
        directory: PathBuf::from(directory),
        code: "RG-DATA-INVALID".into(),
    }
}

fn invalid_library() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::DataInvalid,
        "The library cannot be inspected safely. Check its path and file permissions.",
    )
}

fn limit_reached() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::DataInvalid,
        "The library exceeds the bounded inspection limits. Select a smaller library root.",
    )
}
