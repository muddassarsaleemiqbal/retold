//! Discovers installed guides from the user's configuration folder.
//!
//! Production builds NEVER bundle default guides. A clean install starts with
//! zero installed guides; the user explicitly creates or imports them.

#[cfg(any(test, feature = "test-fixtures"))]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[cfg(any(test, feature = "test-fixtures"))]
use include_dir::{Dir, include_dir};

use crate::error::LoadError;
#[cfg(any(test, feature = "test-fixtures"))]
use crate::guide::is_text;
use crate::guide::{Guide, Origin};

#[cfg(any(test, feature = "test-fixtures"))]
static BUNDLED: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/guides");

/// An installed guide that parsed cleanly.
#[derive(Debug, Clone)]
pub struct Entry {
    pub guide: Guide,
}

/// A guide folder that failed to load.
#[derive(Debug)]
pub struct Broken {
    pub path: PathBuf,
    pub error: LoadError,
}

/// Every installed guide, plus any broken folders so the user can fix them.
#[derive(Debug, Default)]
pub struct Catalog {
    entries: Vec<Entry>,
    broken: Vec<Broken>,
}

impl Catalog {
    /// Loads fixture guides for tests that need a real guide without disk access.
    #[must_use]
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn from_test_guides() -> Self {
        Self {
            entries: bundled()
                .unwrap_or_default()
                .into_iter()
                .map(|guide| Entry { guide })
                .collect(),
            broken: Vec::new(),
        }
    }

    /// Loads only folders inside `user_dir` (if it exists).
    pub fn load(user_dir: Option<&Path>) -> Result<Self, LoadError> {
        let mut entries: Vec<Entry> = Vec::new();
        let mut broken = Vec::new();
        if let Some(dir) = user_dir.filter(|d| d.is_dir()) {
            let read = std::fs::read_dir(dir).map_err(|source| LoadError::Io {
                path: dir.display().to_string(),
                source,
            })?;
            let mut folders: Vec<PathBuf> = read
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && !p
                            .file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with('.'))
                })
                .collect();
            folders.sort();
            for path in folders {
                match Guide::from_dir(&path) {
                    Ok(guide) => {
                        let id = guide.id().to_owned();
                        if let Some(pos) = entries.iter().position(|e| e.guide.id() == id) {
                            let previous = entries.remove(pos);
                            let prev_path = match previous.guide.origin() {
                                Origin::Folder(p) => p.clone(),
                                Origin::Bundled | Origin::Inline => PathBuf::from(&id),
                            };
                            broken.push(Broken {
                                path: prev_path,
                                error: LoadError::invalid(
                                    "guide.id",
                                    vec![format!("duplicate guide id `{id}`")],
                                ),
                            });
                            broken.push(Broken {
                                path,
                                error: LoadError::invalid(
                                    "guide.id",
                                    vec![format!("duplicate guide id `{id}`")],
                                ),
                            });
                        } else if broken.iter().any(|b| {
                            matches!(&b.error, LoadError::Invalid { problems, .. } if problems.iter().any(|msg| msg.contains(&format!("duplicate guide id `{id}`"))))
                        }) {
                            broken.push(Broken {
                                path,
                                error: LoadError::invalid(
                                    "guide.id",
                                    vec![format!("duplicate guide id `{id}`")],
                                ),
                            });
                        } else {
                            entries.push(Entry { guide });
                        }
                    }
                    Err(error) => broken.push(Broken { path, error }),
                }
            }
        }
        entries.sort_by(|a, b| a.guide.id().cmp(b.guide.id()));
        broken.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self { entries, broken })
    }

    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    #[must_use]
    pub fn broken(&self) -> &[Broken] {
        &self.broken
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Guide> {
        self.entries.iter().map(|e| &e.guide).find(|g| g.id() == id)
    }

    /// Ids of all usable guides, for "did you mean" messages and completions.
    #[must_use]
    pub fn ids(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.guide.id()).collect()
    }
}

/// Test fixtures; never discovered or installed by production code.
#[cfg(any(test, feature = "test-fixtures"))]
pub fn bundled() -> Result<Vec<Guide>, LoadError> {
    let mut out = Vec::new();
    for dir in BUNDLED.dirs() {
        let mut files = BTreeMap::new();
        collect(dir, dir.path(), &mut files);
        out.push(Guide::from_files(Origin::Bundled, files)?);
    }
    Ok(out)
}

#[cfg(any(test, feature = "test-fixtures"))]
fn collect(dir: &Dir<'_>, root: &Path, out: &mut BTreeMap<String, String>) {
    for file in dir.files() {
        if !is_text(file.path()) {
            continue;
        }
        if let (Ok(rel), Some(text)) = (file.path().strip_prefix(root), file.contents_utf8()) {
            let rel = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.insert(rel, text.to_owned());
        }
    }
    for sub in dir.dirs() {
        collect(sub, root, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_guides_load_and_validate() {
        let guides = bundled().unwrap();
        assert!(guides.iter().any(|g| g.id() == "narrative-fixture"));
    }

    #[test]
    fn only_user_guides_are_loaded() {
        let tmp = tempfile::tempdir().unwrap();
        let story = bundled()
            .unwrap()
            .into_iter()
            .find(|g| g.id() == "narrative-fixture")
            .unwrap();
        let dir = tmp.path().join("my-copy");
        story.write_to(&dir).unwrap();
        let catalog = Catalog::load(Some(tmp.path())).unwrap();
        let entry = catalog
            .entries()
            .iter()
            .find(|e| e.guide.id() == "narrative-fixture")
            .unwrap();
        assert!(Catalog::load(None).unwrap().entries().is_empty());
        assert_eq!(entry.guide.origin(), &Origin::Folder(dir));
    }

    #[test]
    fn duplicate_guide_ids_are_both_rejected_to_broken() {
        let tmp = tempfile::tempdir().unwrap();
        let story = bundled()
            .unwrap()
            .into_iter()
            .find(|g| g.id() == "narrative-fixture")
            .unwrap();
        let dir1 = tmp.path().join("copy1");
        let dir2 = tmp.path().join("copy2");
        story.write_to(&dir1).unwrap();
        story.write_to(&dir2).unwrap();

        let catalog = Catalog::load(Some(tmp.path())).unwrap();
        assert!(
            catalog.entries().is_empty(),
            "duplicate guides must not be usable"
        );
        assert_eq!(
            catalog.broken().len(),
            2,
            "both duplicate folders must be marked broken"
        );
    }

    #[test]
    fn hidden_staging_directories_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let story = bundled()
            .unwrap()
            .into_iter()
            .find(|g| g.id() == "narrative-fixture")
            .unwrap();
        let staged = tmp.path().join(".retold-create-123");
        story.write_to(&staged).unwrap();

        let catalog = Catalog::load(Some(tmp.path())).unwrap();
        assert!(catalog.entries().is_empty());
        assert!(catalog.broken().is_empty());
    }
}
