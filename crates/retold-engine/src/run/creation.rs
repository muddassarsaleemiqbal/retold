//! A draft becomes visible only after all of its frozen inputs have been saved and synced.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use retold_guide::Guide;

use super::{GuideRef, NewRun, RUN_SCHEMA, RunMeta};
use crate::{EngineError, RunLayout, Stage, fsx};

const MAX_SOURCE_BYTES: u64 = 2_000_000;
const ID_HEX_LENGTH: usize = 32;
const NONCE_CHARACTERS: usize = 24;

/// Creates a complete draft without replacing existing content or admitting generation.
pub fn create(library: &Path, new: NewRun<'_>) -> Result<(RunLayout, RunMeta), EngineError> {
    new.validate()?;
    let source = read_source(new.source_file)?;
    let layout = RunLayout::resolve(library, new.channel, new.guide, &new.inputs)?;
    reject_existing(layout.video_dir(), &layout)?;
    crate::recovery::ensure_active(layout.video_dir())?;
    let destination = prepare_destination(library, &layout)?;
    let staging = tempfile::Builder::new()
        .prefix(".retold-create-")
        .rand_bytes(NONCE_CHARACTERS)
        .tempdir_in(destination.parent().ok_or_else(invalid_parent)?)
        .map_err(|error| EngineError::io(&destination, error))?;
    let meta = new.metadata(&layout, staging.path());
    Draft {
        new,
        source,
        staging,
    }
    .publish(&destination, &meta, &layout)?;
    Ok((layout, meta))
}

impl NewRun<'_> {
    fn validate(&self) -> Result<(), EngineError> {
        let invalid_budget = self.budget.micros() < 0;
        let no_editions = self.variants.is_empty();
        if invalid_budget || no_editions {
            return Err(invalid(
                "A draft needs a nonnegative budget and at least one edition.",
            ));
        }
        validate_editions(&self.variants)
    }

    fn metadata(&self, layout: &RunLayout, nonce: &Path) -> RunMeta {
        let created = jiff::Timestamp::now().to_string();
        RunMeta {
            schema: RUN_SCHEMA,
            id: new_id(layout.video_dir(), &created, nonce),
            created,
            title: self.title(layout),
            guide: guide_ref(self.guide),
            channel: self.channel.id().to_owned(),
            inputs: self.inputs.clone(),
            variants: self.variants.clone(),
            quality: self.quality,
            budget: self.budget,
            skip_reviews: self.skip_reviews,
            source_file: self.source_file.is_some(),
            folder: folder_levels(layout),
            cast_levels: crate::layout::cast_levels(layout),
        }
    }
    fn title(&self, layout: &RunLayout) -> String {
        self.inputs
            .get("title")
            .map_or_else(|| layout.slug().to_string(), ToString::to_string)
    }
}

struct Draft<'a> {
    new: NewRun<'a>,
    source: Option<String>,
    staging: tempfile::TempDir,
}

impl Draft<'_> {
    fn publish(
        self,
        destination: &Path,
        meta: &RunMeta,
        layout: &RunLayout,
    ) -> Result<(), EngineError> {
        let bookkeeping = bookkeeping(self.staging.path());
        save_guide(self.new.guide, &bookkeeping, &meta.guide.hash)?;
        save_channel(self.new.channel, &bookkeeping)?;
        self.save_source()?;
        fsx::write_json_atomic(&bookkeeping.join("run.json"), meta)?;
        fsx::publish_directory(self.staging.path(), destination)
            .map_err(|error| publication_error(error, layout))
    }

    fn save_source(&self) -> Result<(), EngineError> {
        if let Some(source) = &self.source {
            let path = source_path(self.staging.path());
            fsx::write_atomic(&path, source.as_bytes())?;
        }
        Ok(())
    }
}

fn validate_editions(editions: &[crate::Variant]) -> Result<(), EngineError> {
    let mut unique_editions = HashSet::new();
    for edition in editions {
        let unique = unique_editions.insert(edition);
        if !unique {
            return Err(invalid("A draft cannot contain duplicate editions."));
        }
        validate_edition(edition)?;
    }
    Ok(())
}

fn validate_edition(edition: &crate::Variant) -> Result<(), EngineError> {
    let parsed = edition.to_string().parse::<crate::Variant>();
    if parsed.as_ref() == Ok(edition) {
        Ok(())
    } else {
        Err(invalid(
            "The draft contains an invalid caption configuration.",
        ))
    }
}

fn bookkeeping(staging: &Path) -> PathBuf {
    staging.join(".retold")
}

fn source_path(staging: &Path) -> PathBuf {
    staging
        .join("work")
        .join(Stage::Research.folder())
        .join("source-file.txt")
}

