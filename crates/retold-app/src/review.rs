//! Video review, approvals, lock states, edits, notes, redo, and undo.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::Catalog;
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

const REVIEWS_FILE: &str = "meta/reviews.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewNote {
    pub id: String,
    pub item: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemApproval {
    pub item: String,
    pub content_hash: String,
    pub approved_at: String,
    pub locked: bool,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditHistoryEntry {
    pub revision: usize,
    pub kind: String,
    pub target: String,
    pub previous_content: serde_json::Value,
    pub updated_content: serde_json::Value,
    pub timestamp: String,
    pub invalidated_items: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewStore {
    pub approvals: BTreeMap<String, ItemApproval>,
    pub notes: BTreeMap<String, Vec<ReviewNote>>,
    pub history: Vec<EditHistoryEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct ReviewFilter {
    pub pending: bool,
    pub stale: bool,
    pub failed: bool,
    pub lang: Option<String>,
    pub take: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ShotEditSpec<'a> {
    pub frame: usize,
    pub action: Option<&'a str>,
    pub staging: Option<&'a str>,
    pub characters: Option<&'a str>,
    pub shot_type: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LineEditSpec<'a> {
    pub line: usize,
    pub lang: Option<&'a str>,
    pub text: Option<&'a str>,
    pub spoken_only: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CastEditSpec<'a> {
    pub character: &'a str,
    pub look: &'a str,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoteSpec<'a> {
    pub item: &'a str,
    pub text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedoMode {
    DryRun,
    Commit,
}

#[derive(Debug, Clone, Copy)]
pub struct RedoSpec<'a> {
    pub stage: &'a str,
    pub items: Option<&'a str>,
    pub mode: RedoMode,
}

#[derive(Debug, Clone, Copy)]
struct BudgetHistorySpec<'a> {
    budget_usd: &'a str,
    previous: &'a str,
}

fn review_file_path(video_dir: &Path) -> PathBuf {
    video_dir.join(REVIEWS_FILE)
}

fn load_store(video_dir: &Path) -> ReviewStore {
    let path = review_file_path(video_dir);
    let Ok(bytes) = fs::read(&path) else {
        return ReviewStore::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn persist_store(video_dir: &Path, store: &ReviewStore) -> Result<(), Diagnostic> {
    let path = review_file_path(video_dir);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(store)
        .map_err(|err| Diagnostic::new(ErrorCode::DataInvalid, &err.to_string()))?;
    fs::write(&path, json)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))
}

const fn next_revision(history: &[EditHistoryEntry]) -> usize {
    history.len().saturating_add(1)
}

fn invalidate_store(mut store: ReviewStore, items: &[String]) -> ReviewStore {
    for item in items {
        let entry = store.approvals.get_mut(item);
        if let Some(approval) = entry {
            approval.stale = true;
        }
    }
    store
}

fn record_approval(mut store: ReviewStore, item_key: &str, approval: ItemApproval) -> ReviewStore {
    store.approvals.insert(item_key.to_string(), approval);
    store
}

fn record_history_entry(mut store: ReviewStore, entry: EditHistoryEntry) -> ReviewStore {
    store.history.push(entry);
    store
}

pub(crate) fn review_summary(
    catalog: &Catalog,
    reference: &str,
    filter: &ReviewFilter,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let store = crate::notes::merge_saved(video, load_store(&video.directory))?;
    let items = collect_review_items(&store, filter);
    let counts = extract_summary_counts(&store, items.len());
    Ok(build_summary_payload(&video.reference, &items, counts))
}

fn extract_summary_counts(store: &ReviewStore, total: usize) -> SummaryCounts {
    SummaryCounts {
        total,
        notes_count: store.notes.values().map(Vec::len).sum(),
        edits_count: store.history.len(),
    }
}

#[derive(Debug, Clone, Copy)]
struct SummaryCounts {
    total: usize,
    notes_count: usize,
    edits_count: usize,
}

fn build_summary_payload(
    reference: &str,
    items: &[ItemApproval],
    counts: SummaryCounts,
) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "video": reference,
            "items": items,
            "total": counts.total,
            "notes_count": counts.notes_count,
            "edits_count": counts.edits_count,
        }),
    }
}

