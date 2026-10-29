# Phase 3P: same-installation workspace activation

`retold-next storage activate RESTORED_ROOT --dry-run` and
`retold-next storage activate RESTORED_ROOT --plan-hash HASH`
review and safely apply a same-installation workspace ownership transfer from an original root
to an inactive restored root.

Activation verifies continuity between the canonical account journal and the snapshot captured
at backup time. Account ID, timezone, audit journal prefix, job histories, and attempt reservations
must match. Canonical accounting is authoritative and is never replaced or rolled back; newer charges,
holds, limits, and audit events remain strictly intact.

Activation validates the complete restored directory inventory against the pinned backup index
and recovery evidence under `.retold-recovery/`. Injected files, modified runs, unmanaged hooks,
or tampered state files fail closed. The target workspace must have intact inactive markers and
must not already be occupied by another registered workspace.

During activation:
1. Dry run produces a deterministic `activation_plan` with a SHA-256 `plan_hash` covering the financial
   transfer specification and canonical state. Execution requires supplying this exact `--plan-hash`.
2. Activation verifies that the account is not busy, acquires locks on active runs in the original root,
   and confirms the plan hash against the current live state.
3. If valid, an atomic recovery receipt is written and original roots/videos are retired by installing
   retired inactive markers referencing the transfer ID and target root.
4. Canonical account workspace registration is updated to the target root within an atomic database
   transaction recording a `workspace_activated` audit event.
5. Inactive markers are removed from the restored target and its video directories, enabling execution
   authority while keeping queues stopped.

Focused validation covers:
- Reviewed activation preserving newer limits, attempts, and jobs while retiring original root.
- Re-activation idempotency after successful transfer.
- Refusal of activation with stale plan hashes or busy account states.
- Refusal when presented with another account or missing original root.
- Refusal when history is modified or unmanaged hooks are injected.
- Canonical CLI `--dry-run` and `--plan-hash` command pipeline.

All repository quality gates passed:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `python3 scripts/check-architecture.py`
- `cargo test -p retold --test clean_code` against `clean-code.toml` preset `strictest` (zero new findings).
- `./scripts/check.sh` full test suite and quality gates.
