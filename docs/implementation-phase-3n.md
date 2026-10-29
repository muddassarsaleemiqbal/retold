# Phase 3N: managed backup semantic validation

New managed backups use index schema version 2. `storage verify` first checks bounded
paths, inventory and checksums, then validates the strict workspace lineage schema,
fixed archive mappings, original workspace registration in the archived account,
account identity and financial manifest/database agreement. The original filesystem
root is compared as historical evidence; it need not exist on this machine.

The archived workspace is discovered offline using the normal bounded manifest parser.
Its unique run IDs must exactly match lineage. Recorded cache envelopes must be intact.
The managed file selection must exactly match indexed content, excluding unregistered
files, secrets, locks and executable content. Verification is read-only, uses no provider
commands and grants no execution authority.

Version 1 indexes remain supported as byte-only evidence. The response explicitly reports
`managed_workspace_verified: false` for them and `true` only for successful version 2
validation. Downgrading a version never claims semantic verification. This distinguishes
older checksummed snapshots from the stronger precondition needed by future restore.

Regression coverage includes valid offline verification, rehashed lineage/identity/schema
changes, mismatched original registration, invalid run/account manifests, duplicate or
missing run IDs, and checksummed unmanaged content. Restore, activation, external reference
and licence closure, multi-workspace and selected-run backup remain pending.

Validation: focused CLI backup/verification suites passed (19 tests), followed by
`./scripts/check.sh --fast` with formatting, strict Clippy and the unchanged clean-code
baseline. The full repository gate runs before publication. No live calls or exports.
