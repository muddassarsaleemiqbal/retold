//! Saved library views: a named search, sort, tag, channel and archive filter.
use std::path::Path;

use crate::library::ListOptions;
use crate::protocol::{Diagnostic, ErrorCode, Payload};
use crate::views::{self, ViewFilter, ViewRecord};

fn items(payload: Payload) -> Result<Vec<ViewRecord>, Diagnostic> {
    match payload {
        Payload::Result { contents, .. } => {
            serde_json::from_value(contents.get("items").cloned().unwrap_or_default())
                .map_err(|error| Diagnostic::new(ErrorCode::DataInvalid, &error.to_string()))
        }
        Payload::Error { diagnostic, .. } => Err(diagnostic),
        Payload::Event(_) => Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "Unexpected views response.",
        )),
    }
}

pub fn list(library: &Path) -> Result<Vec<ViewRecord>, Diagnostic> {
    items(views::list_views(library)?)
}

/// Save the current catalog filter under `name`, replacing a view of the same name.
pub fn save(
    library: &Path,
    name: &str,
    options: &ListOptions,
) -> Result<Vec<ViewRecord>, Diagnostic> {
    let filters = ViewFilter {
        channel: options.channel.clone(),
        search: options.search.clone(),
        tag: options.tag.clone(),
        archive: Some(options.archive),
        limit: options.limit,
        sort: options.sort.clone(),
        columns: Option::default(),
    };
    views::save_filter(library, name, &filters)?;
    list(library)
}

/// Catalog options for a saved view, starting from the first page.
#[must_use]
pub fn options(view: &ViewRecord) -> ListOptions {
    let filters = &view.filters;
    ListOptions {
        channel: filters.channel.clone(),
        search: filters.search.clone(),
        tag: filters.tag.clone(),
        sort: filters.sort.clone(),
        view: Some(view.name.clone()),
        limit: filters.limit,
        cursor: Option::default(),
        archive: filters.archive.unwrap_or_default(),
    }
}

/// Remove a saved view after the client's explicit confirmation.
pub fn remove(library: &Path, name: &str) -> Result<Vec<ViewRecord>, Diagnostic> {
    views::remove_view(library, name, views::RemovalConsent::Confirmed)?;
    list(library)
}
