//! Read-only projections of recorded evidence; generation readiness is never guessed.

use crate::library::{Catalog, Video, response};
use crate::protocol::{Diagnostic, ErrorCode, Payload};
pub use retold_engine::gate::Usd;
use retold_engine::store::inspection::{self, Inspection, Integrity};
use serde::Serialize;
use std::sync::Arc;

const MANIFEST_LIMIT: u64 = 2 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct VideoSnapshot<'a> {
    pub video: &'a Video,
    pub recorded: Inspection,
    pub snapshot_id: String,
    readiness: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct StagePlan {
    stage: String,
    dependencies: Vec<String>,
    recorded_steps: usize,
    envelope_verified: usize,
    unresolved_records: usize,
    decision: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct CostsOptions<'a> {
    pub request: Option<&'a str>,
    pub cost: Option<Usd>,
}

#[derive(Clone, Copy)]
struct ReconcileRequest<'a> {
    video: &'a Video,
    request: &'a str,
    cost: Usd,
}

pub(crate) fn show(catalog: &Catalog, reference: &str) -> Result<Payload, Diagnostic> {
    let snapshot = snapshot(catalog, reference)?;
    let contents = serde_json::to_value(snapshot).map_err(|_| invalid_state())?;
    Ok(response(contents))
}

/// An executable plan: evidence per stage plus the hashed authority `continue` would admit.
pub(crate) fn plan(
    catalog: &Catalog,
    reference: &str,
    options: &crate::plan::PlanOptions,
) -> Result<Payload, Diagnostic> {
    let (plan, snapshot) = crate::plan::compile(catalog, reference, options)?;
    let stages = stages(&snapshot.recorded);
    let qualification = qualify_plan(&plan.authority.stages, &stages);
    let next = plan.next_command(&snapshot.video.reference);
    Ok(response(serde_json::json!({
        "snapshot": snapshot,
        "stages": stages,
        "plan_kind": "executable",
        "can_execute": plan.is_unblocked(),
        "all_cached": qualification.all_cached,
        "plan_hash": plan.plan_hash,
        "authority": plan.authority,
        "blockers": plan.blockers,
        "estimates": qualification.estimates,
        "dispatches": 0,
        "export_authorized": false,
        "reuse": qualification.reuse,
        "next": next,
    })))
}

struct PlanQualification {
    estimates: serde_json::Value,
    reuse: &'static str,
    all_cached: bool,
}

fn qualify_plan(planned: &[String], stages: &[StagePlan]) -> PlanQualification {
    let all_cached = are_stages_cached(planned, stages);
    if all_cached {
        PlanQualification {
            estimates: serde_json::json!({
                "usd_micros": 0,
                "requests": 0,
                "duration_ms": 0,
                "source": "qualified_cache"
            }),
            reuse: "All planned stages are qualified and reused from cache; 0 steps to run · $0.",
            all_cached: true,
        }
    } else {
        PlanQualification {
            estimates: serde_json::json!({
                "usd_micros": null,
                "requests": null,
                "duration_ms": null,
                "source": "unknown"
            }),
            reuse: "Saved results are reused by their cache keys; only work they do not cover may dispatch, within the authority ceilings.",
            all_cached: false,
        }
    }
}

fn are_stages_cached(planned: &[String], stages: &[StagePlan]) -> bool {
    let has_stages = !planned.is_empty();
    has_stages
        && planned.iter().all(|stage_name| {
            stages.iter().any(|stage_plan| {
                &stage_plan.stage == stage_name && stage_plan.decision == "reused_from_cache"
            })
        })
}

