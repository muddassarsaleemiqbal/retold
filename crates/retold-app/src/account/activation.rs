//! Same-installation root transfer preserves the authoritative journal and never imports a ledger.
use super::{Account, Error, Result, backup, database, state::State, types::Workspace};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const ACTIVATION_EVENT: &str = "workspace_activated";
const RECONCILIATION_EVENT: &str = "workspace_reconciled";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Commitment {
    Committed,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transfer {
    pub original: Workspace,
    pub target: PathBuf,
    pub snapshot: PathBuf,
    pub index_blake3: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub transfer: Transfer,
    pub transfer_id: String,
    pub account_id: String,
    pub canonical_state_hash: String,
    pub committed: bool,
    pub plan_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationPlan {
    pub transfer: Transfer,
    pub source_account_id: String,
    pub target_account_id: String,
    pub workspace_id: String,
    pub target_root: PathBuf,
    pub committed: bool,
    pub plan_hash: String,
}

impl Plan {
    pub(crate) fn commit(&self, maintenance: &backup::Maintenance<'_>) -> Result<()> {
        let current = maintenance.account.transfer_plan(&self.transfer)?;
        if current.committed {
            return Ok(());
        }
        if current.plan_hash != self.plan_hash {
            return Err(Error::Invalid(
                "Activation plan changed; review a fresh dry run.",
            ));
        }
        self.apply_transaction(maintenance.account)
    }

    fn apply_transaction(&self, account: &Account) -> Result<()> {
        let now = account.clock.now();
        database::transaction(&account.directory, now, |state| {
            let current = plan(&state, &self.transfer)?;
            if current.plan_hash != self.plan_hash {
                return Err(Error::Invalid(
                    "Activation plan changed; nothing was transferred.",
                ));
            }
            let updated = apply_transfer(state, &self.transfer, &self.transfer_id);
            Ok((updated, ()))
        })
    }
}

impl ReconciliationPlan {
    pub(crate) fn commit(&self, maintenance: &backup::Maintenance<'_>) -> Result<()> {
        let current = maintenance.account.reconciliation_plan(&self.transfer)?;
        if current.committed {
            return Ok(());
        }
        if current.plan_hash != self.plan_hash {
            return Err(Error::Invalid(
                "Reconciliation plan changed; review a fresh dry run.",
            ));
        }
        self.apply_transaction(maintenance.account)
    }

    fn apply_transaction(&self, account: &Account) -> Result<()> {
        let now = account.clock.now();
        database::transaction(&account.directory, now, |mut state| {
            let current = account.reconciliation_plan(&self.transfer)?;
            if current.plan_hash != self.plan_hash {
                return Err(Error::Invalid(
                    "Reconciliation plan changed; nothing was transferred.",
                ));
            }
            let workspace = Workspace {
                id: self.workspace_id.clone(),
                root: self.target_root.clone(),
            };
            state.workspaces.insert(workspace.id.clone(), workspace);
            state.note(RECONCILIATION_EVENT, &self.plan_hash);
            Ok((state, ()))
        })
    }
}

fn apply_transfer(mut state: State, transfer: &Transfer, transfer_id: &str) -> State {
    let workspace = Workspace {
        id: transfer.original.id.clone(),
        root: transfer.target.clone(),
    };
    state.workspaces.insert(workspace.id.clone(), workspace);
    state.note(ACTIVATION_EVENT, transfer_id);
    state
}

impl Account {
    /// Read-only proof. No account, registry, owner or marker is created.
    pub fn transfer_plan(&self, transfer: &Transfer) -> Result<Plan> {
        let current = database::read(&self.directory)?;
        let archived = verified_snapshot(transfer)?;
        validate_continuity(&current, &archived)?;
        validate_paths(self, transfer)?;
        plan(&current, transfer)
    }

    pub fn reconciliation_plan(&self, transfer: &Transfer) -> Result<ReconciliationPlan> {
        transfer.plan_reconciliation(self)
    }
}

impl Transfer {
    pub fn plan_reconciliation(&self, account: &Account) -> Result<ReconciliationPlan> {
        let current = database::read(&account.directory)?;
        let archived = verified_snapshot(self)?;
        let target = canonical_target(&self.target)?;
        let account_root = account
            .directory
            .canonicalize()
            .map_err(|_| Error::Corrupt)?;
        let overlaps = paths_overlap(&target, &account_root);
        if overlaps {
            return Err(Error::Invalid(
                "Activation requires target workspace separate from account root.",
            ));
        }
        let context = ReconcileContext {
            current,
            archived,
            target,
        };
        build_reconcile_plan(self, context)
    }
}

struct ReconcileContext {
    current: State,
    archived: State,
    target: PathBuf,
}

fn build_reconcile_plan(
    transfer: &Transfer,
    context: ReconcileContext,
) -> Result<ReconciliationPlan> {
    let workspace_id = transfer.original.id.clone();
    let plan_hash = hash(&(
        &context.current.account_id,
        &context.archived.account_id,
        &workspace_id,
        &context.target,
        &transfer.index_blake3,
    ))?;
    let committed = context.current.audit.iter().any(|audit_event| {
        audit_event.kind == RECONCILIATION_EVENT && audit_event.entity == plan_hash
    });
    Ok(ReconciliationPlan {
        transfer: transfer.clone(),
        source_account_id: context.archived.account_id,
        target_account_id: context.current.account_id,
        workspace_id,
        target_root: context.target,
        committed,
        plan_hash,
    })
}

impl backup::Maintenance<'_> {
    pub(crate) fn transfer(&self, reviewed: &Plan) -> Result<()> {
        reviewed.commit(self)
    }

    pub(crate) fn reconcile_transfer(&self, reviewed: &ReconciliationPlan) -> Result<()> {
        reviewed.commit(self)
    }
}

fn verified_snapshot(transfer: &Transfer) -> Result<State> {
    let canonical_account = None;
    backup::verify(&transfer.snapshot, canonical_account)?;
    let archived = database::read(&transfer.snapshot)?;
    let registered = archived.workspaces.get(&transfer.original.id) == Some(&transfer.original);
    if !registered {
        return Err(Error::Invalid(
            "Recovery evidence disagrees with original workspace registration.",
        ));
    }
    Ok(archived)
}

fn continuity_intact(current: &State, archived: &State) -> bool {
    let same = current.account_id == archived.account_id && current.timezone == archived.timezone;
    let journal =
        current.audit.starts_with(&archived.audit) && current.last_seen >= archived.last_seen;
    same && journal
}

fn operations_intact(current: &State, archived: &State) -> bool {
    let jobs = archived
        .jobs
        .iter()
        .all(|(id, job)| current.jobs.get(id) == Some(job));
    let attempts = archived.attempts.iter().all(|(id, attempt)| {
        current
            .attempts
            .get(id)
            .is_some_and(|now| now.reservation == attempt.reservation)
    });
    jobs && attempts
}

fn validate_continuity(current: &State, archived: &State) -> Result<()> {
    let continuous = continuity_intact(current, archived) && operations_intact(current, archived);
    if !continuous {
        return Err(Error::Invalid(
            "Canonical accounting cannot prove continuity with this snapshot. Restore remains inactive; financial transfer/reconciliation is required.",
        ));
    }
    Ok(())
}

fn canonical_source(path: &Path) -> Result<PathBuf> {
    path.canonicalize().map_err(|_| {
        Error::Invalid("Original root must be present for this local ownership transfer.")
    })
}

fn canonical_target(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|_| Error::Invalid("Restored root is unavailable."))
}

