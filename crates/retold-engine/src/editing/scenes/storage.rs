//! Scene pointers reuse the bounded immutable writer while keeping a separate document namespace.
use super::{Baseline, EditError, Revision};
use crate::editing::{
    EDITING_FORMAT_VERSION, MAX_DOCUMENT_BYTES, MAX_POINTER_BYTES, Pointer, files,
};
use crate::{RunLayout, Stage, fsx};

pub(super) fn pointer_bytes(layout: &RunLayout) -> Result<Option<Vec<u8>>, EditError> {
    files::check_existing_directory(&layout.meta())?;
    files::check_existing_directory(&files::directory(layout))?;
    Ok(fsx::read_bounded(
        &files::directory(layout).join("scenes.json"),
        MAX_POINTER_BYTES,
    )?)
}

pub(super) fn pointer(layout: &RunLayout) -> Result<Pointer, EditError> {
    files::decode_pointer(pointer_bytes(layout)?.as_deref())
}

pub(super) fn save_pointer(layout: &RunLayout, pointer: &Pointer) -> Result<(), EditError> {
    let bytes = files::encode(pointer, MAX_POINTER_BYTES)?;
    files::decode_pointer(Some(&bytes))?;
    files::prepare_directory(layout, "scene-revisions")?;
    fsx::write_atomic(&files::directory(layout).join("scenes.json"), &bytes)?;
    Ok(())
}

pub(super) fn baseline(layout: &RunLayout, id: &str) -> Result<Baseline, EditError> {
    let base: Baseline = files::read_object(layout, "scene-bases", id)?;
    if base.schema != EDITING_FORMAT_VERSION {
        return Err(EditError::Integrity(
            "Unsupported scene baseline schema.".into(),
        ));
    }
    Ok(base)
}

pub(super) fn revision(layout: &RunLayout, id: &str) -> Result<Revision, EditError> {
    let revision: Revision = files::read_object(layout, "scene-revisions", id)?;
    let valid_parent = revision
        .parent
        .as_ref()
        .is_none_or(|id| super::super::files::valid_hash(id));
    if revision.schema != EDITING_FORMAT_VERSION
        || !files::valid_hash(&revision.base)
        || !valid_parent
    {
        return Err(EditError::Integrity(
            "Invalid scene revision binding.".into(),
        ));
    }
    Ok(revision)
}

pub(super) fn require_binding(pointer: &Pointer, revision: &Revision) -> Result<(), EditError> {
    let same_base = pointer.base.as_deref() == Some(&revision.base);
    if !same_base {
        return Err(EditError::Integrity(
            "Scene revision belongs to another baseline.".into(),
        ));
    }
    Ok(())
}

pub(super) fn generated(layout: &RunLayout) -> Result<Option<Vec<u8>>, EditError> {
    Ok(fsx::read_bounded(
        &layout.stage(Stage::Storyboard).join("storyboard.json"),
        MAX_DOCUMENT_BYTES,
    )?)
}