fn open_reconciliation_ledger(
    catalog: &Catalog,
    video: &Video,
) -> Result<retold_engine::ledger::Ledger, Diagnostic> {
    let actual = video
        .directory
        .canonicalize()
        .map_err(|_| invalid_state())?;
    if actual != video.directory || !actual.starts_with(&catalog.root) {
        return Err(invalid_state());
    }
    let account = crate::account::canonical_directory()
        .map_err(|error| crate::account::commands::diagnostic(&error))
        .and_then(|dir| {
            crate::account::Account::open(&dir)
                .map_err(|error| crate::account::commands::diagnostic(&error))
        })?;
    let workspace_id = crate::plan::workspace_id(&catalog.root)?;
    let gate = crate::execution::AccountGate::for_reconciliation(account, &workspace_id, &video.id);
    let ledger_path = video.layout.meta().join("ledger.json");
    retold_engine::ledger::Ledger::open(&ledger_path, video.budget)
        .map_err(|error| Diagnostic::new(ErrorCode::DataInvalid, &error.to_string()))
        .map(|ledger| ledger.with_gate(Arc::new(gate)))
}

/// Inspect video spend and reconcile unresolved attempts without dispatch authority.
pub(crate) fn costs(
    catalog: &Catalog,
    reference: &str,
    options: &CostsOptions<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let ledger = open_reconciliation_ledger(catalog, video)?;
    if let (Some(request), Some(cost)) = (options.request, options.cost) {
        let req = ReconcileRequest {
            video,
            request,
            cost,
        };
        return reconcile_video(&ledger, req);
    }
    let has_request = options.request.is_some();
    let has_cost = options.cost.is_some();
    if has_request || has_cost {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Both --request and --cost are required to reconcile a charge.",
        ));
    }
    let meta_dir = video.layout.meta();
    inspect_costs(&ledger, video, &meta_dir)
}

fn reconcile_video(
    ledger: &retold_engine::ledger::Ledger,
    target: ReconcileRequest<'_>,
) -> Result<Payload, Diagnostic> {
    ledger
        .reconcile(target.request, target.cost)
        .map_err(|error| Diagnostic::new(ErrorCode::AccountRequired, &error.to_string()))?;
    Ok(response(serde_json::json!({
        "reconciled": true,
        "video": {
            "id": target.video.id,
            "ref": target.video.reference,
            "title": target.video.title,
        },
        "request": target.request,
        "cost_usd_micros": target.cost.micros(),
        "cost_formatted": target.cost.to_string(),
        "dispatch_authorized": false,
    })))
}

fn stage_costs(
    meta_dir: &std::path::Path,
    video: &Video,
    requests: &std::collections::BTreeMap<String, retold_engine::ledger::Charge>,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let store_receipts = retold_engine::store::read_state(&video.layout)
        .map(|state| state.receipts)
        .unwrap_or_default();
    let receipts = retold_engine::estimate::merged_receipts(meta_dir, &store_receipts, requests);
    retold_engine::estimate::by_stage(&receipts)
        .into_iter()
        .map(|(stage, cost)| {
            (
                stage.to_string(),
                serde_json::json!({
                    "cost_usd_micros": cost.cost.micros(),
                    "cost_formatted": cost.cost.to_string(),
                    "calls": cost.calls,
                }),
            )
        })
        .collect()
}

fn unresolved_charges(
    requests: &std::collections::BTreeMap<String, retold_engine::ledger::Charge>,
) -> Vec<serde_json::Value> {
    requests
        .iter()
        .filter(|(_, charge)| {
            matches!(
                charge,
                retold_engine::ledger::Charge::Reserved { .. }
                    | retold_engine::ledger::Charge::Conservative { .. }
            )
        })
        .map(|(id, charge)| {
            serde_json::json!({
                "request": id,
                "held_usd_micros": charge.amount().micros(),
                "held_formatted": charge.amount().to_string(),
            })
        })
        .collect()
}

