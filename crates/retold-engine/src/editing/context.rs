//! Source stamps bind manual content to the frozen guide, inputs and recorded research/plan.
use std::collections::BTreeMap;

use retold_guide::Guide;

use super::{Baseline, EditError, MAX_DOCUMENT_BYTES};
use crate::{RunLayout, RunMeta, fsx, store, story::Script};

pub(super) fn guide(layout: &RunLayout) -> Result<Guide, EditError> {
    Guide::from_dir(&layout.guide_snapshot())
        .map_err(|error| EditError::Integrity(error.to_string()))
}

pub(super) fn stamp(layout: &RunLayout) -> Result<String, EditError> {
    let bytes = fsx::read_bounded(&layout.run_file(), MAX_DOCUMENT_BYTES)?
        .ok_or_else(|| EditError::Integrity("Run manifest is missing.".into()))?;
    let meta: RunMeta = super::files::decode(&bytes)?;
    let state = store::read_state(layout)?;
    let inputs: BTreeMap<_, _> = state
        .completed
        .iter()
        .filter(|(step, _)| store::stage_of(step) == "research" || step.as_str() == "plan")
        .collect();
    let revisions: BTreeMap<_, _> = state
        .revisions
        .iter()
        .filter(|(scope, _)| matches!(store::stage_of(scope), "research" | "script"))
        .collect();
    let guide = guide(layout)?;
    let channel = fsx::read_bounded(&layout.meta().join("channel.toml"), MAX_DOCUMENT_BYTES)?;
    let source = crate::research::saved_source_hash(layout)?;
    Ok(store::hash(&(
        meta.guide,
        meta.inputs,
        meta.quality,
        guide.files(),
        channel,
        source,
        inputs,
        revisions,
    ))?)
}

pub(super) fn require_current(layout: &RunLayout, base: &Baseline) -> Result<(), EditError> {
    if stamp(layout)? != base.context_hash {
        return Err(EditError::SourceChanged);
    }
    Ok(())
}

pub(super) fn quality(layout: &RunLayout, script: &Script) -> Result<Vec<String>, EditError> {
    let guide = guide(layout)?;
    Ok(crate::story::gates(script, guide.manifest())
        .into_iter()
        .map(|violation| violation.message)
        .collect())
}

pub(super) fn require_quality(layout: &RunLayout, script: &Script) -> Result<(), EditError> {
    let problems = quality(layout, script)?;
    if !problems.is_empty() {
        return Err(EditError::Invalid(problems.join("; ")));
    }
    Ok(())
}