fn collect_review_items(store: &ReviewStore, filter: &ReviewFilter) -> Vec<ItemApproval> {
    store
        .approvals
        .values()
        .filter(|approval| {
            let filter_stale = filter.stale;
            let item_stale = approval.stale;
            let skip_not_stale = filter_stale && !item_stale;

            let filter_pending = filter.pending;
            let item_locked = approval.locked;
            let skip_locked = filter_pending && item_locked;

            !skip_not_stale && !skip_locked
        })
        .cloned()
        .collect()
}

fn create_approval(item_key: &str, now: &str) -> ItemApproval {
    ItemApproval {
        item: item_key.to_string(),
        content_hash: format!("hash-{item_key}"),
        approved_at: now.to_string(),
        locked: true,
        stale: false,
    }
}

pub(crate) fn approve(
    catalog: &Catalog,
    reference: &str,
    target: Option<&str>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    if let Some(selection) = character_selection(video, target) {
        return approve_characters(video, &selection);
    }
    let store = load_store(&video.directory);
    let now = jiff::Timestamp::now().to_string();
    let item_key = target.unwrap_or("all");
    let approval = create_approval(item_key, &now);

    let updated_store = record_approval(store, item_key, approval);
    persist_store(&video.directory, &updated_store)?;
    let text = format!("Approved and locked '{item_key}' for {}.", video.reference);
    Ok(build_approve_payload(&text, item_key))
}

/// Character sheets are approved by content hash, which is what lets production continue.
fn character_selection(
    video: &crate::library::Video,
    target: Option<&str>,
) -> Option<std::path::PathBuf> {
    let target = target?;
    if target == "characters" {
        return Some(std::path::PathBuf::new());
    }
    let path = video.directory.join(target);
    retold_engine::character_approval::is_sheet(&video.layout, &path).then_some(path)
}

fn approve_characters(
    video: &crate::library::Video,
    sheet: &std::path::Path,
) -> Result<Payload, Diagnostic> {
    use retold_engine::character_approval::{Selection, approve};
    let _writer = retold_engine::store::Store::open(&video.layout)
        .map_err(|error| Diagnostic::new(ErrorCode::InputRequired, &error.to_string()))?;
    let all_sheets = sheet.as_os_str().is_empty();
    let selection = if all_sheets {
        Selection::All
    } else {
        Selection::Sheet(sheet)
    };
    let names = approve(&video.layout, selection)
        .map_err(|error| Diagnostic::new(ErrorCode::InputRequired, &error.to_string()))?;
    let text = format!(
        "Approved character sheets for {}: {}. Plan and continue the video to make scene images.",
        video.reference,
        names.join(", ")
    );
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({"text": text, "item": "characters", "approved": names, "locked": true}),
    })
}

fn build_approve_payload(text: &str, item_key: &str) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "item": item_key,
            "locked": true,
        }),
    }
}

pub(crate) fn unlock(
    catalog: &Catalog,
    reference: &str,
    target: Option<&str>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let mut store = load_store(&video.directory);
    let item_key = target.unwrap_or("all");

    let entry = store.approvals.get_mut(item_key);
    match entry {
        Some(approval) => approval.locked = false,
        None => {
            store.approvals.remove(item_key);
        }
    }
    persist_store(&video.directory, &store)?;
    let text = format!("Unlocked '{item_key}' for {}.", video.reference);
    Ok(build_unlock_payload(&text, item_key))
}

fn build_unlock_payload(text: &str, item_key: &str) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "item": item_key,
            "locked": false,
        }),
    }
}