fn inspect_costs(
    ledger: &retold_engine::ledger::Ledger,
    video: &Video,
    meta_dir: &std::path::Path,
) -> Result<Payload, Diagnostic> {
    let total = ledger
        .total()
        .map_err(|error| Diagnostic::new(ErrorCode::DataInvalid, &error.to_string()))?;
    let requests = ledger
        .entries()
        .map_err(|error| Diagnostic::new(ErrorCode::DataInvalid, &error.to_string()))?;
    let stages = stage_costs(meta_dir, video, &requests);
    let unresolved = unresolved_charges(&requests);
    let all_settled = unresolved.is_empty();
    Ok(response(serde_json::json!({
        "video": {
            "id": video.id,
            "ref": video.reference,
            "title": video.title,
        },
        "budget_usd_micros": video.budget.micros(),
        "budget_formatted": video.budget.to_string(),
        "total_spent_usd_micros": total.micros(),
        "total_spent_formatted": total.to_string(),
        "stages": stages,
        "unresolved": unresolved,
        "all_settled": all_settled,
        "dispatch_authorized": false,
    })))
}

pub(crate) fn snapshot<'a>(
    catalog: &'a Catalog,
    reference: &str,
) -> Result<VideoSnapshot<'a>, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let actual = video
        .directory
        .canonicalize()
        .map_err(|_| invalid_state())?;
    if actual != video.directory || !actual.starts_with(&catalog.root) {
        return Err(invalid_state());
    }
    let recorded = inspection::inspect(&video.layout).map_err(|_| invalid_state())?;
    let manifest = retold_engine::fsx::read_bounded(&video.layout.run_file(), MANIFEST_LIMIT)
        .map_err(|_| invalid_state())?
        .ok_or_else(invalid_state)?;
    let digest = retold_engine::store::hash(&manifest).map_err(|_| invalid_state())?;
    if digest != video.metadata_hash {
        return Err(invalid_state());
    }
    let snapshot_id =
        retold_engine::store::hash(&(video, &recorded)).map_err(|_| invalid_state())?;
    Ok(VideoSnapshot {
        video,
        recorded,
        snapshot_id,
        readiness: "requires_execution_preflight",
    })
}

pub(crate) fn stages(inspection: &Inspection) -> Vec<StagePlan> {
    retold_engine::graph::Graph::stages()
        .dependencies
        .into_iter()
        .map(|(stage, dependencies)| build_stage_plan(inspection, stage, dependencies))
        .collect()
}

fn build_stage_plan(
    inspection: &Inspection,
    stage: String,
    dependencies: std::collections::BTreeSet<String>,
) -> StagePlan {
    let recorded: Vec<_> = inspection
        .steps
        .iter()
        .filter(|step| retold_engine::store::stage_of(&step.step) == stage)
        .collect();
    let verified = recorded
        .iter()
        .filter(|step| step.integrity == Integrity::EnvelopeVerified)
        .count();
    let unresolved = recorded
        .iter()
        .filter(|step| step.integrity != Integrity::EnvelopeVerified)
        .count();
    let decision = stage_decision(recorded.len(), unresolved);
    StagePlan {
        stage,
        dependencies: dependencies.into_iter().collect(),
        recorded_steps: recorded.len(),
        envelope_verified: verified,
        unresolved_records: unresolved,
        decision,
    }
}

const fn stage_decision(recorded_len: usize, unresolved: usize) -> &'static str {
    if recorded_len > 0 && unresolved == 0 {
        "reused_from_cache"
    } else if unresolved > 0 {
        "requires_repair"
    } else {
        "requires_execution_preflight"
    }
}

fn invalid_state() -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        ErrorCode::DataInvalid,
        "Run evidence is invalid, exceeds inspection limits, or changed during inspection. Retry the query; inspect the saved files if it persists.",
    );
    diagnostic.redacted = true;
    diagnostic
}

#[derive(Debug, Clone, Default)]
pub struct CopySpec<'a> {
    pub title: Option<&'a str>,
    pub lang: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteMode {
    Trash,
    Permanent,
}

pub(crate) fn archive(catalog: &Catalog, reference: &str) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let marker = video.directory.join(".archived");
    let write_res = std::fs::write(&marker, b"");
    if let Err(err) = write_res {
        return Err(Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &err.to_string(),
        ));
    }
    Ok(response(serde_json::json!({
        "status": "archived",
        "reference": video.reference,
        "archived": true,
    })))
}

