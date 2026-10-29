//! Executable plans: a hashed, finite authority envelope for continuing or exporting one saved
//! video.
//! Planning reads the library, provider settings and account; it never writes or dispatches.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use retold_engine::gate::{PaidCapability, Usd};
use retold_engine::pipeline::Until;
use retold_engine::session::{self, Expected, PreparedDelivery, Providers, Selection};
use retold_engine::{Paths, RunLayout};
use serde::{Deserialize, Serialize};

use crate::account::types::{
    Action, Capability, Job, Limit, Resources, Scope, Unit, Window, Workspace,
};
use crate::account::{Account, canonical_directory};
use crate::execution::{capability, operation_node};
use crate::library::Catalog;
use crate::protocol::{Diagnostic, ErrorCode};
use crate::video::VideoSnapshot;

/// Paid-call ceiling when `--spend` is positive and `--max-requests` is absent.
pub const DEFAULT_PAID_REQUESTS: u64 = 1_000;
const WORKSPACE_ID_HEX: usize = 16;
const PAID_CAPABILITIES: [PaidCapability; 4] = [
    PaidCapability::Text,
    PaidCapability::Image,
    PaidCapability::Speech,
    PaidCapability::Transcription,
];

/// Stages whose work an export may run: the editor project, rendering and its packaging.
const EXPORT_STAGES: &[&str] = &["deliver", "edit", "metrics"];
/// `--format both` names the two common delivery shapes.
const BOTH_FORMATS: &[&str] = &["16x9", "9x16"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanOptions {
    pub until: Option<String>,
    pub spend: Option<String>,
    pub max_requests: Option<u64>,
    /// Present for `video export`; absent for `video continue`.
    pub export: Option<ExportOptions>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportOptions {
    pub editions: Vec<String>,
    pub formats: Vec<String>,
    pub project_only: bool,
    pub preset: Option<String>,
    pub aspect: Option<String>,
}

fn filter_aspect(
    record: &crate::presets::PresetRecord,
    preset_name: &str,
    aspect: &str,
) -> Result<Vec<String>, Diagnostic> {
    let supported = record.aspects.iter().any(|item| item == aspect);
    if !supported {
        let options = record.aspects.join(", ");
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            &format!(
                "Aspect '{aspect}' is not supported by preset '{preset_name}' (supports: {options})."
            ),
        ));
    }
    Ok(vec![aspect.replace(':', "x")])
}

fn all_preset_formats(record: &crate::presets::PresetRecord) -> Vec<String> {
    record
        .aspects
        .iter()
        .map(|asp| asp.replace(':', "x"))
        .collect()
}

fn resolve_preset_formats(
    root: &Path,
    preset_name: &str,
    aspect: Option<&str>,
) -> Result<Vec<String>, Diagnostic> {
    let record = crate::presets::find_preset_record(root, preset_name)?;
    aspect.map_or_else(
        || Ok(all_preset_formats(&record)),
        |asp| filter_aspect(&record, preset_name, asp),
    )
}

impl ExportOptions {
    fn to_scope(&self, root: &Path) -> Result<ExportScope, Diagnostic> {
        let mut resolved_formats: BTreeSet<String> = self
            .formats
            .iter()
            .flat_map(|format| expand_format(format))
            .collect();
        if let Some(preset_name) = &self.preset {
            let formats = resolve_preset_formats(root, preset_name, self.aspect.as_deref())?;
            resolved_formats.extend(formats);
        }
        let editions: BTreeSet<String> = self.editions.iter().cloned().collect();
        let incomplete = editions.is_empty() || resolved_formats.is_empty();
        if incomplete {
            return Err(Diagnostic::new(
                ErrorCode::InputRequired,
                "Export needs at least one --edition and one --format (16x9, 9x16, 1x1 or both) or --preset.",
            ));
        }
        Ok(ExportScope {
            editions,
            formats: resolved_formats,
            project_only: self.project_only,
            preset: self.preset.clone(),
        })
    }
}

/// The exact deliveries an export job may open, render or save.
#[derive(Debug, Clone, Serialize)]
pub struct ExportScope {
    pub editions: BTreeSet<String>,
    pub formats: BTreeSet<String>,
    pub project_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
}

impl ExportScope {
    fn from_options(options: &ExportOptions, root: &Path) -> Result<Self, Diagnostic> {
        options.to_scope(root)
    }

