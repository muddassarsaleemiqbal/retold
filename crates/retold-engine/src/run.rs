//! Creating runs and finding existing ones.

use std::path::{Path, PathBuf};

use retold_core::{Usd, Variant};
use retold_guide::{Channel, Guide, Inputs};
use serde::{Deserialize, Serialize};

use crate::layout::RunLayout;

pub const RUN_SCHEMA: u32 = 1;

/// How much effort (and money) a run spends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    /// Half the images: each one is used as a wide shot, then a close-up.
    Lean,
    #[default]
    Standard,
    /// More drafts and the best text models.
    Premium,
}

impl Quality {
    pub const ALL: [Self; 3] = [Self::Lean, Self::Standard, Self::Premium];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lean => "lean",
            Self::Standard => "standard",
            Self::Premium => "premium",
        }
    }

    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Lean => "half the images, lowest cost",
            Self::Standard => "the default",
            Self::Premium => "more drafts, best models",
        }
    }
}

impl std::str::FromStr for Quality {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|q| q.as_str().eq_ignore_ascii_case(s))
            .ok_or_else(|| format!("`{s}` is not a quality (lean, standard, premium)"))
    }
}

impl std::fmt::Display for Quality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `run.json`: what was asked for. Written once when the run starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunMeta {
    pub schema: u32,
    pub id: String,
    pub created: String,
    /// The title as typed, in any script.
    pub title: String,
    pub guide: GuideRef,
    pub channel: String,
    pub inputs: Inputs,
    pub variants: Vec<Variant>,
    pub quality: Quality,
    pub budget: Usd,
    /// `--yes`: continue through review pauses without stopping.
    pub skip_reviews: bool,
    #[serde(default)]
    pub source_file: bool,
    /// Folder levels under the channel and how many own the cast, so the layout can be
    /// rebuilt without the guide.
    pub folder: Vec<String>,
    pub cast_levels: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuideRef {
    pub id: String,
    pub version: String,
    /// Hash of the guide files; a mismatch on resume means the guide changed.
    pub hash: String,
}

/// Everything needed to start a run.
#[derive(Debug)]
pub struct NewRun<'a> {
    pub guide: &'a Guide,
    pub channel: &'a Channel,
    pub inputs: Inputs,
    pub variants: Vec<Variant>,
    pub quality: Quality,
    pub budget: Usd,
    pub skip_reviews: bool,
    pub source_file: Option<&'a Path>,
}

mod creation;
pub use creation::create;

/// A run found in the library.
#[derive(Debug, Clone)]
pub struct FoundRun {
    pub layout: RunLayout,
    pub meta: RunMeta,
}

/// Every run under the library, newest first. Unreadable runs are skipped.
#[must_use]
pub fn find_all(library: &Path) -> Vec<FoundRun> {
    let mut out = Vec::new();
    walk(library, library, 0, &mut out);
    out.sort_by(|a, b| b.meta.created.cmp(&a.meta.created));
    out
}

fn walk(library: &Path, dir: &Path, depth: usize, out: &mut Vec<FoundRun>) {
    const MAX_DEPTH: usize = 6;
    let run_file = dir.join(".retold").join("run.json");
    if let Ok(text) = std::fs::read_to_string(&run_file) {
        if let Ok(meta) = serde_json::from_str::<RunMeta>(&text)
            && let Some(layout) =
                RunLayout::from_video_dir(library, &meta.channel, dir, meta.cast_levels)
        {
            out.push(FoundRun { layout, meta });
        }
        return;
    }
    if depth >= MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || matches!(name.as_ref(), "deliverables" | "work" | "cast") {
            continue;
        }
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(library, &e.path(), depth + 1, out);
        }
    }
}

/// Finds a run by slug (`nosferatu-1922`), path under the channel (`series/you/season-1`) or
/// folder path.
#[must_use]
pub fn find(library: &Path, query: &str) -> Vec<FoundRun> {
    let as_path = PathBuf::from(query);
    find_all(library)
        .into_iter()
        .filter(|r| {
            r.layout.slug().as_str() == query
                || r.layout.display_path() == query
                || format!("{}/{}", r.meta.channel, r.layout.display_path()) == query
                || r.meta.id == query
                || (as_path.is_absolute() && r.layout.video_dir() == as_path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EngineError;
    use indexmap::IndexMap;
    use retold_guide::catalog::bundled;
    use retold_guide::channel::init_from_template;

    #[test]
    fn creates_finds_and_refuses_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = tmp.path().join("lib");
        let channel =
            init_from_template("test-studio", &tmp.path().join("cfg/test-studio")).unwrap();
        let guide = bundled()
            .unwrap()
            .into_iter()
            .find(|g| g.id() == "narrative-fixture")
            .unwrap();
        let make = |title: &str| {
            let raw: IndexMap<String, String> = [
                ("title".to_owned(), title.to_owned()),
                ("scope".to_owned(), "Season 1".to_owned()),
            ]
            .into();
            NewRun {
                guide: &guide,
                channel: &channel,
                inputs: retold_guide::inputs::resolve(guide.manifest(), &raw).unwrap(),
                variants: vec!["en".parse().unwrap(), "hi:hi,en.srt".parse().unwrap()],
                quality: Quality::Standard,
                budget: Usd::from_micros(1_000_000),
                skip_reviews: false,
                source_file: None,
            }
        };
        let (layout, meta) = create(&lib, make("You")).unwrap();
        assert!(layout.guide_snapshot().join("guide.toml").exists());
        assert_eq!(meta.title, "You");
        assert!(matches!(
            create(&lib, make("You")),
            Err(EngineError::RunExists { .. })
        ));

        create(&lib, make("Dark")).unwrap();
        assert_eq!(find_all(&lib).len(), 2);
        let found = find(&lib, "you-season-1");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].layout, layout);
        assert_eq!(find(&lib, "series/dark/season-1").len(), 1);
        assert_eq!(find(&lib, "test-studio/series/dark/season-1").len(), 1);
        assert_eq!(found[0].meta.variants[1].to_string(), "hi:hi,en.srt");
    }
}
