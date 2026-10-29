//! Semantic casting: who appears in one scene and which profile expression each one shows.
use super::mutation::{changed_units, current, impact, require_editable, save_unit};
use super::{ChangeImpact, EditError, SceneSnapshot, context};
use crate::{RunLayout, assets::Storyboard, story::CastPose};

/// A frame stays readable and within the reference-sheet budget with at most this many people.
pub const MAX_CAST: usize = 6;
/// Only a profile's first expressions have drawn poses on its reference sheet.
pub const PROFILE_EXPRESSIONS: usize = 5;

/// Cast changes preserve narration, visual text and every other scene field.
#[derive(Debug, Clone, Copy)]
pub struct CastEdit<'a> {
    pub expected_version: &'a str,
    pub unit: usize,
    pub cast: &'a [CastPose],
}

/// Shape checks only; profile membership and expressions are checked against the saved plan.
pub fn validate_cast(edit: &CastEdit<'_>) -> Result<(), EditError> {
    let crowded = edit.cast.len() > MAX_CAST;
    if crowded {
        return Err(EditError::Invalid(format!(
            "A scene can show at most {MAX_CAST} characters."
        )));
    }
    edit.cast
        .iter()
        .enumerate()
        .try_for_each(|(index, pose)| validate_pose(&edit.cast[..index], pose))
}

fn validate_pose(earlier: &[CastPose], pose: &CastPose) -> Result<(), EditError> {
    let duplicate = earlier.iter().any(|other| other.name == pose.name);
    if duplicate {
        return Err(EditError::Invalid(format!(
            "{} appears more than once in this scene.",
            pose.name
        )));
    }
    let incomplete = pose.name.trim().is_empty() || pose.expression.trim().is_empty();
    if incomplete {
        return Err(EditError::Invalid(
            "Choose a character and an expression for every cast entry.".into(),
        ));
    }
    Ok(())
}

fn require_profile_pose(board: &Storyboard, pose: &CastPose) -> Result<(), EditError> {
    let profile = board
        .plan
        .cast
        .iter()
        .find(|profile| profile.name == pose.name)
        .ok_or_else(|| EditError::Invalid(format!("{} is not in this video's cast.", pose.name)))?;
    let wanted = pose.expression.trim();
    let allowed = profile
        .emotions
        .iter()
        .take(PROFILE_EXPRESSIONS)
        .any(|emotion| emotion.trim().eq_ignore_ascii_case(wanted));
    if !allowed {
        return Err(EditError::Invalid(format!(
            "Choose one of {}'s profile expressions.",
            pose.name
        )));
    }
    Ok(())
}

fn edited_board(current: &SceneSnapshot, edit: CastEdit<'_>) -> Result<Storyboard, EditError> {
    validate_cast(&edit)?;
    let mut board = current.board.clone();
    for pose in edit.cast {
        require_profile_pose(&board, pose)?;
    }
    let shot = board
        .direction
        .shots
        .iter_mut()
        .find(|shot| shot.unit == edit.unit)
        .ok_or_else(|| EditError::Invalid("Choose an existing scene.".into()))?;
    let names: Vec<String> = edit.cast.iter().map(|pose| pose.name.clone()).collect();
    let same_people = shot.characters == names;
    let same_poses = shot.poses == edit.cast;
    if same_people && same_poses {
        return Ok(board);
    }
    shot.characters = names;
    shot.poses = edit.cast.to_vec();
    // A recut keeps the earlier frame's people, so a recast scene needs its own image.
    shot.reuse = None;
    Ok(board)
}

/// Exact visual descendants of a recast scene; read-only.
pub fn preview_cast(layout: &RunLayout, edit: CastEdit<'_>) -> Result<ChangeImpact, EditError> {
    let current = current(layout)?;
    require_editable(&current, edit.expected_version)?;
    let board = edited_board(&current, edit)?;
    context::require_quality(layout, &board)?;
    let changed = changed_units(&current.board, &board)?;
    let panels = super::panels::dependents(layout, (&current.board, &board), &changed)?;
    let latest = super::snapshot(layout)?.ok_or(EditError::SceneChanged)?;
    if latest.version != current.version {
        return Err(EditError::SceneChanged);
    }
    Ok(impact(&current.board, changed, panels))
}

pub fn save_cast(layout: &RunLayout, edit: CastEdit<'_>) -> Result<SceneSnapshot, EditError> {
    save_unit(layout, (edit.expected_version, edit.unit), |current| {
        edited_board(current, edit)
    })
}
