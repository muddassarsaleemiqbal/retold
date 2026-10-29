//! Export of deliveries saved at `ready`. Only the editions and formats a request names are
//! opened in the editor; nothing upstream is generated again, and the saved manifest's own
//! approval flag grants nothing.

use std::collections::BTreeSet;

use retold_filmcraft::timeline::Timeline;
use serde::Serialize;

use super::{MANIFEST_LIMIT, Request, SessionError, context, drive, revalidate};
use crate::store::{Store, read_state};
use crate::workflow::{self, Delivery, Manifest};
use crate::{EngineError, RunLayout, fsx};

/// The step that commits every assembled timeline at `ready`.
const TIMELINE_STEP: &str = "timeline";

/// One assembled delivery saved at `ready`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreparedDelivery {
    pub id: String,
    pub edition: String,
    /// Aspect as a format name, such as `9x16`.
    pub format: String,
}

/// What an export request may open, render or save.
#[derive(Debug, Clone)]
pub struct Selection {
    pub editions: BTreeSet<String>,
    pub formats: BTreeSet<String>,
    /// Save the editor project only; render nothing.
    pub project_only: bool,
}

impl Selection {
    fn covers(&self, delivery: &PreparedDelivery) -> bool {
        self.editions.contains(&delivery.edition) && self.formats.contains(&delivery.format)
    }

    /// Every requested edition and format must exist; a selection never silently shrinks.
    fn satisfied_by(&self, chosen: &[PreparedDelivery]) -> bool {
        let editions: BTreeSet<_> = chosen.iter().map(|item| item.edition.clone()).collect();
        let formats: BTreeSet<_> = chosen.iter().map(|item| item.format.clone()).collect();
        editions == self.editions && formats == self.formats
    }
}

/// Read-only: deliveries saved at `ready`, or none when the video has not reached it.
pub fn prepared_deliveries(layout: &RunLayout) -> Result<Vec<PreparedDelivery>, EngineError> {
    let ready = read_state(layout)?.completed.contains_key(TIMELINE_STEP);
    if !ready {
        return Ok(Vec::new());
    }
    let manifest = saved_manifest(layout)?;
    Ok(manifest
        .map(|manifest| manifest.editions.iter().map(prepared).collect())
        .unwrap_or_default())
}

/// Exports exactly `selection` from the committed ready timelines, under the run lock.
pub fn export(request: &Request<'_>, selection: &Selection) -> Result<(), SessionError> {
    super::ensure_running(request)?;
    let mut store = Store::open(request.layout)?;
    let meta = revalidate(request.layout, request.expected)?;
    let key = committed_key(&store)?;
    let committed: Vec<Timeline> = store
        .cached(TIMELINE_STEP, &key)?
        .ok_or(SessionError::NotReady)?;
    let (timelines, manifest) = selected(&committed, request.layout, selection)?;
    let context = context(request, meta, store)?;
    let options = workflow::Options {
        allow_video: true,
        project_only: selection.project_only,
        filmcraft_binary: request.editor.clone(),
        ..workflow::Options::default()
    };
    drive(context, |context| async move {
        let result = workflow::export_prepared(&context, &timelines, &manifest, &options).await;
        record_outcome(&context, &result).await?;
        result
    })
}

/// Saved state shows whether the export finished, as a workflow run's does.
async fn record_outcome(
    context: &crate::context::Context,
    result: &Result<(), workflow::WorkflowError>,
) -> Result<(), EngineError> {
    let mut store = context.media.store.lock().await;
    match result {
        Ok(()) => store.finish(),
        Err(error) => store.fail(&error.to_string()),
    }
}

fn committed_key(store: &Store) -> Result<String, SessionError> {
    let record = store.state.completed.get(TIMELINE_STEP);
    record
        .map(|record| record.key.clone())
        .ok_or(SessionError::NotReady)
}

/// The committed timelines and manifest entries for the selection, approved by this request.
fn selected(
    committed: &[Timeline],
    layout: &RunLayout,
    selection: &Selection,
) -> Result<(Vec<Timeline>, Manifest), SessionError> {
    let mut manifest = saved_manifest(layout)?.ok_or(SessionError::NotReady)?;
    manifest
        .editions
        .retain(|delivery| selection.covers(&prepared(delivery)));
    let chosen: Vec<_> = manifest.editions.iter().map(prepared).collect();
    if !selection.satisfied_by(&chosen) {
        return Err(SessionError::NotReady);
    }
    let timelines = matching(committed, &manifest)?;
    manifest.video_approved = true;
    Ok((timelines, manifest))
}

fn matching(timelines: &[Timeline], manifest: &Manifest) -> Result<Vec<Timeline>, SessionError> {
    manifest
        .editions
        .iter()
        .map(|delivery| {
            timelines
                .iter()
                .find(|timeline| timeline.name == delivery.id)
                .cloned()
                .ok_or(SessionError::NotReady)
        })
        .collect()
}

fn saved_manifest(layout: &RunLayout) -> Result<Option<Manifest>, EngineError> {
    let path = layout.deliverables().join("manifest.json");
    let bytes = fsx::read_bounded(&path, MANIFEST_LIMIT)?;
    bytes
        .map(|bytes| {
            serde_json::from_slice(&bytes).map_err(|error| EngineError::Config {
                path: path.clone(),
                message: error.to_string(),
            })
        })
        .transpose()
}

fn prepared(delivery: &Delivery) -> PreparedDelivery {
    PreparedDelivery {
        id: delivery.id.clone(),
        edition: delivery.edition.clone(),
        format: delivery.aspect.replace(':', "x"),
    }
}