pub(crate) fn note(
    catalog: &Catalog,
    reference: &str,
    spec: NoteSpec<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let query = crate::notes::Query::default();
    let current = crate::notes::document(&catalog.root, reference, &query)?;
    let id = crate::notes::draft_id()?;
    let saved = crate::notes::append(
        &catalog.root,
        reference,
        crate::notes::Append {
            expected_version: &current.version,
            id: &id,
            item: spec.item,
            text: spec.text,
        },
    )?;
    let text = format!("Attached note to '{}' for {}.", spec.item, video.reference);
    Ok(build_note_payload(&text, &saved.note))
}

fn build_note_payload(text: &str, note: &ReviewNote) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "note": note,
        }),
    }
}

fn shot_history_entry(revision: usize, spec: ShotEditSpec<'_>, now: String) -> EditHistoryEntry {
    let target = format!("frame-{}", spec.frame);
    let invalidated = vec![format!("images/{}", spec.frame), "assembly".into()];
    EditHistoryEntry {
        revision,
        kind: "shot".into(),
        target,
        previous_content: serde_json::json!({"frame": spec.frame}),
        updated_content: serde_json::json!({
            "frame": spec.frame,
            "action": spec.action,
            "staging": spec.staging,
            "characters": spec.characters,
            "shot_type": spec.shot_type,
        }),
        timestamp: now,
        invalidated_items: invalidated,
    }
}

pub(crate) fn edit_shot(
    catalog: &Catalog,
    reference: &str,
    spec: ShotEditSpec<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let store = load_store(&video.directory);
    let now = jiff::Timestamp::now().to_string();
    let revision = next_revision(&store.history);
    let entry = shot_history_entry(revision, spec, now);
    let updated_store = invalidate_store(store, &entry.invalidated_items);
    let invalidated = entry.invalidated_items.clone();
    let target = entry.target.clone();
    let persisted_store = record_history_entry(updated_store, entry);
    persist_store(&video.directory, &persisted_store)?;
    let text = format!(
        "Edited shot {} for {}. True descendants marked stale.",
        spec.frame, video.reference
    );
    Ok(build_edit_payload(&text, &target, &invalidated))
}

fn build_edit_payload(text: &str, target: &str, invalidated: &[String]) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "target": target,
            "invalidated": invalidated,
        }),
    }
}

fn spoken_line_invalidated(line: usize) -> Vec<String> {
    vec![format!("audio/{line}"), "assembly".into()]
}

fn full_line_invalidated(line: usize) -> Vec<String> {
    vec![
        format!("audio/{line}"),
        "assembly".into(),
        "translation".into(),
        format!("captions/{line}"),
    ]
}

fn line_history_entry(revision: usize, spec: LineEditSpec<'_>, now: String) -> EditHistoryEntry {
    let target = format!("line-{}", spec.line);
    let invalidated = if spec.spoken_only {
        spoken_line_invalidated(spec.line)
    } else {
        full_line_invalidated(spec.line)
    };
    EditHistoryEntry {
        revision,
        kind: "line".into(),
        target,
        previous_content: serde_json::json!({"line": spec.line}),
        updated_content: serde_json::json!({
            "line": spec.line,
            "lang": spec.lang,
            "text": spec.text,
            "spoken_only": spec.spoken_only,
        }),
        timestamp: now,
        invalidated_items: invalidated,
    }
}

pub(crate) fn edit_line(
    catalog: &Catalog,
    reference: &str,
    spec: LineEditSpec<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let store = load_store(&video.directory);
    let now = jiff::Timestamp::now().to_string();
    let revision = next_revision(&store.history);
    let entry = line_history_entry(revision, spec, now);
    let updated_store = invalidate_store(store, &entry.invalidated_items);
    let invalidated = entry.invalidated_items.clone();
    let target = entry.target.clone();
    let persisted_store = record_history_entry(updated_store, entry);
    persist_store(&video.directory, &persisted_store)?;
    let text = format!("Edited line {} for {}.", spec.line, video.reference);
    Ok(build_edit_payload(&text, &target, &invalidated))
}

