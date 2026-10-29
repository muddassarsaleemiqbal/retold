//! Bounded, hash-checked revisions and atomic current-pointer publication under the run lock.
use serde::{Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};

use super::{
    Baseline, EDITING_FORMAT_VERSION, EditError, HASH_LENGTH, MAX_DOCUMENT_BYTES,
    MAX_POINTER_BYTES, MAX_REDO, Pointer, Revision,
};
use crate::{EngineError, RunLayout, fsx};

pub(super) fn directory(layout: &RunLayout) -> PathBuf {
    layout.meta().join("edits")
}
fn pointer_path(layout: &RunLayout) -> PathBuf {
    directory(layout).join("script.json")
}

pub(super) fn pointer_bytes(layout: &RunLayout) -> Result<Option<Vec<u8>>, EditError> {
    check_existing_directory(&layout.meta())?;
    check_existing_directory(&directory(layout))?;
    Ok(fsx::read_bounded(&pointer_path(layout), MAX_POINTER_BYTES)?)
}

pub(super) fn pointer(layout: &RunLayout) -> Result<Pointer, EditError> {
    decode_pointer(pointer_bytes(layout)?.as_deref())
}

pub(super) fn decode_pointer(bytes: Option<&[u8]>) -> Result<Pointer, EditError> {
    let Some(bytes) = bytes else {
        return Ok(Pointer::default());
    };
    let pointer: Pointer = decode(bytes)?;
    validate_pointer(&pointer)?;
    Ok(pointer)
}

fn validate_pointer(pointer: &Pointer) -> Result<(), EditError> {
    let mut ids = pointer
        .base
        .iter()
        .chain(pointer.head.iter())
        .chain(pointer.approved_hash.iter())
        .chain(pointer.redo.iter());
    let valid_ids = ids.all(|id| valid_hash(id));
    let head_needs_base = pointer.head.is_some() && pointer.base.is_none();
    let orphan_approval = pointer.approved_hash.is_some() && pointer.head.is_none();
    let paired_approval = pointer.approved_hash.is_some() == pointer.approved_at.is_some();
    let bounded = pointer.redo.len() <= MAX_REDO;
    let invalid_binding = head_needs_base || orphan_approval || !paired_approval;
    let correct_format = pointer.schema == EDITING_FORMAT_VERSION && valid_ids;
    if !correct_format || !bounded || invalid_binding {
        return Err(EditError::Integrity("Invalid script pointer.".into()));
    }
    Ok(())
}

pub(super) fn baseline(layout: &RunLayout, id: &str) -> Result<Baseline, EditError> {
    let base: Baseline = read_object(layout, "bases", id)?;
    if base.schema != EDITING_FORMAT_VERSION {
        return Err(EditError::Integrity("Unsupported baseline schema.".into()));
    }
    Ok(base)
}

pub(super) fn revision(layout: &RunLayout, id: &str) -> Result<Revision, EditError> {
    let revision: Revision = read_object(layout, "revisions", id)?;
    let parent_valid = revision.parent.as_ref().is_none_or(|id| valid_hash(id));
    if revision.schema != EDITING_FORMAT_VERSION || !valid_hash(&revision.base) || !parent_valid {
        return Err(EditError::Integrity("Invalid script revision.".into()));
    }
    Ok(revision)
}

pub(super) fn check_binding(pointer: &Pointer, revision: &Revision) -> Result<(), EditError> {
    let same_base = pointer.base.as_deref() == Some(&revision.base);
    if !same_base {
        return Err(EditError::Integrity(
            "Revision belongs to another script baseline.".into(),
        ));
    }
    Ok(())
}

pub(super) fn read_object<T: DeserializeOwned + Serialize>(
    layout: &RunLayout,
    kind: &str,
    id: &str,
) -> Result<T, EditError> {
    if !valid_hash(id) {
        return Err(EditError::Integrity("Invalid revision identifier.".into()));
    }
    let parent = directory(layout).join(kind);
    check_existing_directory(&parent)?;
    let path = parent.join(format!("{id}.json"));
    let bytes = fsx::read_bounded(&path, MAX_DOCUMENT_BYTES)?
        .ok_or_else(|| EditError::Integrity("A referenced revision is missing.".into()))?;
    let value: T = decode(&bytes)?;
    if crate::store::hash(&value)? != id {
        return Err(EditError::Integrity(
            "Revision hash does not match its content.".into(),
        ));
    }
    Ok(value)
}

pub(super) fn save_object<T: Serialize>(
    layout: &RunLayout,
    kind: &str,
    value: &T,
) -> Result<String, EditError> {
    prepare_directory(layout, kind)?;
    let id = crate::store::hash(value)?;
    let path = directory(layout).join(kind).join(format!("{id}.json"));
    let bytes = encode(value, MAX_DOCUMENT_BYTES)?;
    if let Some(existing) = fsx::read_bounded(&path, MAX_DOCUMENT_BYTES)? {
        if existing != bytes {
            return Err(EditError::Integrity(
                "An immutable revision already has different bytes.".into(),
            ));
        }
        return Ok(id);
    }
    fsx::write_atomic(&path, &bytes)?;
    Ok(id)
}

pub(super) fn save_pointer(layout: &RunLayout, pointer: &Pointer) -> Result<(), EditError> {
    validate_pointer(pointer)?;
    prepare_directory(layout, "revisions")?;
    let bytes = encode(pointer, MAX_POINTER_BYTES)?;
    fsx::write_atomic(&pointer_path(layout), &bytes)?;
    Ok(())
}

pub(super) fn generated_bytes(layout: &RunLayout) -> Result<Option<Vec<u8>>, EditError> {
    Ok(fsx::read_bounded(
        &layout.stage(crate::Stage::Script).join("script.json"),
        MAX_DOCUMENT_BYTES,
    )?)
}

pub(super) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, EditError> {
    serde_json::from_slice(bytes)
        .map_err(|_| EditError::Integrity("Script document JSON does not match its schema.".into()))
}

pub(super) fn encode<T: Serialize>(value: &T, limit: u64) -> Result<Vec<u8>, EditError> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| EngineError::Setup(error.to_string()))?;
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if length > limit {
        return Err(EditError::Invalid(
            "The script revision exceeds its bounded document size.".into(),
        ));
    }
    Ok(bytes)
}

pub(super) fn prepare_directory(layout: &RunLayout, kind: &str) -> Result<(), EditError> {
    check_existing_directory(&layout.meta())?;
    ensure_directory(&directory(layout))?;
    ensure_directory(&directory(layout).join(kind))
}

fn ensure_directory(path: &Path) -> Result<(), EditError> {
    match std::fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            check_existing_directory(path)
        }
        Err(error) => Err(EngineError::io(path, error).into()),
    }
}

pub(super) fn check_existing_directory(path: &Path) -> Result<(), EditError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(EditError::Integrity(
            "Script revision folders must be regular directories.".into(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EngineError::io(path, error).into()),
    }
}

pub(super) fn valid_hash(id: &str) -> bool {
    id.len() == HASH_LENGTH && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}