pub(crate) fn unarchive(catalog: &Catalog, reference: &str) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let marker = video.directory.join(".archived");
    if marker.exists() {
        let remove_res = std::fs::remove_file(&marker);
        if let Err(err) = remove_res {
            return Err(Diagnostic::new(
                ErrorCode::ExecutionFailed,
                &err.to_string(),
            ));
        }
    }
    Ok(response(serde_json::json!({
        "status": "unarchived",
        "reference": video.reference,
        "archived": false,
    })))
}

pub(crate) fn delete(
    catalog: &Catalog,
    reference: &str,
    mode: DeleteMode,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    if mode == DeleteMode::Permanent {
        std::fs::remove_dir_all(&video.directory)
            .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
        return Ok(response(serde_json::json!({
            "status": "deleted",
            "reference": video.reference,
            "mode": "permanent",
        })));
    }
    let trash_dir = catalog.root.join(".trash");
    let _ = std::fs::create_dir_all(&trash_dir);
    let item_name = format!("{}-{}", jiff::Timestamp::now().as_millisecond(), video.id);
    let target = trash_dir.join(item_name);
    std::fs::rename(&video.directory, &target)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    Ok(response(serde_json::json!({
        "status": "trashed",
        "reference": video.reference,
        "trash_path": target.display().to_string(),
        "mode": "trash",
    })))
}

fn copy_directory_entries(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    let entries = std::fs::read_dir(src)?;
    for entry in entries {
        let entry = entry?;
        let entry_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry_type.is_dir() {
            copy_directory_entries(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

struct ManifestUpdateSpec {
    new_id: String,
    folder_name: String,
    title: Option<String>,
}

fn build_update_spec(new_id: &str, folder_name: &str, title: Option<&str>) -> ManifestUpdateSpec {
    ManifestUpdateSpec {
        new_id: new_id.to_string(),
        folder_name: folder_name.to_string(),
        title: title.map(ToString::to_string),
    }
}

fn update_copied_meta(
    mut meta: retold_engine::RunMeta,
    spec: ManifestUpdateSpec,
) -> retold_engine::RunMeta {
    meta.id = spec.new_id;
    if let Some(new_title) = spec.title {
        meta.title = new_title;
    }
    let last_level = meta.folder.last_mut();
    if let Some(last) = last_level {
        *last = spec.folder_name;
    }
    meta
}

fn manifest_run_path(dest_dir: &std::path::Path) -> std::path::PathBuf {
    dest_dir.join(".retold").join("run.json")
}

fn resolve_destination_folder_name(dest_dir: &std::path::Path, default_name: &str) -> String {
    dest_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(default_name)
        .to_string()
}

fn update_copied_manifest(
    dest_dir: &std::path::Path,
    new_id: &str,
    spec: &CopySpec<'_>,
) -> Result<(), Diagnostic> {
    let manifest_path = manifest_run_path(dest_dir);
    let bytes = std::fs::read(&manifest_path)
        .map_err(|err| Diagnostic::new(ErrorCode::DataInvalid, &err.to_string()))?;
    let meta: retold_engine::RunMeta = serde_json::from_slice(&bytes)
        .map_err(|err| Diagnostic::new(ErrorCode::DataInvalid, &err.to_string()))?;
    let folder_name = resolve_destination_folder_name(dest_dir, new_id);
    let update_spec = build_update_spec(new_id, &folder_name, spec.title);
    let updated = update_copied_meta(meta, update_spec);
    let serialized = serde_json::to_string_pretty(&updated)
        .map_err(|err| Diagnostic::new(ErrorCode::DataInvalid, &err.to_string()))?;
    std::fs::write(&manifest_path, serialized)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))
}

fn record_provenance(dest_dir: &std::path::Path, video: &Video) -> Result<(), Diagnostic> {
    let provenance_path = dest_dir.join(".retold").join("copied_from.json");
    let content = serde_json::json!({
        "origin_reference": video.reference,
        "origin_id": video.id,
        "copied_at": jiff::Timestamp::now().to_string(),
    });
    let serialized = serde_json::to_string_pretty(&content)
        .map_err(|err| Diagnostic::new(ErrorCode::DataInvalid, &err.to_string()))?;
    std::fs::write(provenance_path, serialized)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))
}

