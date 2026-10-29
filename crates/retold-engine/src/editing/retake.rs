//! Targeted retakes: request a new version of one saved frame or of narration, reviewed first.
//! A retake changes only the named revision; the next reviewed run rebuilds its descendants.
use serde::Serialize;

use super::EditError;
use crate::{RunLayout, store};

/// The saved output to make again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Retake {
    /// One scene frame, by zero-based unit.
    Frame(usize),
    /// Narration for every edition.
    Narration,
}

/// Exact effect of a retake, bound to the revision it was computed from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RetakePlan {
    pub scope: String,
    pub revision: u64,
    pub affected_stages: Vec<String>,
}

fn scope(retake: &Retake) -> String {
    match retake {
        Retake::Frame(unit) => format!("image-{unit}-0"),
        Retake::Narration => "voice".into(),
    }
}

fn require_target(layout: &RunLayout, retake: &Retake) -> Result<(), EditError> {
    let Retake::Frame(unit) = retake else {
        return Ok(());
    };
    let scenes = super::scenes::snapshot(layout)?
        .ok_or_else(|| EditError::Invalid("This video has no saved scene sequence.".into()))?;
    let exists = scenes
        .board
        .direction
        .shots
        .iter()
        .any(|shot| shot.unit == *unit);
    if !exists {
        return Err(EditError::Invalid("Choose an existing scene.".into()));
    }
    Ok(())
}

/// Read-only review of a retake.
///
/// # Errors
/// Rejects unknown scenes and unreadable state.
pub fn plan(layout: &RunLayout, retake: &Retake) -> Result<RetakePlan, EditError> {
    require_target(layout, retake)?;
    let scope = scope(retake);
    let state = store::read_state(layout)?;
    let affected_stages = crate::graph::Graph::stages()
        .affected(store::stage_of(&scope))
        .into_iter()
        .collect();
    Ok(RetakePlan {
        revision: state.revision(&scope),
        scope,
        affected_stages,
    })
}

/// Publish a reviewed retake if its revision is unchanged; repeated requests do nothing extra.
///
/// # Errors
/// Rejects a changed revision, generation in progress and unknown scenes.
pub fn request(
    layout: &RunLayout,
    reviewed: &RetakePlan,
    retake: &Retake,
) -> Result<RetakePlan, EditError> {
    let mut store = store::Store::open(layout)?;
    let current = plan(layout, retake)?;
    if current.scope != reviewed.scope {
        return Err(EditError::Invalid(
            "The reviewed retake names another item.".into(),
        ));
    }
    let already = current.revision == reviewed.revision.saturating_add(1);
    if already {
        return Ok(current);
    }
    if current.revision != reviewed.revision {
        return Err(EditError::SceneChanged);
    }
    store.bump_revision(&current.scope)?;
    plan(layout, retake)
}
