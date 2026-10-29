//! Source stamps distinguish semantic input changes from approval bookkeeping and media progress.
use super::{Baseline, EditError};
use crate::{RunLayout, assets::Storyboard, store};

pub(super) fn stamp(layout: &RunLayout) -> Result<String, EditError> {
    let script = crate::editing::snapshot::script(layout)?.map(|script| script.content_hash);
    let state = store::read_state(layout)?;
    let revisions: std::collections::BTreeMap<_, _> = state
        .revisions
        .iter()
        .filter(|(scope, _)| store::stage_of(scope) == "storyboard")
        .collect();
    Ok(store::hash(&(
        production_id(layout)?,
        crate::editing::context::stamp(layout)?,
        script,
        state.completed.get("storyboard"),
        revisions,
    ))?)
}

fn production_id(layout: &RunLayout) -> Result<String, EditError> {
    let bytes = crate::fsx::read_bounded(&layout.run_file(), crate::editing::MAX_DOCUMENT_BYTES)?
        .ok_or_else(|| {
        EditError::Integrity("The scene production manifest is missing.".into())
    })?;
    let meta: crate::RunMeta = crate::editing::files::decode(&bytes)?;
    Ok(meta.id)
}

pub(super) fn visual_stamp(layout: &RunLayout) -> Result<String, EditError> {
    let state = store::read_state(layout)?;
    let revisions: std::collections::BTreeMap<_, _> = state
        .revisions
        .iter()
        .filter(|(scope, _)| store::stage_of(scope) == "images")
        .collect();
    Ok(store::hash(&revisions)?)
}

pub(super) fn require_current(layout: &RunLayout, base: &Baseline) -> Result<(), EditError> {
    if stamp(layout)? != base.context_hash {
        return Err(EditError::SceneSourceChanged);
    }
    Ok(())
}

pub(super) fn quality(layout: &RunLayout, board: &Storyboard) -> Result<Vec<String>, EditError> {
    let guide = crate::editing::context::guide(layout)?;
    Ok(crate::story::direction_gates(
        &board.direction,
        &board.units,
        &board.plan,
        guide.manifest(),
    ))
}

pub(super) fn require_quality(layout: &RunLayout, board: &Storyboard) -> Result<(), EditError> {
    let problems = quality(layout, board)?;
    if !problems.is_empty() {
        return Err(EditError::Invalid(problems.join("; ")));
    }
    Ok(())
}

pub(super) fn require_record(layout: &RunLayout) -> Result<(), EditError> {
    let inspected = store::inspection::inspect(layout)?;
    let verified = inspected.steps.iter().any(|step| {
        step.step == "storyboard"
            && step.integrity == store::inspection::Integrity::EnvelopeVerified
    });
    if !verified {
        return Err(EditError::Integrity("The generated scene cache is missing or invalid. Keep manual scene history and repair it before continuing.".into()));
    }
    Ok(())
}