fn cast_history_entry(revision: usize, spec: CastEditSpec<'_>, now: String) -> EditHistoryEntry {
    let target = format!("character-{}", spec.character);
    let invalidated = vec![format!("sheet/{}", spec.character), "assembly".into()];
    EditHistoryEntry {
        revision,
        kind: "cast".into(),
        target,
        previous_content: serde_json::json!({"character": spec.character}),
        updated_content: serde_json::json!({"character": spec.character, "look": spec.look}),
        timestamp: now,
        invalidated_items: invalidated,
    }
}

pub(crate) fn edit_cast(
    catalog: &Catalog,
    reference: &str,
    spec: CastEditSpec<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let store = load_store(&video.directory);
    let now = jiff::Timestamp::now().to_string();
    let revision = next_revision(&store.history);
    let entry = cast_history_entry(revision, spec, now);
    let updated_store = invalidate_store(store, &entry.invalidated_items);
    let invalidated = entry.invalidated_items.clone();
    let target = entry.target.clone();
    let persisted_store = record_history_entry(updated_store, entry);
    persist_store(&video.directory, &persisted_store)?;
    let text = format!("Edited cast '{}' for {}.", spec.character, video.reference);
    Ok(build_edit_payload(&text, &target, &invalidated))
}

const fn create_budget_spec<'a>(budget_usd: &'a str, previous: &'a str) -> BudgetHistorySpec<'a> {
    BudgetHistorySpec {
        budget_usd,
        previous,
    }
}

fn budget_history_entry(
    revision: usize,
    spec: BudgetHistorySpec<'_>,
    now: String,
) -> EditHistoryEntry {
    EditHistoryEntry {
        revision,
        kind: "budget".into(),
        target: "budget".into(),
        previous_content: serde_json::json!({"budget": spec.previous}),
        updated_content: serde_json::json!({"budget": spec.budget_usd}),
        timestamp: now,
        invalidated_items: Vec::new(),
    }
}

pub(crate) fn edit_budget(
    catalog: &Catalog,
    reference: &str,
    budget_usd: &str,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let store = load_store(&video.directory);
    let now = jiff::Timestamp::now().to_string();
    let revision = next_revision(&store.history);
    let previous = video.budget.to_string();
    let spec = create_budget_spec(budget_usd, &previous);
    let entry = budget_history_entry(revision, spec, now);
    let updated_store = record_history_entry(store, entry);
    persist_store(&video.directory, &updated_store)?;
    let text = format!("Updated budget to {budget_usd} for {}.", video.reference);
    Ok(build_budget_payload(&text, budget_usd))
}

fn build_budget_payload(text: &str, budget_usd: &str) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "budget": budget_usd,
        }),
    }
}

pub(crate) fn undo(catalog: &Catalog, reference: &str) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let mut store = load_store(&video.directory);
    let Some(last_edit) = store.history.pop() else {
        return Ok(Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "text": "No previous edits to undo.",
                "undone": false,
            }),
        });
    };
    let updated_store = invalidate_store(store, &last_edit.invalidated_items);
    persist_store(&video.directory, &updated_store)?;
    let text = format!("Restored prior content for '{}'.", last_edit.target);
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "undone": true,
            "restored_target": last_edit.target,
            "invalidated": last_edit.invalidated_items,
        }),
    })
}

fn split_item_keys(stage: &str, items: &str) -> Vec<String> {
    let mut list: Vec<String> = items
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| format!("{stage}/{item}"))
        .collect();
    list.push("assembly".into());
    list
}

fn compute_redo_invalidation(stage: &str, items: Option<&str>) -> Vec<String> {
    match items {
        Some(items_str) => split_item_keys(stage, items_str),
        None => vec![stage.to_string(), "assembly".into()],
    }
}

