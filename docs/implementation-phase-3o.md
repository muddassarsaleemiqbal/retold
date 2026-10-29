# Phase 3O: inactive offline workspace restore

`retold-next storage restore BACKUP_DIRECTORY --into NEW_DIRECTORY [--dry-run]`
restores a version 2 managed backup into a new directory whose parent already exists.
Version 1 byte-only indexes, corrupt/schema-inconsistent backups and existing destinations
are refused. Preview verifies the backup and reports private content and account metadata
without creating folders, locks or accounts. Plain and JSON commands share application logic.

Restore copies only the pinned index entries into private sibling staging, verifies all copied
bytes and managed/account schemas again, then moves the managed workspace files to the new
root. Source changes between verification and copying fail checks rather than publish unchecked
bytes. The shared atomic no-replace publication protects against destination races. Ordinary
failures clean staging; a killed process can leave private unreferenced staging but cannot publish
a partial root. A final parent-sync failure may leave a complete destination and reports that
possibility. No archive commands or provider processes are run.

The new workspace and each run receive `.retold-restore.json` markers recording original
workspace/account identity, a digest of the recovery index, stopped-queue status and absence of
execution authority. Account registration, job admission/reservation and engine store ownership
refuse marked roots or descendants before dispatch/lock creation. Existing-ancestor resolution
also covers missing future run paths and symlink aliases. Invalid or unreadable markers block
conservatively. This is a local application guard, not protection against a user deliberately
removing recovery metadata or changing the executable.

Run manifests, approvals, cache evidence and media are preserved. Saved `state.json` active
fields/tasks are cleared; exact original state bytes are retained under
`.retold-recovery/original-state/`. No queue or saved account job is admitted automatically.
`.retold-recovery/` retains the original workspace lineage, index and account snapshot as
historical evidence only. The original index describes backup paths; it is not a new live backup
of the transformed restore tree. Canonical accounting is never opened, created, imported or
replaced. Newer canonical limits/charges therefore remain authoritative.

The restored root can be selected with `--library NEW_DIRECTORY` for offline list/show
inspection without the original root or a local account. Plan queries remain read-only and may
report missing provider/guide configuration. Activation is intentionally unavailable;
removing a marker is not a supported activation procedure. Future activation must reconcile
canonical financial lineage and exclusive ownership before revoking inactivity.

Focused proof covers read-only preview, round-trip manifest/account evidence, stopped tasks with
original state retained, newer canonical limits preserved, offline restoration with missing
original root/account, read-only video listing, existing destinations, corrupt/version-1
archives, run/root registration and engine-write refusal, stale admitted-job refusal and
symlink/missing-descendant guards. No live provider calls or media exports.

Remaining F18 work: explicit activation/financial reconciliation, multi-workspace and selected-run
backup closure, external asset/licence references and broader crash/kill acceptance coverage.

Focused validation: CLI backup/verification suites (26 tests), account suite (18 tests),
and engine inactivity guards (2 tests) passed. Formatting, strict Clippy and the unchanged
clean-code baseline passed; the full repository gate runs before publication.
