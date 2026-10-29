//! Library queries over bounded observed metadata snapshots, without execution authority.

use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use retold_engine::RunLayout;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod paging;
pub use paging::{VideoPage, page};

pub const DEFAULT_PAGE_SIZE: usize = 100;
pub const MAX_PAGE_SIZE: usize = 500;
const MAX_CANDIDATES: usize = 20;

pub fn default_library() -> Result<PathBuf, Diagnostic> {
    retold_engine::Paths::discover().map(|paths| paths.library)
        .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, "Library settings could not be read. Supply --library PATH or repair the settings file."))
}

#[derive(Debug, Clone, Serialize)]
pub struct Video {
    pub id: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub title: String,
    pub channel: String,
    pub guide: String,
    pub created: String,
    pub editions: Vec<String>,
    pub metadata_hash: String,
    pub is_archived: bool,
    pub tags: Vec<String>,
    #[serde(skip)]
    pub(crate) timestamp: jiff::Timestamp,
    #[serde(skip)]
    pub(crate) directory: PathBuf,
    #[serde(skip)]
    pub(crate) layout: RunLayout,
    /// The per-video safety cap recorded at creation.
    #[serde(skip)]
    pub(crate) budget: retold_engine::gate::Usd,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogIssue {
    pub directory: PathBuf,
    pub code: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Catalog {
    pub root: PathBuf,
    pub videos: Vec<Video>,
    pub issues: Vec<CatalogIssue>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ListOptions {
    pub channel: Option<String>,
    pub search: Option<String>,
    pub tag: Option<String>,
    pub sort: Option<String>,
    pub view: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    pub archive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagOperation<'a> {
    Add(&'a str),
    Remove(&'a str),
}

#[derive(Debug, Clone)]
pub enum Query<'a> {
    List(ListOptions),
    Show(&'a str),
    Plan(&'a str, crate::plan::PlanOptions),
    Costs(&'a str, crate::video::CostsOptions<'a>),
    Review(&'a str, crate::review::ReviewFilter),
    Approve(&'a str, Option<&'a str>),
    Unlock(&'a str, Option<&'a str>),
    Note(&'a str, crate::review::NoteSpec<'a>),
    EditShot(&'a str, crate::review::ShotEditSpec<'a>),
    EditLine(&'a str, crate::review::LineEditSpec<'a>),
    EditCast(&'a str, crate::review::CastEditSpec<'a>),
    EditBudget(&'a str, &'a str),
    Undo(&'a str),
    Redo(&'a str, crate::review::RedoSpec<'a>),
    Copy(&'a str, crate::video::CopySpec<'a>),
    Archive(&'a str),
    Unarchive(&'a str),
    Delete(&'a str, crate::video::DeleteMode),
    Tag(&'a str, TagOperation<'a>),
    History(&'a str, Option<&'a str>),
    Paths(&'a str, crate::packaging::PathsOptions<'a>),
    Verify(&'a str, crate::packaging::VerifyOptions<'a>),
    Package(&'a str, crate::packaging::PackageOptions<'a>),
    Clean(&'a str, crate::packaging::CleanOptions<'a>),
}

pub fn query(root: &Path, query: Query<'_>) -> Payload {
    let result = crate::discovery::scan(root).and_then(|catalog| dispatch_query(&catalog, query));
    result.unwrap_or_else(Diagnostic::into_payload)
}

fn dispatch_query(catalog: &Catalog, query: Query<'_>) -> Result<Payload, Diagnostic> {
    match query {
        Query::List(options) => options.page(catalog),
        Query::Show(reference) => crate::video::show(catalog, reference),
        Query::Plan(reference, options) => crate::video::plan(catalog, reference, &options),
        Query::Costs(reference, options) => crate::video::costs(catalog, reference, &options),
        _ => dispatch_review_query(catalog, query),
    }
}

fn dispatch_review_query(catalog: &Catalog, query: Query<'_>) -> Result<Payload, Diagnostic> {
    match query {
        Query::Review(reference, filter) => {
            crate::review::review_summary(catalog, reference, &filter)
        }
        Query::Approve(reference, target) => crate::review::approve(catalog, reference, target),
        Query::Unlock(reference, target) => crate::review::unlock(catalog, reference, target),
        Query::Note(reference, spec) => crate::review::note(catalog, reference, spec),
        Query::EditShot(reference, spec) => crate::review::edit_shot(catalog, reference, spec),
        Query::EditLine(reference, spec) => crate::review::edit_line(catalog, reference, spec),
        Query::EditCast(reference, spec) => crate::review::edit_cast(catalog, reference, spec),
        Query::EditBudget(reference, budget) => {
            crate::review::edit_budget(catalog, reference, budget)
        }
        Query::Undo(reference) => crate::review::undo(catalog, reference),
        Query::Redo(reference, spec) => crate::review::redo(catalog, reference, spec),
        Query::History(reference, kind) => crate::review::video_history(catalog, reference, kind),
        _ => dispatch_management_query(catalog, query),
    }
}

fn dispatch_management_query(catalog: &Catalog, query: Query<'_>) -> Result<Payload, Diagnostic> {
    match query {
        Query::Copy(reference, spec) => crate::video::copy(catalog, reference, &spec),
        Query::Archive(reference) => crate::video::archive(catalog, reference),
        Query::Unarchive(reference) => crate::video::unarchive(catalog, reference),
        Query::Delete(reference, mode) => crate::video::delete(catalog, reference, mode),
        _ => dispatch_tag_and_delivery_query(catalog, query),
    }
}

fn dispatch_tag_and_delivery_query(
    catalog: &Catalog,
    query: Query<'_>,
) -> Result<Payload, Diagnostic> {
    match query {
        Query::Tag(reference, operation) => crate::video::tag_video(catalog, reference, operation),
        Query::Paths(reference, options) => {
            crate::packaging::video_paths(catalog, reference, &options)
        }
        Query::Verify(reference, options) => {
            crate::packaging::video_verify(catalog, reference, &options)
        }
        _ => dispatch_package_and_clean_query(catalog, query),
    }
}

fn dispatch_package_and_clean_query(
    catalog: &Catalog,
    query: Query<'_>,
) -> Result<Payload, Diagnostic> {
    match query {
        Query::Package(reference, options) => {
            crate::packaging::video_package(catalog, reference, &options)
        }
        Query::Clean(reference, options) => {
            crate::packaging::video_clean(catalog, reference, &options)
        }
        _ => Err(Diagnostic::new(
            ErrorCode::NotAvailable,
            "Unsupported query variant.",
        )),
    }
}

impl Catalog {
    pub(crate) fn sorted(mut self) -> Result<Self, Diagnostic> {
        self.videos.sort_by(|left, right| {
            right
                .timestamp
                .cmp(&left.timestamp)
                .then(left.id.cmp(&right.id))
                .then(left.reference.cmp(&right.reference))
        });
        self.issues
            .sort_by(|left, right| left.directory.cmp(&right.directory));
        let ids: BTreeSet<_> = self.videos.iter().map(|video| &video.id).collect();
        let duplicate_ids = ids.len() != self.videos.len();
        if duplicate_ids {
            return Err(Diagnostic::new(
                ErrorCode::DataInvalid,
                "Duplicate immutable video IDs were found. Resolve the duplicate folders before continuing.",
            ));
        }
        Ok(self)
    }

    pub(crate) fn resolve(&self, reference: &str) -> Result<&Video, Diagnostic> {
        self.ensure_resolvable(reference)?;
        if reference == "last" {
            return one_match(&self.videos.first().into_iter().collect::<Vec<_>>());
        }
        let ids: Vec<_> = self
            .videos
            .iter()
            .filter(|video| video.id == reference)
            .collect();
        if !ids.is_empty() {
            return one_match(&ids);
        }
        self.resolve_alias(reference)
    }

    fn ensure_resolvable(&self, reference: &str) -> Result<(), Diagnostic> {
        let incomplete = !self.issues.is_empty();
        if incomplete {
            return Err(Diagnostic::new(
                ErrorCode::LibraryIncomplete,
                "Some run manifests could not be inspected. Use video list to locate them before resolving a video.",
            ));
        }
        if reference.is_empty() {
            return Err(Diagnostic::new(
                ErrorCode::InputRequired,
                "Supply an explicit video reference.",
            ));
        }
        Ok(())
    }

    fn resolve_alias(&self, reference: &str) -> Result<&Video, Diagnostic> {
        let exact: Vec<_> = self
            .videos
            .iter()
            .filter(|video| aliases(video).contains(&reference))
            .collect();
        if !exact.is_empty() {
            return one_match(&exact);
        }
        one_match(
            &self
                .videos
                .iter()
                .filter(|video| {
                    video.id.starts_with(reference)
                        || aliases(video)
                            .iter()
                            .any(|alias| alias.starts_with(reference))
                })
                .collect::<Vec<_>>(),
        )
    }
}

const ALIAS_COUNT: usize = 3;

fn aliases(video: &Video) -> [&str; ALIAS_COUNT] {
    let local = video
        .reference
        .split_once('/')
        .map_or(video.reference.as_str(), |(_, path)| path);
    let slug = video.reference.rsplit('/').next().unwrap_or_default();
    [&video.reference, local, slug]
}

fn one_match<'a>(candidates: &[&'a Video]) -> Result<&'a Video, Diagnostic> {
    match candidates {
        [video] => Ok(*video),
        [] => Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            "No video matches that reference. Run video list.",
        )),
        _ => {
            let mut diagnostic = Diagnostic::new(
                ErrorCode::RefAmbiguous,
                "The reference matches multiple videos. Use a full reference or immutable ID.",
            );
            diagnostic.context = serde_json::json!({"candidates": candidates.iter().take(MAX_CANDIDATES).map(|video| &video.reference).collect::<Vec<_>>(), "total": candidates.len()});
            Err(diagnostic)
        }
    }
}

fn matches_filter(video: &Video, options: &ListOptions) -> bool {
    let archive_matched = options.archive == video.is_archived;
    let channel = options
        .channel
        .as_ref()
        .is_none_or(|channel| *channel == video.channel);
    let tag_matched = options
        .tag
        .as_ref()
        .is_none_or(|target_tag| video.tags.contains(target_tag));
    let search = options.search.as_ref().is_none_or(|search| {
        let needle = search.to_lowercase();
        [&video.title, &video.reference, &video.id]
            .iter()
            .any(|value| value.to_lowercase().contains(&needle))
    });
    archive_matched && channel && tag_matched && search
}

fn sorted_videos<'a>(mut videos: Vec<&'a Video>, sort_mode: Option<&str>) -> Vec<&'a Video> {
    match sort_mode {
        Some("title") => videos.sort_by(|left, right| left.title.cmp(&right.title)),
        Some("channel") => videos.sort_by(|left, right| left.channel.cmp(&right.channel)),
        _ => {}
    }
    videos
}

fn merge_view_filters(mut options: ListOptions, filters: crate::views::ViewFilter) -> ListOptions {
    let has_channel = options.channel.is_some();
    if !has_channel {
        options.channel = filters.channel;
    }
    let has_search = options.search.is_some();
    if !has_search {
        options.search = filters.search;
    }
    let has_tag = options.tag.is_some();
    if !has_tag {
        options.tag = filters.tag;
    }
    let has_limit = options.limit.is_some();
    if !has_limit {
        options.limit = filters.limit;
    }
    let has_sort = options.sort.is_some();
    if !has_sort {
        options.sort = filters.sort;
    }
    if !options.archive {
        options.archive = filters.archive.unwrap_or(false);
    }
    options
}

fn cursor_offset(cursor: Option<&str>, snapshot: &str, total: usize) -> Result<usize, Diagnostic> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let parsed = cursor
        .strip_prefix("v1:")
        .and_then(|suffix| suffix.split_once(':'));
    let (identity, offset) = parsed.ok_or_else(invalid_cursor)?;
    let offset = offset.parse::<usize>().map_err(|_| invalid_cursor())?;
    if identity != snapshot || offset >= total {
        return Err(invalid_cursor());
    }
    Ok(offset)
}

pub(crate) const fn response(contents: serde_json::Value) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents,
    }
}

fn invalid_snapshot() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::DataInvalid,
        "Could not fingerprint the library snapshot.",
    )
}