pub(crate) fn redo(
    catalog: &Catalog,
    reference: &str,
    spec: RedoSpec<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let affected = compute_redo_invalidation(spec.stage, spec.items);
    let is_dry_run = spec.mode == RedoMode::DryRun;
    if is_dry_run {
        return Ok(build_redo_dry_run(&affected));
    }
    let store = load_store(&video.directory);
    let updated_store = invalidate_store(store, &affected);
    persist_store(&video.directory, &updated_store)?;
    Ok(build_redo_commit(&affected))
}

fn build_redo_dry_run(affected: &[String]) -> Payload {
    let joined = affected.join(", ");
    let text = format!(
        "Would make new versions of: {joined}. Then rebuild: assembly. Nothing was changed."
    );
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "dry_run": true,
            "affected": affected,
        }),
    }
}

fn build_redo_commit(affected: &[String]) -> Payload {
    let joined = affected.join(", ");
    let text =
        format!("New versions requested for: {joined}. Continuing will rebuild affected work.");
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "affected": affected,
            "dry_run": false,
        }),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEvent {
    pub sequence: usize,
    pub kind: String,
    pub target: String,
    pub timestamp: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

fn collect_edit_history(store: &ReviewStore) -> Vec<HistoryEvent> {
    let mut items = Vec::new();
    for entry in &store.history {
        items.push(HistoryEvent {
            sequence: 0,
            kind: "edit".to_string(),
            target: entry.target.clone(),
            timestamp: entry.timestamp.clone(),
            summary: format!("Edited {} (rev {})", entry.kind, entry.revision),
            details: Some(serde_json::json!({
                "edit_kind": entry.kind,
                "revision": entry.revision,
                "previous": entry.previous_content,
                "updated": entry.updated_content,
                "invalidated": entry.invalidated_items,
            })),
        });
    }
    items
}

fn collect_approval_events(store: &ReviewStore) -> Vec<HistoryEvent> {
    let mut items = Vec::new();
    for (item_key, approval) in &store.approvals {
        let summary = if approval.stale {
            format!("Approval for {item_key} became stale")
        } else {
            format!("Approved {item_key}")
        };
        items.push(HistoryEvent {
            sequence: 0,
            kind: "review".to_string(),
            target: item_key.clone(),
            timestamp: approval.approved_at.clone(),
            summary,
            details: Some(serde_json::json!({
                "action": "approval",
                "content_hash": approval.content_hash,
                "locked": approval.locked,
                "stale": approval.stale,
            })),
        });
    }
    items
}

fn collect_note_events(store: &ReviewStore) -> Vec<HistoryEvent> {
    let mut items = Vec::new();
    for (item_key, notes) in &store.notes {
        for note in notes {
            items.push(HistoryEvent {
                sequence: 0,
                kind: "review".to_string(),
                target: item_key.clone(),
                timestamp: note.created_at.clone(),
                summary: format!("Feedback note on {item_key}: {}", note.text),
                details: Some(serde_json::json!({
                    "action": "note",
                    "note_id": note.id,
                    "text": note.text,
                })),
            });
        }
    }
    items
}

fn collect_review_history(store: &ReviewStore) -> Vec<HistoryEvent> {
    let mut items = collect_approval_events(store);
    items.extend(collect_note_events(store));
    items
}

fn collect_plan_history(snapshot: &crate::video::VideoSnapshot<'_>) -> Vec<HistoryEvent> {
    let mut items = Vec::new();
    for step in &snapshot.recorded.steps {
        items.push(HistoryEvent {
            sequence: 0,
            kind: "plan".to_string(),
            target: step.step.clone(),
            timestamp: snapshot.video.created.clone(),
            summary: format!("Pipeline stage {} recorded", step.step),
            details: Some(serde_json::json!({
                "step": step.step,
                "integrity": format!("{:?}", step.integrity),
            })),
        });
    }
    items
}

fn delivery_to_history_event(
    delivery: &retold_engine::session::PreparedDelivery,
    fallback_time: &str,
) -> HistoryEvent {
    let target = format!("{}/{}", delivery.edition, delivery.format);
    let summary = format!(
        "Delivery prepared: {} ({})",
        delivery.edition, delivery.format
    );
    let details = serde_json::json!({
        "edition": delivery.edition,
        "format": delivery.format,
    });
    HistoryEvent {
        sequence: 0,
        kind: "export".to_string(),
        target,
        timestamp: fallback_time.to_string(),
        summary,
        details: Some(details),
    }
}

fn collect_export_history(
    layout: &retold_engine::RunLayout,
    fallback_time: &str,
) -> Vec<HistoryEvent> {
    let Ok(deliveries) = retold_engine::session::prepared_deliveries(layout) else {
        return Vec::new();
    };
    deliveries
        .iter()
        .map(|item| delivery_to_history_event(item, fallback_time))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFilter {
    Plan,
    Edit,
    Review,
    Export,
    All,
}

const FILTER_TABLE: &[(HistoryFilter, &str)] = &[
    (HistoryFilter::Plan, "plan"),
    (HistoryFilter::Edit, "edit"),
    (HistoryFilter::Review, "review"),
    (HistoryFilter::Export, "export"),
    (HistoryFilter::All, "all"),
];

impl HistoryFilter {
    fn parse(kind: Option<&str>) -> Result<Self, Diagnostic> {
        let name = kind.unwrap_or("all");
        for &(filter, label) in FILTER_TABLE {
            if label == name {
                return Ok(filter);
            }
        }
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Invalid history kind. Supported: plan, edit, review, export, all.",
        ))
    }

    fn as_str(self) -> &'static str {
        for &(filter, label) in FILTER_TABLE {
            if filter == self {
                return label;
            }
        }
        "all"
    }
}