    fn covered_by(&self, prepared: &[PreparedDelivery]) -> bool {
        let selection = self.selection();
        let available: BTreeSet<_> = prepared
            .iter()
            .map(|item| (item.edition.clone(), item.format.clone()))
            .collect();
        let editions = selection.editions.iter();
        let mut pairs = editions.flat_map(|edition| {
            let formats = selection.formats.iter();
            formats.map(move |format| (edition.clone(), format.clone()))
        });
        pairs.all(|pair| available.contains(&pair))
    }

    pub(crate) fn selection(&self) -> Selection {
        Selection {
            editions: self.editions.clone(),
            formats: self.formats.clone(),
            project_only: self.project_only,
        }
    }
}

fn expand_format(format: &str) -> Vec<String> {
    if format == "both" {
        BOTH_FORMATS.iter().map(|&name| name.to_owned()).collect()
    } else {
        vec![format.to_owned()]
    }
}

/// Everything the plan hash pins. A continue under a different envelope needs a new plan.
#[derive(Debug, Clone, Serialize)]
pub struct Authority {
    pub video_id: String,
    pub workspace_id: String,
    pub until: String,
    pub stages: Vec<String>,
    pub usd_micros: u64,
    pub requests: u64,
    pub video_cap_usd_micros: i64,
    /// Per-use route → the provider account whose allowance it shares.
    pub routes: BTreeMap<String, String>,
    pub input_versions: BTreeMap<String, String>,
    /// Absent for continue, which can never open the editor or render.
    pub export: Option<ExportScope>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Blocker {
    pub code: ErrorCode,
    pub message: String,
    pub context: serde_json::Value,
}

/// A computed plan meeting the caller's confirmation: a reviewed hash, or `--yes`.
pub(crate) struct Review<'a> {
    pub(crate) plan_hash: &'a str,
    pub(crate) blockers: &'a [Blocker],
    pub(crate) reviewed: Option<&'a str>,
    pub(crate) yes: bool,
}

impl Review<'_> {
    /// Refuses a changed or unconfirmed plan, then its first blocker. `--yes` never raises an
    /// allowance; it only accepts the plan computed now.
    pub(crate) fn confirm(&self, unconfirmed: &str) -> Result<(), Diagnostic> {
        let changed = self.reviewed.is_some_and(|hash| hash != self.plan_hash);
        if changed {
            let mut diagnostic = Diagnostic::new(
                ErrorCode::PlanChanged,
                "The plan changed since it was reviewed. Nothing was admitted or dispatched.",
            );
            diagnostic.context = serde_json::json!({ "plan_hash": self.plan_hash });
            return Err(diagnostic);
        }
        let confirmed = self.reviewed.is_some() || self.yes;
        if !confirmed {
            return Err(Diagnostic::new(ErrorCode::InputRequired, unconfirmed));
        }
        self.unblocked()
    }

    fn unblocked(&self) -> Result<(), Diagnostic> {
        let Some(blocker) = self.blockers.first() else {
            return Ok(());
        };
        let mut diagnostic = Diagnostic::new(blocker.code, &blocker.message);
        diagnostic.context =
            serde_json::json!({ "plan_hash": self.plan_hash, "blockers": self.blockers });
        Err(diagnostic)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutablePlan {
    pub plan_hash: String,
    pub authority: Authority,
    pub blockers: Vec<Blocker>,
    #[serde(skip)]
    pub(crate) runtime: Runtime,
}

/// What execution needs beyond the hashed envelope.
#[derive(Debug, Clone)]
pub(crate) struct Runtime {
    pub(crate) paths: Paths,
    pub(crate) providers: Providers,
    pub(crate) layout: RunLayout,
    pub(crate) root: PathBuf,
    pub(crate) until: Until,
    pub(crate) expected: Expected,
    pub(crate) selection: Option<Selection>,
}

impl ExecutablePlan {
    #[must_use]
    pub const fn is_unblocked(&self) -> bool {
        self.blockers.is_empty()
    }

    /// The exact command that executes this plan.
    #[must_use]
    pub fn next_command(&self, reference: &str) -> Vec<String> {
        let mut words: Vec<String> = ["retold", "video"].map(String::from).to_vec();
        match &self.authority.export {
            None => words.extend(["continue".into(), reference.into()]),
            Some(scope) => words.extend(export_words(reference, scope)),
        }
        words.extend(["--plan-hash".into(), self.plan_hash.clone()]);
        words
    }

    /// Fresh job authority for exactly this plan; per-stage nodes are slices of its total.
    pub(crate) fn job(&self, job_id: &str) -> Result<Job, Diagnostic> {
        let authority = &self.authority;
        let resources = resources(authority.requests, authority.usd_micros);
        Ok(Job {
            id: job_id.into(),
            workspace: Workspace {
                id: authority.workspace_id.clone(),
                root: self.runtime.root.clone(),
            },
            video_id: authority.video_id.clone(),
            series_id: None,
            plan_hash: self.plan_hash.clone(),
            input_versions: authority.input_versions.clone(),
            action: action(authority.export.as_ref()),
            routes: authority.routes.keys().cloned().collect(),
            capabilities: capabilities(authority.export.as_ref()),
            operations: self.operation_tree(&resources)?,
            resources,
            deadline: None,
        })
    }

    fn operation_tree(
        &self,
        ceiling: &Resources,
    ) -> Result<BTreeMap<String, Resources>, Diagnostic> {
        let video_id = &self.authority.video_id;
        let mut nodes = BTreeMap::new();
        for stage in &self.authority.stages {
            for paid in PAID_CAPABILITIES {
                let node = operation_node(video_id, stage, capability(paid))
                    .map_err(|_| invalid("Could not derive the operation tree."))?;
                nodes.insert(node, ceiling.clone());
            }
        }
        Ok(nodes)
    }
}

pub(crate) fn compile<'a>(
    catalog: &'a Catalog,
    reference: &str,
    options: &PlanOptions,
) -> Result<(ExecutablePlan, VideoSnapshot<'a>), Diagnostic> {
    let ceilings = Ceilings::from_options(options, &catalog.root)?;
    let snapshot = crate::video::snapshot(catalog, reference)?;
    let installation = Installation::discover(catalog)?;
    let authority = authority(&snapshot, &ceilings, &installation)?;
    let video = snapshot.video;
    let plan = ExecutablePlan {
        plan_hash: digest(&authority)?,
        blockers: blockers(&authority, &video.layout),
        runtime: Runtime {
            paths: installation.paths,
            providers: installation.providers,
            layout: video.layout.clone(),
            root: installation.root,
            until: ceilings.until,
            expected: reviewed_inputs(&snapshot, &authority)?,
            selection: ceilings.export.as_ref().map(ExportScope::selection),
        },
        authority,
    };
    Ok((plan, snapshot))
}

