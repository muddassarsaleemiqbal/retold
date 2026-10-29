//! The folder layout of one video. Every path in a run comes from here, so no stage builds
//! paths by hand and the layout can change in one place.
//!
//! ```text
//! <library>/<channel>/<guide folder levels…>/      e.g. test-studio/series/you/season-1
//!   deliverables/<lang>/   <slug>.<lang>.mp4  thumbnail.png  thumbnails/  subtitles/  post/
//!   review.html
//!   work/01-research … 08-edit
//!   .retold/              run.json  state.json  guide/  cost.json  events.log  lock
//! ```

use std::path::{Path, PathBuf};

use retold_core::{LanguageTag, Slug};
use retold_guide::{Channel, Guide, Inputs, template};
use serde_json::json;

use crate::error::EngineError;

/// Pipeline stages, in order. Their work folders are numbered so they sort that way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Research,
    Script,
    Storyboard,
    Characters,
    Images,
    Voice,
    Captions,
    Edit,
}

impl Stage {
    pub const ALL: [Self; 8] = [
        Self::Research,
        Self::Script,
        Self::Storyboard,
        Self::Characters,
        Self::Images,
        Self::Voice,
        Self::Captions,
        Self::Edit,
    ];

    #[must_use]
    pub fn folder(self) -> &'static str {
        const STAGE_FOLDERS: &[(Stage, &str)] = &[
            (Stage::Research, "01-research"),
            (Stage::Script, "02-script"),
            (Stage::Storyboard, "03-storyboard"),
            (Stage::Characters, "04-characters"),
            (Stage::Images, "05-images"),
            (Stage::Voice, "06-voice"),
            (Stage::Captions, "07-captions"),
            (Stage::Edit, "08-edit"),
        ];
        STAGE_FOLDERS
            .iter()
            .find(|(item_stage, _)| *item_stage == self)
            .map_or("", |(_, folder_name)| *folder_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunLayout {
    channel_dir: PathBuf,
    video: PathBuf,
    cast_owner: PathBuf,
    slug: Slug,
    /// Folder levels under the channel, e.g. `["series", "you", "season-1"]`.
    levels: Vec<Slug>,
}

impl RunLayout {
    /// Works out where a video lives from the guide's `[run]` rules and the inputs.
    pub fn resolve(
        library: &Path,
        channel: &Channel,
        guide: &Guide,
        inputs: &Inputs,
    ) -> Result<Self, EngineError> {
        let rules = &guide.manifest().run;
        let ctx = json!({
            "inputs": inputs.template_context(),
            "channel": channel.template_context(),
            "guide": { "id": guide.id(), "name": guide.manifest().name },
        });
        let mut levels = Vec::new();
        let mut owner_levels = 0;
        for (i, t) in rules.folder.iter().enumerate() {
            let text = template::render(&format!("run.folder[{i}]"), t, &ctx)
                .map_err(|e| EngineError::Template(e.to_string()))?;
            if text.trim().is_empty() {
                continue;
            }
            let slug = Slug::from_title(&text).map_err(|e| EngineError::Template(e.to_string()))?;
            levels.push(slug);
            if i < rules.cast_depth {
                owner_levels = levels.len();
            }
        }
        if levels.is_empty() {
            return Err(EngineError::Template(
                "the guide's run.folder produced an empty path".into(),
            ));
        }
        Ok(Self::from_levels(
            library,
            channel.id(),
            levels,
            owner_levels,
        ))
    }

    /// Builds a layout from already-slugged folder levels.
    #[must_use]
    pub fn from_levels(
        library: &Path,
        channel: &str,
        levels: Vec<Slug>,
        cast_levels: usize,
    ) -> Self {
        build_layout_from_levels(library.join(channel), levels, cast_levels)
    }

    /// Re-creates the layout of an existing run from its folder.
    #[must_use]
    pub fn from_video_dir(
        library: &Path,
        channel: &str,
        video: &Path,
        cast_levels: usize,
    ) -> Option<Self> {
        let rel = video.strip_prefix(library.join(channel)).ok()?;
        let levels = rel
            .components()
            .map(|c| c.as_os_str().to_str()?.parse().ok())
            .collect::<Option<Vec<Slug>>>()?;
        (!levels.is_empty()).then(|| Self::from_levels(library, channel, levels, cast_levels))
    }

    /// Name used in file names and commands: `nosferatu-1922`, `you-season-1`.
    #[must_use]
    pub const fn slug(&self) -> &Slug {
        &self.slug
    }

    #[must_use]
    pub fn levels(&self) -> &[Slug] {
        &self.levels
    }

    /// Path under the channel, for display: `series/you/season-1`.
    #[must_use]
    pub fn display_path(&self) -> String {
        self.levels
            .iter()
            .map(Slug::as_str)
            .collect::<Vec<_>>()
            .join("/")
    }

    #[must_use]
    pub fn channel_dir(&self) -> &Path {
        &self.channel_dir
    }

    #[must_use]
    pub fn video_dir(&self) -> &Path {
        &self.video
    }

    // ---- deliverables ----

    #[must_use]
    pub fn deliverables(&self) -> PathBuf {
        self.video.join("deliverables")
    }

    #[must_use]
    pub fn deliverables_for(&self, lang: &LanguageTag) -> PathBuf {
        self.deliverables().join(lang.to_string())
    }

    /// `deliverables/<lang>/<slug>.<lang>.<ext>`.
    #[must_use]
    pub fn deliverable_file(&self, lang: &LanguageTag, ext: &str) -> PathBuf {
        self.deliverables_for(lang)
            .join(format!("{}.{lang}.{ext}", self.slug))
    }

    #[must_use]
    pub fn thumbnail(&self, lang: &LanguageTag) -> PathBuf {
        self.deliverables_for(lang).join("thumbnail.png")
    }

    /// `deliverables/<lang>/subtitles/<slug>.<track>.<ext>`.
    #[must_use]
    pub fn subtitle_file(&self, lang: &LanguageTag, track: &LanguageTag, ext: &str) -> PathBuf {
        self.deliverables_for(lang)
            .join("subtitles")
            .join(format!("{}.{track}.{ext}", self.slug))
    }

    #[must_use]
    pub fn post_dir(&self, lang: &LanguageTag) -> PathBuf {
        self.deliverables_for(lang).join("post")
    }

    #[must_use]
    pub fn review_page(&self) -> PathBuf {
        self.video.join("review.html")
    }

    // ---- work ----

    #[must_use]
    pub fn work(&self) -> PathBuf {
        self.video.join("work")
    }

    #[must_use]
    pub fn stage(&self, stage: Stage) -> PathBuf {
        self.work().join(stage.folder())
    }

    /// Character sheets: shared by every video under the cast owner (all seasons of a
    /// series), or kept inside the video when it owns its own cast (a film).
    #[must_use]
    pub fn cast_dir(&self) -> PathBuf {
        if self.cast_owner == self.video {
            self.stage(Stage::Characters).join("sheets")
        } else {
            self.cast_owner.join("cast")
        }
    }

    // ---- bookkeeping ----

    #[must_use]
    pub fn meta(&self) -> PathBuf {
        self.video.join(".retold")
    }
    #[must_use]
    pub fn run_file(&self) -> PathBuf {
        self.meta().join("run.json")
    }
    #[must_use]
    pub fn state_file(&self) -> PathBuf {
        self.meta().join("state.json")
    }
    #[must_use]
    pub fn guide_snapshot(&self) -> PathBuf {
        self.meta().join("guide")
    }
    #[must_use]
    pub fn cost_file(&self) -> PathBuf {
        self.meta().join("cost.json")
    }
    #[must_use]
    pub fn events_log(&self) -> PathBuf {
        self.meta().join("events.log")
    }
    #[must_use]
    pub fn lock_file(&self) -> PathBuf {
        self.meta().join("lock")
    }
}

/// Number of resolved folder levels that own the shared cast.
pub(crate) fn cast_levels(layout: &RunLayout) -> usize {
    layout
        .cast_owner
        .strip_prefix(&layout.channel_dir)
        .map_or(0, |relative| relative.components().count())
}

fn build_layout_from_levels(
    channel_dir: PathBuf,
    levels: Vec<Slug>,
    cast_levels: usize,
) -> RunLayout {
    let join_prefix = |count: usize| {
        levels
            .iter()
            .take(count)
            .fold(channel_dir.clone(), |path, level| path.join(level.as_str()))
    };
    let video = join_prefix(levels.len());
    let cast_owner = join_prefix(cast_levels.clamp(1, levels.len()));
    let last_level_idx = levels.len().saturating_sub(1);
    let slug = levels
        .iter()
        .skip(cast_levels.saturating_sub(1).min(last_level_idx))
        .map(Slug::as_str)
        .collect::<Vec<_>>()
        .join("-");
    let slug = Slug::from_title(&slug).unwrap_or_else(|_| levels[last_level_idx].clone());
    RunLayout {
        channel_dir,
        video,
        cast_owner,
        slug,
        levels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use retold_guide::catalog::bundled;
    use retold_guide::channel::init_from_template;

    fn setup(pairs: &[(&str, &str)]) -> (tempfile::TempDir, RunLayout) {
        let tmp = tempfile::tempdir().unwrap();
        let channel =
            init_from_template("test-studio", &tmp.path().join("cfg/test-studio")).unwrap();
        let guide = bundled()
            .unwrap()
            .into_iter()
            .find(|g| g.id() == "narrative-fixture")
            .unwrap();
        let raw: IndexMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect();
        let inputs = retold_guide::inputs::resolve(guide.manifest(), &raw).unwrap();
        let layout =
            RunLayout::resolve(&tmp.path().join("lib"), &channel, &guide, &inputs).unwrap();
        (tmp, layout)
    }

    #[test]
    fn film_is_one_folder_that_owns_its_cast() {
        let (tmp, l) = setup(&[("title", "Nosferatu"), ("kind", "film"), ("year", "1922")]);
        let lib = tmp.path().join("lib/test-studio");
        assert_eq!(l.video_dir(), lib.join("films/nosferatu-1922"));
        assert_eq!(l.slug().as_str(), "nosferatu-1922");
        assert_eq!(
            l.cast_dir(),
            lib.join("films/nosferatu-1922/work/04-characters/sheets")
        );
        let en: LanguageTag = "en".parse().unwrap();
        assert_eq!(
            l.deliverable_file(&en, "mp4"),
            lib.join("films/nosferatu-1922/deliverables/en/nosferatu-1922.en.mp4")
        );
        let hi: LanguageTag = "hi".parse().unwrap();
        assert_eq!(
            l.subtitle_file(&hi, &en, "srt"),
            lib.join("films/nosferatu-1922/deliverables/hi/subtitles/nosferatu-1922.en.srt")
        );
    }

    #[test]
    fn seasons_share_the_series_cast() {
        let (tmp, s1) = setup(&[("title", "You"), ("scope", "Season 1")]);
        let lib = tmp.path().join("lib/test-studio");
        assert_eq!(s1.video_dir(), lib.join("series/you/season-1"));
        assert_eq!(s1.slug().as_str(), "you-season-1");
        assert_eq!(s1.cast_dir(), lib.join("series/you/cast"));
        assert_eq!(
            s1.stage(Stage::Images),
            lib.join("series/you/season-1/work/05-images")
        );
        assert_eq!(
            s1.run_file(),
            lib.join("series/you/season-1/.retold/run.json")
        );
        let back =
            RunLayout::from_video_dir(&tmp.path().join("lib"), "test-studio", s1.video_dir(), 2)
                .unwrap();
        assert_eq!(back, s1);
    }

    #[test]
    fn film_year_is_optional() {
        let (_tmp, l) = setup(&[("title", "Inception"), ("kind", "film")]);
        assert_eq!(l.display_path(), "films/inception");
    }
}
