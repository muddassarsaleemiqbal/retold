//! Reviewed same-installation ownership transfer. Markers fail closed around the journal commit.
mod evidence;
mod markers;

use super::{creation, invalid, restoration::Inactive};
use crate::{
    account::{Account, activation::Plan, commands},
    protocol::{Diagnostic, ErrorCode, Outcome, Payload},
};
use evidence::{Evidence, RECEIPT_FILE, RECOVERY};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct ReconcileConfirmation<'a> {
    pub plan_hash: &'a str,
    pub attest_prior_stopped: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Mode<'a> {
    Preview,
    Activate(&'a str),
    ReconcilePreview,
    ReconcileActivate(ReconcileConfirmation<'a>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    plan_hash: String,
    financial: Plan,
    inactive: Inactive,
}

struct Review {
    financial: Plan,
    original: crate::library::Catalog,
    plan_hash: String,
}

#[must_use]
pub fn respond(root: &Path, mode: Mode<'_>) -> Payload {
    let result = canonical_account().and_then(|account| apply(&account, root, mode));
    match result {
        Ok(contents) => Payload::Result {
            outcome: Outcome::Completed,
            contents,
        },
        Err(error) => error.into_payload(),
    }
}

/// Explicit accounts are for isolated embedders/tests; CLI only opens canonical accounting.
pub fn apply(
    account: &Account,
    root: &Path,
    mode: Mode<'_>,
) -> Result<serde_json::Value, Diagnostic> {
    match mode {
        Mode::Preview => {
            let restored = Evidence::load(root)?;
            let review = review(account, &restored)?;
            Ok(
                serde_json::json!({"text":"Read-only activation plan. Transfers ownership from the original root; preserves canonical charges, holds and limits. Queues remain stopped; no work is dispatched.",
                "activation_plan": {"plan_hash":review.plan_hash,"financial":review.financial,
                    "original_videos":review.original.videos,"restored_videos":restored.catalog.videos,
                    "live_snapshot":true,"execution_authorized":false}}),
            )
        }
        Mode::Activate(plan_hash) => activate(account, root, plan_hash),
        Mode::ReconcilePreview => reconcile_preview(account, root),
        Mode::ReconcileActivate(confirmation) => reconcile_activate(account, confirmation, root),
    }
}

fn review(account: &Account, restored: &Evidence) -> Result<Review, Diagnostic> {
    let financial = account
        .transfer_plan(&restored.transfer)
        .map_err(|error| commands::diagnostic(&error))?;
    if !financial.committed && !restored.fully_inactive {
        return Err(invalid());
    }
    let original = crate::discovery::scan(&restored.transfer.original.root)?;
    let complete = original.issues.is_empty();
    if !complete {
        return Err(invalid());
    }
    let plan_hash =
        retold_engine::store::hash(&(&financial.plan_hash, &original)).map_err(|_| invalid())?;
    Ok(Review {
        financial,
        original,
        plan_hash,
    })
}

fn activate(
    account: &Account,
    root: &Path,
    reviewed: &str,
) -> Result<serde_json::Value, Diagnostic> {
    let ownership = account
        .maintenance()
        .map_err(|error| commands::diagnostic(&error))?;
    let restored = Evidence::load(root)?;
    let current = review(account, &restored)?;
    let _original_runs = creation::lock_runs(&current.original)?;
    confirm(&restored, &current, reviewed)?;
    markers::prepare(&restored, &current)?;
    ownership
        .transfer(&current.financial)
        .map_err(|error| commands::diagnostic(&error))?;
    markers::enable(&restored)?;
    Ok(
        serde_json::json!({"text":"Workspace activated on this installation. Original root is retired. Canonical financial history and stopped queues are preserved; no job was admitted or dispatched.",
        "workspace_activated":{"root":restored.root,"workspace_id":restored.inactive.workspace_id,
            "transfer_id":current.financial.transfer_id,"queues_stopped":true,"accounting_replaced":false}}),
    )
}

fn reconcile_preview(account: &Account, root: &Path) -> Result<serde_json::Value, Diagnostic> {
    let restored = Evidence::load(root)?;
    let inactive = restored.fully_inactive;
    if !inactive {
        return Err(invalid());
    }
    let plan = account
        .reconciliation_plan(&restored.transfer)
        .map_err(|error| commands::diagnostic(&error))?;
    Ok(serde_json::json!({
        "text": "Read-only cross-installation reconciliation plan. Rebinds restored workspace to target canonical account without rolling back charges or limits. Queues remain stopped; no work is dispatched.",
        "reconciliation_plan": {
            "plan_hash": plan.plan_hash,
            "source_account_id": plan.source_account_id,
            "target_account_id": plan.target_account_id,
            "workspace_id": plan.workspace_id,
            "target_root": plan.target_root,
            "restored_videos": restored.catalog.videos,
            "live_snapshot": true,
            "execution_authorized": false,
        }
    }))
}

impl ReconcileConfirmation<'_> {
    fn verify_attestation(&self) -> Result<(), Diagnostic> {
        if !self.attest_prior_stopped {
            return Err(Diagnostic::new(
                ErrorCode::InputRequired,
                "Cross-installation activation requires explicit --attest-prior-stopped confirming prior installation has stopped.",
            ));
        }
        Ok(())
    }
}

