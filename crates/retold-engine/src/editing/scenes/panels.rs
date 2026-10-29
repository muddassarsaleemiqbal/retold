//! A changed batched sheet may affect neighboring frames even when their direction is unchanged.
use std::collections::BTreeSet;

use super::EditError;
use crate::{RunLayout, assets::Storyboard, store, story::Shot};

pub(super) fn dependents(
    layout: &RunLayout,
    boards: (&Storyboard, &Storyboard),
    changed: &[usize],
) -> Result<Vec<usize>, EditError> {
    let guide = crate::editing::context::guide(layout)?;
    let panels = usize::try_from(guide.manifest().visuals.panels)
        .map_err(|_| EditError::Invalid("Unsupported scene panel count.".into()))?;
    if panels < 2 || changed.is_empty() {
        return Ok(Vec::new());
    }
    let state = store::read_state(layout)?;
    let redone = boards
        .0
        .direction
        .shots
        .iter()
        .filter(|shot| state.revision(&format!("image-{}-0", shot.unit)) > 0)
        .map(|shot| shot.unit)
        .collect();
    Ok(group_changes(
        (&boards.0.direction.shots, &boards.1.direction.shots),
        panels,
        (redone, changed),
    ))
}

fn group_changes(
    shots: (&[Shot], &[Shot]),
    panels: usize,
    revisions: (BTreeSet<usize>, &[usize]),
) -> Vec<usize> {
    let (mut redone, changed) = revisions;
    let previous = crate::panels::groups(shots.0, panels, &|unit| redone.contains(&unit));
    redone.extend(changed);
    let next = crate::panels::groups(shots.1, panels, &|unit| redone.contains(&unit));
    let removed = previous.iter().filter(|group| !next.contains(group));
    let added = next.iter().filter(|group| !previous.contains(group));
    let grouped = removed.chain(added).flatten();
    let affected: BTreeSet<_> = grouped
        .copied()
        .filter(|unit| !changed.contains(unit))
        .collect();
    affected.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::group_changes;
    use std::collections::BTreeSet;
    const PANEL_COUNT: usize = 4;
    const SCENE_COUNT: usize = 9;
    const EXPECTED_AFFECTED: &[usize] = &[0, 2, 3, 4, 5, 6, 7];

    #[test]
    fn changing_one_panel_reports_the_other_frames_that_may_need_rebuilding() {
        let shots: Vec<_> = (0..SCENE_COUNT).map(|unit| serde_json::from_value(serde_json::json!({"unit":unit,"shot":"wide","location":"room","action":"waits","mood":"quiet","characters":[],"focus":[0.5,0.4]})).unwrap()).collect();
        let dependents = group_changes((&shots, &shots), PANEL_COUNT, (BTreeSet::default(), &[1]));
        assert_eq!(dependents, EXPECTED_AFFECTED);
        assert_eq!(
            group_changes((&shots, &shots), PANEL_COUNT, (BTreeSet::default(), &[])),
            Vec::<usize>::new()
        );
    }
}
