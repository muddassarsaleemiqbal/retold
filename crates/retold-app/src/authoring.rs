//! Explicit user-owned resource authoring. Validation precedes atomic, non-overwriting publication.
use std::path::{Path, PathBuf};

use retold_engine::Slug;

use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

mod channel;
mod document;
mod examples;
mod fields;
mod guide;
mod inspection;
pub use channel::{ChannelOptions, create_channel};
pub use document::{Edit, document, edit_document};
pub use guide::{GuideOptions, create_guide};
pub use inspection::{check, schema};

struct Target<'a> {
    config: &'a Path,
    kind: &'static str,
    id: Slug,
}

#[derive(Clone, Copy)]
enum Publication {
    Preview,
    Created,
}

impl<'a> Target<'a> {
    fn new(config: &'a Path, kind: &'static str, id: &str) -> Result<Self, Diagnostic> {
        let id = id
            .parse()
            .map_err(|_| invalid("Names must use lowercase letters, digits and single dashes."))?;
        let target = Self { config, kind, id };
        let exists = std::fs::symlink_metadata(target.directory()).is_ok();
        if exists {
            return Err(invalid(
                "This configuration already exists. Creation never overwrites it.",
            ));
        }
        Ok(target)
    }

    fn directory(&self) -> PathBuf {
        self.config.join(self.kind).join(self.id.as_str())
    }

    fn stage(&self) -> Result<tempfile::TempDir, Diagnostic> {
        let parent = self.config.join(self.kind);
        ensure_resource_directory(&parent)?;
        tempfile::Builder::new()
            .prefix(".retold-create-")
            .tempdir_in(parent)
            .map_err(|_| failed("Could not prepare a configuration folder."))
    }

    fn publish(&self, staging: &Path) -> Result<Payload, Diagnostic> {
        crate::storage::publish_directory(staging, &self.directory()).map_err(|_| {
            failed("Could not publish this configuration. Existing configurations were preserved.")
        })?;
        Ok(self.report(Publication::Created))
    }

    fn report(&self, publication: Publication) -> Payload {
        let dry_run = matches!(publication, Publication::Preview);
        let verb = if dry_run { "Would create" } else { "Created" };
        let kind = self.kind.trim_end_matches('s');
        let id = self.id.as_str();
        Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "text": format!("{verb} {kind} '{id}'. Review it with: retold {} check {id}", self.kind),
                "created": {"kind": kind, "id": id, "directory": self.directory()},
                "dry_run": dry_run,
                "next_command": ["retold", self.kind, "check", id],
            }),
        }
    }
}

fn ensure_resource_directory(path: &Path) -> Result<(), Diagnostic> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        let regular_directory = metadata.is_dir() && !metadata.file_type().is_symlink();
        if !regular_directory {
            return Err(invalid("Resource folders must be regular directories."));
        }
    }
    std::fs::create_dir_all(path).map_err(|_| failed("Could not create the resource directory."))
}

fn import_directory(source: &Path, filename: &str) -> Result<PathBuf, Diagnostic> {
    if source.is_dir() {
        return Ok(source.to_path_buf());
    }
    let correct_file = source.file_name().is_some_and(|name| name == filename);
    if !correct_file {
        return Err(invalid("Import a resource directory or its manifest file."));
    }
    source
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| invalid("Import source has no parent directory."))
}

fn invalid(message: impl AsRef<str>) -> Diagnostic {
    Diagnostic::new(ErrorCode::InputRequired, message.as_ref())
}

fn failed(message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCode::ExecutionFailed, message)
}