fn paths_overlap(first: &Path, second: &Path) -> bool {
    first.starts_with(second) || second.starts_with(first)
}

fn paths_separate(source: &Path, target: &Path, account: &Path) -> bool {
    !paths_overlap(source, target) && !paths_overlap(account, target)
}

fn roots_canonical(source: &Path, target: &Path, transfer: &Transfer) -> bool {
    source == transfer.original.root && target == transfer.target
}

fn validate_paths(account: &Account, transfer: &Transfer) -> Result<()> {
    let source = canonical_source(&transfer.original.root)?;
    let target = canonical_target(&transfer.target)?;
    let account_root = account
        .directory
        .canonicalize()
        .map_err(|_| Error::Corrupt)?;
    let valid = roots_canonical(&source, &target, transfer)
        && paths_separate(&source, &target, &account_root);
    if !valid {
        return Err(Error::Invalid(
            "Activation requires separate canonical workspace and account roots.",
        ));
    }
    Ok(())
}

fn plan(state: &State, transfer: &Transfer) -> Result<Plan> {
    let transfer_id = hash(&(
        state.account_id.clone(),
        &transfer.original,
        &transfer.target,
        &transfer.index_blake3,
    ))?;
    let committed = state.audit.iter().any(|audit_event| {
        audit_event.kind == ACTIVATION_EVENT && audit_event.entity == transfer_id
    });
    let commitment = if committed {
        Commitment::Committed
    } else {
        Commitment::Pending
    };
    validate_binding(state, transfer, commitment)?;
    let canonical_state_hash = state_hash(state)?;
    let plan_hash = hash(&(&transfer_id, &canonical_state_hash))?;
    Ok(Plan {
        transfer: transfer.clone(),
        transfer_id,
        account_id: state.account_id.clone(),
        canonical_state_hash,
        committed,
        plan_hash,
    })
}

fn validate_binding(state: &State, transfer: &Transfer, commitment: Commitment) -> Result<()> {
    let current = state
        .workspaces
        .get(&transfer.original.id)
        .ok_or(Error::Invalid(
            "Original workspace is no longer registered.",
        ))?;
    let expected = match commitment {
        Commitment::Committed => &transfer.target,
        Commitment::Pending => &transfer.original.root,
    };
    let occupied = state
        .workspaces
        .iter()
        .any(|(id, workspace)| id != &current.id && workspace.root == transfer.target);
    if current.root != *expected || occupied {
        return Err(Error::Invalid(
            "Another root owns this workspace lineage; activation is refused.",
        ));
    }
    Ok(())
}

fn state_hash(state: &State) -> Result<String> {
    hash(&(
        &state.account_id,
        &state.timezone,
        &state.workspaces,
        &state.routes,
        &state.limits,
        &state.jobs,
        &state.attempts,
        &state.audit,
    ))
}

fn hash(value: &impl Serialize) -> Result<String> {
    retold_engine::store::hash(value).map_err(|_| Error::Corrupt)
}