fn resolve_target_copy_dir(video: &Video) -> Result<std::path::PathBuf, Diagnostic> {
    let parent = video
        .directory
        .parent()
        .ok_or_else(|| Diagnostic::new(ErrorCode::DataInvalid, "Video directory has no parent."))?;
    let base_name = video
        .directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("video");
    let dest_dir = parent.join(format!("{base_name}-copy"));
    if dest_dir.exists() {
        return Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "Target copied video directory already exists.",
        ));
    }
    Ok(dest_dir)
}

pub(crate) fn copy(
    catalog: &Catalog,
    reference: &str,
    spec: &CopySpec<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let dest_dir = resolve_target_copy_dir(video)?;
    copy_directory_entries(&video.directory, &dest_dir)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    let new_id = format!("{}-copy", video.id);
    update_copied_manifest(&dest_dir, &new_id, spec)?;
    record_provenance(&dest_dir, video)?;

    Ok(response(serde_json::json!({
        "status": "copied",
        "source": video.reference,
        "new_id": new_id,
        "destination": dest_dir.display().to_string(),
    })))
}

fn validate_tag_name(tag: &str) -> Result<(), Diagnostic> {
    let valid = !tag.is_empty()
        && tag.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character == '-'
                || character == '_'
                || character == ':'
                || character == '/'
        });
    if valid {
        Ok(())
    } else {
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Tag name must contain only alphanumeric characters, dashes, underscores, colons, or slashes.",
        ))
    }
}

fn load_existing_tags(tags_path: &std::path::Path) -> Vec<String> {
    if !tags_path.exists() {
        return Vec::new();
    }
    let Ok(content) = std::fs::read(tags_path) else {
        return Vec::new();
    };
    serde_json::from_slice(&content).unwrap_or_default()
}

fn add_tag_to_list(mut tags: Vec<String>, tag: &str) -> Vec<String> {
    let already_present = tags.iter().any(|existing| existing == tag);
    if !already_present {
        tags.push(tag.to_string());
        tags.sort();
    }
    tags
}

fn remove_tag_from_list(mut tags: Vec<String>, tag: &str) -> Vec<String> {
    tags.retain(|item| item != tag);
    tags
}

fn apply_tag_mutation(
    tags: Vec<String>,
    operation: &crate::library::TagOperation<'_>,
) -> Result<(Vec<String>, &'static str, String), Diagnostic> {
    match operation {
        crate::library::TagOperation::Add(tag) => {
            validate_tag_name(tag)?;
            let updated = add_tag_to_list(tags, tag);
            Ok((updated, "add", (*tag).to_string()))
        }
        crate::library::TagOperation::Remove(tag) => {
            validate_tag_name(tag)?;
            let updated = remove_tag_from_list(tags, tag);
            Ok((updated, "remove", (*tag).to_string()))
        }
    }
}

pub(crate) fn tag_video(
    catalog: &Catalog,
    reference: &str,
    operation: crate::library::TagOperation<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let tags_path = video.directory.join(".retold").join("tags.json");
    let initial_tags = load_existing_tags(&tags_path);
    let (tags, action, tag_name) = apply_tag_mutation(initial_tags, &operation)?;

    let serialized = serde_json::to_string_pretty(&tags)
        .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, "Failed to serialize tags."))?;
    if let Some(parent) = tags_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&tags_path, serialized).map_err(|error| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to write tags file: {error}"),
        )
    })?;

    let text = format!(
        "Tag '{tag_name}' successfully applied ({action}) on video '{}'.",
        video.reference
    );
    Ok(response(serde_json::json!({
        "text": text,
        "reference": video.reference,
        "operation": action,
        "tag": tag_name,
        "tags": tags,
    })))
}