fn reviewed_inputs(
    snapshot: &VideoSnapshot<'_>,
    authority: &Authority,
) -> Result<Expected, Diagnostic> {
    let version = |key: &str| {
        authority
            .input_versions
            .get(key)
            .cloned()
            .ok_or_else(|| invalid("Plan is missing a reviewed input version."))
    };
    Ok(Expected {
        manifest_hash: snapshot.video.metadata_hash.clone(),
        state_hash: snapshot.recorded.state_hash.clone(),
        source_hash: version("source_file")?,
        editing_hash: version("manual_script")?,
    })
}

/// The stop, finite allowance and export scope a request asks for.
struct Ceilings {
    until: Until,
    usd_micros: u64,
    requests: u64,
    export: Option<ExportScope>,
}

impl Ceilings {
    fn from_options(options: &PlanOptions, root: &Path) -> Result<Self, Diagnostic> {
        let PlanOptions {
            until: stop,
            spend: amount,
            max_requests,
            export,
        } = options;
        let usd_micros = spend(amount.as_deref())?;
        let paid = usd_micros > 0;
        let default_requests = if paid { DEFAULT_PAID_REQUESTS } else { 0 };
        let export = export
            .as_ref()
            .map(|opts| ExportScope::from_options(opts, root))
            .transpose()?;
        let until = match &export {
            Some(scope) => export_stop(stop.as_deref(), scope)?,
            None => until(stop.as_deref())?,
        };
        Ok(Self {
            until,
            usd_micros,
            requests: max_requests.unwrap_or(default_requests),
            export,
        })
    }

    fn stages(&self) -> Vec<String> {
        let exporting = self.export.is_some();
        if exporting {
            return EXPORT_STAGES
                .iter()
                .map(|&stage| stage.to_owned())
                .collect();
        }
        session::stages_through(self.until).into_iter().collect()
    }
}

/// Export never takes a generation stop; its scope decides project or render.
fn export_stop(requested: Option<&str>, scope: &ExportScope) -> Result<Until, Diagnostic> {
    if requested.is_some() {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Export takes no --until. Continue the video to ready first, then export it.",
        ));
    }
    Ok(if scope.project_only {
        Until::Project
    } else {
        Until::Export
    })
}

/// Installation settings the plan pins: provider routes and the canonical library root.
struct Installation {
    paths: Paths,
    providers: Providers,
    root: PathBuf,
}