fn bind_reconciled_workspace(
    account: &Account,
    restored: &Evidence,
    expected_hash: &str,
) -> Result<String, Diagnostic> {
    let ownership = account
        .maintenance()
        .map_err(|error| commands::diagnostic(&error))?;
    let plan = account
        .reconciliation_plan(&restored.transfer)
        .map_err(|error| commands::diagnostic(&error))?;
    if plan.plan_hash != expected_hash {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "Reconciliation plan changed; review a fresh dry run.",
        ));
    }
    ownership
        .reconcile_transfer(&plan)
        .map_err(|error| commands::diagnostic(&error))?;
    markers::enable(restored)?;
    Ok(plan.workspace_id)
}

fn reconcile_activate(
    account: &Account,
    confirmation: ReconcileConfirmation<'_>,
    root: &Path,
) -> Result<serde_json::Value, Diagnostic> {
    confirmation.verify_attestation()?;
    let restored = Evidence::load(root)?;
    let target_account_id = account
        .canonical_account_id()
        .map_err(|error| commands::diagnostic(&error))?;
    let workspace_id = bind_reconciled_workspace(account, &restored, confirmation.plan_hash)?;
    Ok(serde_json::json!({
        "text": "Workspace successfully reconciled and activated in target account. Prior machine installation stopped by user attestation. Canonical financial history preserved; no job was admitted or dispatched.",
        "workspace_reconciled": {
            "root": restored.root,
            "workspace_id": workspace_id,
            "target_account_id": target_account_id,
            "queues_stopped": true,
            "accounting_replaced": false,
        }
    }))
}

fn confirm(restored: &Evidence, current: &Review, reviewed: &str) -> Result<(), Diagnostic> {
    let expected = if current.financial.committed {
        let receipt: Receipt = read_json(&restored.root.join(RECOVERY).join(RECEIPT_FILE))?;
        if receipt.financial.transfer_id != current.financial.transfer_id {
            return Err(invalid());
        }
        receipt.plan_hash
    } else {
        current.plan_hash.clone()
    };
    if expected != reviewed {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Activation plan changed. Review storage activate ROOT --dry-run and supply its --plan-hash HASH.",
        ));
    }
    Ok(())
}

fn canonical_account() -> Result<Account, Diagnostic> {
    let directory =
        crate::account::canonical_directory().map_err(|error| commands::diagnostic(&error))?;
    Account::open(&directory).map_err(|error| commands::diagnostic(&error))
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, Diagnostic> {
    super::regular_file(path)?;
    let bytes = retold_engine::fsx::read_bounded(path, super::INDEX_BYTES)
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}
