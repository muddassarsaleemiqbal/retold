# Phase 3Q: provider login, multi-workspace/selected-video backup, and cross-installation reconciliation

## 1. Provider Login (`providers login`)
- Implemented offline credential profile inspection and configuration for CLI and API-key-based providers (`claude_cli`, `codex_cli`, `openrouter`).
- Added `retold-next providers login [PROVIDER_ID]` with `--profile-dir`, `--key-env`, and `--dry-run`.
- Reads provider settings offline and verifies presence of credential directories/environment variables without making external network calls or leaking secrets.
- Validated via focused CLI tests in `crates/retold-cli/tests/provider_login.rs`.

## 2. Multi-Workspace & Selected-Video Backup Closure
- Extended `retold-next storage backup` with `--all` (backing up all registered workspaces in the canonical account) and `--video <VIDEO_ID>` (creating a targeted backup for a specific video run while omitting unrelated videos).
- Full support for `--dry-run` inspection of multi-workspace and selected-video backups.
- Validated in `crates/retold-cli/tests/storage_backup.rs`.

## 3. Cross-Installation Portable Reconciliation
- Implemented cross-installation portable workspace activation via `retold-next storage activate RESTORED_ROOT --reconcile --attest-prior-stopped --plan-hash HASH`.
- Where source and target canonical accounts differ, reconciliation computes a deterministic `reconciliation_plan` with a unique `plan_hash` covering source account ID, target account ID, workspace ID, target root, and backup index hash.
- Requires explicit user attestation (`--attest-prior-stopped`) confirming the prior machine installation has stopped.
- Rebinds the restored workspace to the target account and removes inactive markers without overwriting target financial accounting or auto-dispatching work.
- Validated in `crates/retold-cli/tests/storage_backup/activation.rs`.

## 4. Quality Gates
All quality gates pass:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `python3 scripts/check-architecture.py`
- `cargo test -p retold --test clean_code` against `clean-code.toml` preset `strictest` (zero new findings).
- `./scripts/check.sh --fast`
- `./scripts/check.sh`
