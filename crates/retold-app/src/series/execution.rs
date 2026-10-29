//! A series run submits real, individually pinned jobs under a divided total allowance.
use super::{SeriesManifest, SeriesPart, filter_parts, load_series_manifest, series_folder_dir};
use crate::plan::{Blocker, ExecutablePlan, PlanOptions, Review};
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Options {
    pub spend: Option<String>,
    pub max_requests: Option<u64>,
    pub plan_hash: Option<String>,
    pub yes: bool,
}

/// The series and which of its parts a plan or run covers.
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    pub root: &'a Path,
    pub id: &'a str,
    pub parts: Option<&'a str>,
}

struct Prepared {
    hash: String,
    manifest: SeriesManifest,
    selected: Vec<SeriesPart>,
    parts: Vec<(String, ExecutablePlan)>,
    blockers: Vec<Blocker>,
    allowance: PlanOptions,
}

pub fn plan(target: Target<'_>, options: &Options) -> Result<Payload, Diagnostic> {
    let prepared = prepare(target, options)?;
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: describe(&prepared),
    })
}

pub fn run(target: Target<'_>, options: &Options) -> Result<Payload, Diagnostic> {
    let prepared = prepare(target, options)?;
    Review {
        plan_hash: &prepared.hash,
        blockers: &prepared.blockers,
        reviewed: options.plan_hash.as_deref(),
        yes: options.yes,
    }
    .confirm("Review series plan, then supply --plan-hash HASH or --yes.")?;
    submit(target.root, &prepared)
}

fn select(
    manifest: &SeriesManifest,
    selector: Option<&str>,
) -> Result<Vec<SeriesPart>, Diagnostic> {
    let selected = filter_parts(&manifest.parts, selector);
    if selected.is_empty() {
        return Err(input("Select at least one existing series part."));
    }
    let exact = selector.is_none_or(|selector| names_each_once(selector, &selected));
    if !exact {
        return Err(input("Part selectors must be unique existing part IDs."));
    }
    Ok(selected.into_iter().cloned().collect())
}

/// Whether `selector` names each selected part once and nothing else.
fn names_each_once(selector: &str, selected: &[&SeriesPart]) -> bool {
    let known: BTreeSet<_> = selected.iter().map(|part| part.id.as_str()).collect();
    let requested: Vec<_> = selector.split(',').map(str::trim).collect();
    let unique: BTreeSet<_> = requested.iter().copied().collect();
    distinct(&requested, &unique) && unique.is_subset(&known)
}

fn distinct(requested: &[&str], unique: &BTreeSet<&str>) -> bool {
    requested.len() == unique.len()
}

fn allowance(options: &Options, count: usize) -> Result<PlanOptions, Diagnostic> {
    let spend: retold_engine::gate::Usd = options
        .spend
        .as_deref()
        .unwrap_or("0")
        .parse()
        .map_err(|_| input("Spend must be a nonnegative USD amount."))?;
    let count = i64::try_from(count).map_err(|_| input("Too many series parts."))?;
    let total_requests = options.max_requests.unwrap_or_else(|| {
        if spend.micros() > 0 {
            crate::plan::DEFAULT_PAID_REQUESTS
        } else {
            0
        }
    });
    let requests = total_requests / count.cast_unsigned();
    Ok(PlanOptions {
        until: Some("ready".into()),
        spend: Some(retold_engine::gate::Usd::from_micros(spend.micros() / count).to_string()),
        max_requests: Some(requests),
        export: None,
    })
}

fn prepare(target: Target<'_>, options: &Options) -> Result<Prepared, Diagnostic> {
    let manifest = load_series_manifest(&series_folder_dir(target.root, target.id))?;
    let selected = select(&manifest, target.parts)?;
    let allowance = allowance(options, selected.len())?;
    let (parts, blockers) = compile_parts(target.root, &selected, &allowance);
    let mut prepared = Prepared {
        hash: String::new(),
        manifest,
        selected,
        parts,
        blockers,
        allowance,
    };
    let bytes = serde_json::to_vec(&(
        &prepared.manifest,
        &prepared.selected,
        &prepared.allowance,
        &prepared.parts,
        &prepared.blockers,
    ))
    .map_err(|error| input(&error.to_string()))?;
    prepared.hash = blake3::hash(&bytes).to_hex().to_string();
    Ok(prepared)
}

type PartPlans = (Vec<(String, ExecutablePlan)>, Vec<Blocker>);
type Accepted = (
    Vec<(String, ExecutablePlan)>,
    Vec<Blocker>,
    BTreeSet<String>,
);

/// Each part's own plan; a part without a usable plan becomes a blocker instead.
fn compile_parts(root: &Path, selected: &[SeriesPart], allowance: &PlanOptions) -> PartPlans {
    let compiled = selected
        .iter()
        .map(|part| (part.id.clone(), compile_part(root, part, allowance)));
    let start: Accepted = (Vec::new(), Vec::new(), BTreeSet::new());
    let (parts, blockers, _) = compiled.fold(start, accept);
    (parts, blockers)
}