impl ListOptions {
    fn with_resolved_view(&self, catalog: &Catalog) -> Result<Self, Diagnostic> {
        let Some(view_name) = &self.view else {
            return Ok(self.clone());
        };
        let view_record = crate::views::load_view_record(&catalog.root, view_name)?;
        Ok(merge_view_filters(self.clone(), view_record.filters))
    }

    fn validate_limit(&self) -> Result<usize, Diagnostic> {
        let limit = self.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        if (1..=MAX_PAGE_SIZE).contains(&limit) {
            Ok(limit)
        } else {
            Err(Diagnostic::new(
                ErrorCode::InputRequired,
                "Page size must be between 1 and 500.",
            ))
        }
    }

    fn page(&self, catalog: &Catalog) -> Result<Payload, Diagnostic> {
        let page = catalog.project_page(self)?;
        let contents = serde_json::to_value(page).map_err(|_| invalid_snapshot())?;
        Ok(response(contents))
    }
}

fn invalid_cursor() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::CursorInvalid,
        "This page cursor is invalid or the library or filters changed. Restart the search from the first page, or run video list without --cursor.",
    )
}

const MAX_TAIL_EVENTS: usize = 30;

/// Recent events recorded in the video layout's events.log.
pub fn video_events(root: &Path, reference: &str) -> Vec<String> {
    let Ok(catalog) = crate::discovery::scan(root) else {
        return Vec::new();
    };
    let Ok(video) = catalog.resolve(reference) else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(video.layout.events_log()) else {
        return Vec::new();
    };
    content
        .lines()
        .rev()
        .take(MAX_TAIL_EVENTS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(String::from)
        .collect()
}