impl Installation {
    fn discover(catalog: &Catalog) -> Result<Self, Diagnostic> {
        let paths = retold_engine::paths::workspace::for_library(&catalog.root)
            .map_err(|_| invalid("Provider settings could not be located."))?;
        let providers = Providers::load(&paths).map_err(|_| {
            invalid("Provider settings are invalid. Repair providers.toml before planning.")
        })?;
        let context = crate::workspace::context::Context::resolve(Some(&catalog.root))?;
        let root = context
            .root
            .canonicalize()
            .map_err(|_| invalid("The library root is unavailable."))?;
        Ok(Self {
            paths,
            providers,
            root,
        })
    }
}

fn authority(
    snapshot: &VideoSnapshot<'_>,
    ceilings: &Ceilings,
    installation: &Installation,
) -> Result<Authority, Diagnostic> {
    let video = snapshot.video;
    let providers = &installation.providers;
    Ok(Authority {
        video_id: video.id.clone(),
        workspace_id: workspace_id(&installation.root)?,
        until: until_name(ceilings.until),
        stages: ceilings.stages(),
        usd_micros: ceilings.usd_micros,
        requests: ceilings.requests,
        video_cap_usd_micros: video.budget.micros(),
        routes: providers.per_use_routes(),
        input_versions: input_versions(snapshot, providers)?,
        export: ceilings.export.clone(),
    })
}

fn input_versions(
    snapshot: &VideoSnapshot<'_>,
    providers: &Providers,
) -> Result<BTreeMap<String, String>, Diagnostic> {
    let video = snapshot.video;
    Ok(BTreeMap::from([
        ("run_manifest".into(), video.metadata_hash.clone()),
        ("run_state".into(), snapshot.recorded.state_hash.clone()),
        ("run_snapshot".into(), snapshot.snapshot_id.clone()),
        ("providers".into(), providers.digest().into()),
        (
            "manual_script".into(),
            retold_engine::editing::digest(&video.layout)
                .map_err(|error| invalid(&error.to_string()))?,
        ),
        ("deliveries".into(), digest(&prepared(&video.layout)?)?),
        (
            "source_file".into(),
            retold_engine::research::saved_source_hash(&video.layout).map_err(|_| {
                invalid("Saved source could not be inspected as a bounded regular file.")
            })?,
        ),
    ]))
}

fn prepared(layout: &RunLayout) -> Result<Vec<PreparedDelivery>, Diagnostic> {
    session::prepared_deliveries(layout).map_err(|_| {
        invalid("Saved ready deliveries are invalid. Inspect the deliverables folder.")
    })
}

fn action(export: Option<&ExportScope>) -> Action {
    export.map_or(Action::Continue, |scope| Action::Export {
        editions: scope.editions.clone(),
        formats: scope.formats.clone(),
    })
}

/// Only an export job carries the export capability.
fn capabilities(export: Option<&ExportScope>) -> Vec<Capability> {
    let paid = PAID_CAPABILITIES.map(capability);
    let exporting = export.map(|_| Capability::Export);
    paid.into_iter().chain(exporting).collect()
}

fn export_words(reference: &str, scope: &ExportScope) -> Vec<String> {
    let mut words = vec!["export".to_owned(), reference.to_owned()];
    for edition in &scope.editions {
        words.extend(["--edition".into(), edition.clone()]);
    }
    for format in &scope.formats {
        words.extend(["--format".into(), format.clone()]);
    }
    if let Some(preset) = &scope.preset {
        words.extend(["--preset".into(), preset.clone()]);
    }
    if scope.project_only {
        words.push("--project-only".into());
    }
    words
}

fn until(requested: Option<&str>) -> Result<Until, Diagnostic> {
    let until: Until = requested.map_or(Ok(Until::Ready), str::parse).map_err(|_| {
        Diagnostic::new(
            ErrorCode::InputRequired,
            "Choose --until created, research, script, storyboard, characters, images, voice, captions, timeline or ready.",
        )
    })?;
    let beyond_ready = until > Until::Ready;
    if beyond_ready {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Continue stops at ready. Projects and exports need a separate video export request.",
        ));
    }
    Ok(until)
}

fn until_name(until: Until) -> String {
    format!("{until:?}").to_lowercase()
}

/// `--spend` in USD; the default is zero, which considers no per-use route at all.
pub(crate) fn spend(requested: Option<&str>) -> Result<u64, Diagnostic> {
    let amount: Usd = requested
        .map_or(Ok(Usd::ZERO), str::parse)
        .map_err(|_| spend_error())?;
    u64::try_from(amount.micros()).map_err(|_| spend_error())
}

fn spend_error() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::InputRequired,
        "--spend must be a non-negative USD amount, such as 0 or 2.50.",
    )
}