/// Keeps a compiled part, unless an earlier part already uses its video.
fn accept(accepted: Accepted, compiled: (String, Result<ExecutablePlan, Diagnostic>)) -> Accepted {
    let (id, result) = compiled;
    match result.and_then(|plan| unused_video(&accepted.2, plan)) {
        Ok(plan) => keep(accepted, id, plan),
        Err(error) => refuse(accepted, &id, &error),
    }
}

fn unused_video(
    videos: &BTreeSet<String>,
    plan: ExecutablePlan,
) -> Result<ExecutablePlan, Diagnostic> {
    let shared = videos.contains(&plan.authority.video_id);
    if shared {
        return Err(input(SHARED_VIDEO));
    }
    Ok(plan)
}

fn keep(mut accepted: Accepted, id: String, plan: ExecutablePlan) -> Accepted {
    accepted.2.insert(plan.authority.video_id.clone());
    accepted.1.extend(plan.blockers.clone());
    accepted.0.push((id, plan));
    accepted
}

fn refuse(mut accepted: Accepted, id: &str, error: &Diagnostic) -> Accepted {
    accepted.1.push(blocker(id, error));
    accepted
}

const SHARED_VIDEO: &str = "Two selected parts point to the same video.";

fn compile_part(
    root: &Path,
    part: &SeriesPart,
    options: &PlanOptions,
) -> Result<ExecutablePlan, Diagnostic> {
    let reference = part.video_ref.as_deref().ok_or_else(|| {
        input(
            "Create a draft and link it using series update --part ID --video REF before running.",
        )
    })?;
    let catalog = crate::discovery::scan(root)?;
    crate::plan::compile(&catalog, reference, options).map(|(plan, _)| plan)
}

fn blocker(part: &str, error: &Diagnostic) -> Blocker {
    Blocker {
        code: error.code,
        message: format!("{part}: {}", error.message),
        context: serde_json::json!({"part":part,"cause":error.context}),
    }
}

fn describe(prepared: &Prepared) -> serde_json::Value {
    let count = u64::try_from(prepared.selected.len()).unwrap_or(u64::MAX);
    let usd = prepared
        .parts
        .iter()
        .map(|(_, plan)| plan.authority.usd_micros)
        .sum::<u64>();
    let requests = prepared
        .allowance
        .max_requests
        .unwrap_or_default()
        .saturating_mul(count);
    serde_json::json!({
        "text":format!("Plan for {}: {} part(s). Total allowance is divided equally; unused remainder is not authorized.", prepared.manifest.work, count),
        "series_id":prepared.manifest.id, "work":prepared.manifest.work,
        "selected_parts":prepared.selected, "part_plans":prepared.parts,
        "plan_hash":prepared.hash, "blockers":prepared.blockers,
        "can_execute":prepared.blockers.is_empty(), "dispatches":0,
        "export_authorized":false,
        "authority":{"usd_micros":usd,"requests":requests,"stages":["queue selected parts"]}
    })
}

fn submit(root: &Path, prepared: &Prepared) -> Result<Payload, Diagnostic> {
    let mut submitted = Vec::new();
    for (part, plan) in &prepared.parts {
        let receipt = submit_part(root, prepared, plan).map_err(|mut diagnostic| {
            diagnostic.context = serde_json::json!({"failed_part":part,"submitted":submitted,"cause":diagnostic.context});
            diagnostic
        })?;
        submitted.push(serde_json::json!({"part":part,"receipt":receipt}));
    }
    Ok(Payload::Result {
        outcome: Outcome::Queued,
        contents: serde_json::json!({
            "text":format!("Submitted {} real studio job(s).", submitted.len()),
            "series_id":prepared.manifest.id,"parts_scheduled":submitted.len(),
            "status":"queued","submitted":submitted,"plan_hash":prepared.hash
        }),
    })
}

/// Queues one part pinned to its own reviewed plan hash; resubmitting is idempotent.
fn submit_part(
    root: &Path,
    prepared: &Prepared,
    plan: &ExecutablePlan,
) -> Result<serde_json::Value, Diagnostic> {
    let options = crate::studio::SubmitOptions {
        execution: crate::continuation::ContinueOptions {
            plan: prepared.allowance.clone(),
            plan_hash: Some(plan.plan_hash.clone()),
            yes: false,
        },
        idempotency_key: Some(format!("series:{}:{}", prepared.hash, plan.plan_hash)),
    };
    match crate::studio::submit(root, &plan.authority.video_id, &options) {
        Payload::Result { contents, .. } => Ok(contents),
        Payload::Error { diagnostic, .. } => Err(diagnostic),
        Payload::Event(_) => Err(input("Unexpected submission event.")),
    }
}

fn input(message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCode::InputRequired, message)
}