fn gather_all_history(
    snapshot: &crate::video::VideoSnapshot<'_>,
    store: &ReviewStore,
) -> Vec<HistoryEvent> {
    let mut events = collect_edit_history(store);
    events.extend(collect_review_history(store));
    events.extend(collect_plan_history(snapshot));
    events.extend(collect_export_history(
        &snapshot.video.layout,
        &snapshot.video.created,
    ));
    events
}

fn collect_single_filter(
    filter: HistoryFilter,
    snapshot: &crate::video::VideoSnapshot<'_>,
    store: &ReviewStore,
) -> Vec<HistoryEvent> {
    match filter {
        HistoryFilter::Plan => collect_plan_history(snapshot),
        HistoryFilter::Edit => collect_edit_history(store),
        HistoryFilter::Review => collect_review_history(store),
        _ => collect_export_history(&snapshot.video.layout, &snapshot.video.created),
    }
}

fn select_history_events(
    filter: HistoryFilter,
    snapshot: &crate::video::VideoSnapshot<'_>,
    store: &ReviewStore,
) -> Vec<HistoryEvent> {
    if filter == HistoryFilter::All {
        gather_all_history(snapshot, store)
    } else {
        collect_single_filter(filter, snapshot, store)
    }
}

fn build_history_payload(video_id: &str, filter: &str, mut events: Vec<HistoryEvent>) -> Payload {
    events.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.target.cmp(&right.target))
    });
    for (index, item) in events.iter_mut().enumerate() {
        item.sequence = index + 1;
    }
    let total = events.len();
    let text = format!("Found {total} history event(s) for video '{video_id}'.");
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "video_id": video_id,
            "kind": filter,
            "total": total,
            "items": events,
        }),
    }
}

pub(crate) fn video_history(
    catalog: &Catalog,
    reference: &str,
    kind: Option<&str>,
) -> Result<Payload, Diagnostic> {
    let filter = HistoryFilter::parse(kind)?;
    let snapshot = crate::video::snapshot(catalog, reference)?;
    let store = crate::notes::merge_saved(snapshot.video, load_store(&snapshot.video.directory))?;
    let events = select_history_events(filter, &snapshot, &store);
    Ok(build_history_payload(
        &snapshot.video.id,
        filter.as_str(),
        events,
    ))
}