/// One stable workspace per canonical library root.
pub(crate) fn workspace_id(root: &std::path::Path) -> Result<String, Diagnostic> {
    let registered = crate::account::canonical_directory()
        .ok()
        .and_then(|directory| crate::account::Account::open(&directory).ok())
        .and_then(|account| account.workspace_at(root).ok().flatten());
    if let Some(workspace) = registered {
        return Ok(workspace.id);
    }
    let manifest = root.join(retold_engine::paths::workspace::FILENAME);
    if manifest.exists() {
        return retold_engine::paths::workspace::Manifest::load(root)
            .map(|manifest| manifest.id)
            .map_err(|_| invalid("Workspace identity could not be read."));
    }
    let hex = digest(&root)?;
    Ok(format!("library-{}", &hex[..WORKSPACE_ID_HEX]))
}

fn editing_blocker(layout: &RunLayout) -> Option<Blocker> {
    retold_engine::editing::ready(layout).err().map(|error| {
        let diagnostic = crate::editing::diagnostic(&error);
        Blocker {
            code: diagnostic.code,
            message: diagnostic.message,
            context: serde_json::json!({"manual_script": true}),
        }
    })
}

fn blockers(authority: &Authority, layout: &RunLayout) -> Vec<Blocker> {
    let mut found = readiness(authority, layout);
    found.extend(account_blockers(authority));
    found.extend(editing_blocker(layout));
    if let Err(error) = retold_engine::recovery::ensure_active(layout.video_dir()) {
        found.push(Blocker {
            code: ErrorCode::InputRequired,
            message: error.to_string(),
            context: serde_json::json!({"inactive":true}),
        });
    }
    found
}

/// Export needs every requested edition and format already saved at ready.
fn readiness(authority: &Authority, layout: &RunLayout) -> Vec<Blocker> {
    let Some(scope) = &authority.export else {
        return Vec::new();
    };
    let prepared = session::prepared_deliveries(layout).unwrap_or_default();
    if scope.covered_by(&prepared) {
        return Vec::new();
    }
    vec![Blocker {
        code: ErrorCode::NotReady,
        message: "Some editions or formats are not finished yet. Finish the video, or export only what is ready.".into(),
        context: serde_json::json!({ "prepared": prepared }),
    }]
}

fn account_blockers(authority: &Authority) -> Vec<Blocker> {
    let account = canonical_directory().and_then(|directory| Account::open(&directory));
    let account = match account {
        Ok(account) => account,
        Err(error) => {
            return vec![Blocker {
                code: ErrorCode::AccountRequired,
                message: format!("{error} Set an account limit, then plan again."),
                context: serde_json::Value::Null,
            }];
        }
    };
    let paid = authority.usd_micros > 0;
    let limits = account
        .summary()
        .map(|summary| summary.limits)
        .unwrap_or_default();
    let missing = missing_limits(authority, &limits);
    if !paid || missing.is_empty() {
        return Vec::new();
    }
    vec![Blocker {
        code: ErrorCode::AccountRequired,
        message: "Paid work needs positive usd_micros and requests limits for every listed scope. Add them with account set-limits.".into(),
        context: serde_json::json!({ "missing_limits": missing }),
    }]
}

/// Account day/month, workspace and video lifetime ceilings that would refuse every paid call.
fn missing_limits(authority: &Authority, limits: &[Limit]) -> Vec<serde_json::Value> {
    let required = [
        (Scope::Account, Window::Day),
        (Scope::Account, Window::Month),
        (
            Scope::Workspace(authority.workspace_id.clone()),
            Window::Lifetime,
        ),
        (Scope::Video(authority.video_id.clone()), Window::Lifetime),
    ];
    required
        .into_iter()
        .filter(|(scope, window)| !positive_limit(limits, scope, *window))
        .map(|(scope, window)| serde_json::json!({ "scope": scope, "window": window }))
        .collect()
}

pub(crate) fn positive_limit(limits: &[Limit], scope: &Scope, window: Window) -> bool {
    limits
        .iter()
        .filter(|limit| limit.scope == *scope && limit.window == window)
        .any(|limit| {
            let amount = |unit| limit.resources.get(&unit).copied().unwrap_or(0);
            amount(Unit::UsdMicros) > 0 && amount(Unit::Requests) > 0
        })
}

pub(crate) fn resources(requests: u64, usd_micros: u64) -> Resources {
    BTreeMap::from([(Unit::Requests, requests), (Unit::UsdMicros, usd_micros)])
}

fn digest<T: Serialize>(value: &T) -> Result<String, Diagnostic> {
    retold_engine::store::hash(value).map_err(|_| invalid("Could not hash the plan."))
}

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, message)
}