fn save_guide(guide: &Guide, bookkeeping: &Path, expected: &str) -> Result<(), EngineError> {
    let snapshot = bookkeeping.join("guide");
    guide
        .write_to(&snapshot)
        .map_err(|error| invalid(&format!("Saving the guide snapshot: {error}")))?;
    validate_snapshot(&snapshot, expected)
}

fn save_channel(channel: &retold_guide::Channel, bookkeeping: &Path) -> Result<(), EngineError> {
    let text =
        toml::to_string_pretty(channel.file()).map_err(|error| invalid(&error.to_string()))?;
    fsx::write_atomic(&bookkeeping.join("channel.toml"), text.as_bytes())
}

fn folder_levels(layout: &RunLayout) -> Vec<String> {
    layout.levels().iter().map(ToString::to_string).collect()
}

fn prepare_destination(library: &Path, layout: &RunLayout) -> Result<PathBuf, EngineError> {
    let parent = layout.video_dir().parent().ok_or_else(invalid_parent)?;
    let relative = parent.strip_prefix(library).map_err(|_| invalid_parent())?;
    let relative = relative.to_str().ok_or_else(invalid_parent)?;
    std::fs::create_dir_all(library).map_err(|error| EngineError::io(library, error))?;
    let parent = crate::paths::workspace::inside(library, relative)?;
    std::fs::create_dir_all(&parent).map_err(|error| EngineError::io(&parent, error))?;
    let parent = parent
        .canonicalize()
        .map_err(|error| EngineError::io(&parent, error))?;
    let root = library
        .canonicalize()
        .map_err(|error| EngineError::io(library, error))?;
    sync_ancestors(&parent, &root)?;
    let destination = parent.join(layout.video_dir().file_name().ok_or_else(invalid_parent)?);
    reject_existing(&destination, layout)?;
    Ok(destination)
}

fn sync_ancestors(parent: &Path, root: &Path) -> Result<(), EngineError> {
    if !parent.starts_with(root) {
        return Err(invalid("Draft folders must remain inside the library."));
    }
    for ancestor in parent.ancestors() {
        fsx::sync_directory(ancestor)?;
        if ancestor == root {
            break;
        }
    }
    Ok(())
}

fn reject_existing(destination: &Path, layout: &RunLayout) -> Result<(), EngineError> {
    match std::fs::symlink_metadata(destination) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EngineError::io(destination, error)),
        Ok(_) => Err(already_exists(layout)),
    }
}

fn read_source(path: Option<&Path>) -> Result<Option<String>, EngineError> {
    path.map(|path| {
        let bytes = fsx::read_bounded(path, MAX_SOURCE_BYTES)?
            .ok_or_else(|| invalid("The source file does not exist."))?;
        let source = String::from_utf8(bytes)
            .map_err(|_| invalid("The source file must contain valid UTF-8."))?;
        let empty = source.trim().is_empty();
        if empty {
            return Err(invalid("The source file must not be empty."));
        }
        Ok(source)
    })
    .transpose()
}

fn validate_snapshot(path: &Path, expected: &str) -> Result<(), EngineError> {
    let saved = Guide::from_dir(path)
        .map_err(|error| invalid(&format!("Reading the saved guide: {error}")))?;
    let actual = saved.content_hash();
    if actual != expected {
        return Err(invalid("The saved guide differs from the requested guide."));
    }
    Ok(())
}

fn guide_ref(guide: &Guide) -> GuideRef {
    GuideRef {
        id: guide.id().to_owned(),
        version: guide.manifest().version.clone(),
        hash: guide.content_hash(),
    }
}

fn new_id(destination: &Path, created: &str, nonce: &Path) -> String {
    let mut digest = blake3::Hasher::new();
    digest.update(destination.as_os_str().as_encoded_bytes());
    digest.update(created.as_bytes());
    digest.update(nonce.as_os_str().as_encoded_bytes());
    format!("video-{}", &digest.finalize().to_hex()[..ID_HEX_LENGTH])
}

fn publication_error(error: EngineError, layout: &RunLayout) -> EngineError {
    if matches!(&error, EngineError::Io { source, .. } if source.kind() == std::io::ErrorKind::AlreadyExists)
    {
        already_exists(layout)
    } else {
        error
    }
}

fn already_exists(layout: &RunLayout) -> EngineError {
    EngineError::RunExists {
        path: layout.video_dir().to_owned(),
        slug: layout.slug().to_string(),
    }
}

fn invalid_parent() -> EngineError {
    invalid("The draft destination could not be resolved inside the library.")
}

fn invalid(message: &str) -> EngineError {
    EngineError::Setup(message.into())
}
