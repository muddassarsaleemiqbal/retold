//! Typed pages use the CLI's existing filters and bind cursors to the complete query.
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{
    Catalog, CatalogIssue, ListOptions, Video, cursor_offset, invalid_snapshot, matches_filter,
    sorted_videos,
};
use crate::protocol::Diagnostic;

#[derive(Debug, Clone, Serialize)]
pub struct VideoPage {
    pub items: Vec<Video>,
    pub total: usize,
    pub snapshot_id: String,
    pub next_cursor: Option<String>,
    pub complete: bool,
    pub issues: Vec<CatalogIssue>,
    pub library: PathBuf,
}

pub fn page(root: &Path, options: &ListOptions) -> Result<VideoPage, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    catalog.project_page(options)
}

impl Catalog {
    pub(super) fn project_page(&self, options: &ListOptions) -> Result<VideoPage, Diagnostic> {
        let effective = options.with_resolved_view(self)?;
        let limit = effective.validate_limit()?;
        let selected: Vec<_> = self
            .videos
            .iter()
            .filter(|video| matches_filter(video, &effective))
            .collect();
        let sorted = sorted_videos(selected, effective.sort.as_deref());
        let snapshot = page_identity(self, &effective)?;
        let total = sorted.len();
        let offset = cursor_offset(effective.cursor.as_deref(), &snapshot, total)?;
        let end = offset.saturating_add(limit).min(total);
        Ok(VideoPage {
            items: sorted[offset..end]
                .iter()
                .map(|video| (*video).clone())
                .collect(),
            total,
            snapshot_id: snapshot.clone(),
            next_cursor: (end < total).then(|| format!("v1:{snapshot}:{end}")),
            complete: self.issues.is_empty(),
            issues: self.issues.clone(),
            library: self.root.clone(),
        })
    }
}

fn page_identity(catalog: &Catalog, options: &ListOptions) -> Result<String, Diagnostic> {
    let mut query = options.clone();
    query.cursor = Option::default();
    retold_engine::store::hash(&(catalog, query)).map_err(|_| invalid_snapshot())
}
