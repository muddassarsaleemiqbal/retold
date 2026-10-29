# Phase 3I: Provider Health Classification & Configuration Migration

## Provider Health Classification

Provider health is tracked durably with explicit status classifications:
- `ok`: Provider is verified operational, with consecutive failure counters reset to zero on success.
- `limited`: Active rate-limit cooldown with parsed or default reset duration.
- `logged_out`: Unauthenticated or missing credentials; blocks dispatch until authentication succeeds.
- `failing`: Consecutive unexpected failures tracked with error snippet; dispatch refuses when failure thresholds are exceeded.
- `not_checked`: No prior observation recorded.

### Reset-Time Parsing
Error strings from CLIs and API responses are parsed for reset windows across multiple formats:
- Pipe timestamps (e.g. `|1760000000`)
- Relative durations (e.g. `try again in 45s`, `resets in 2 minutes`)
- Time of day with 12h/24h support (e.g. `try again at 11:30 pm`, `resets at 2:45 PM`)
Fallback defaults to a bounded conservative window (120s) when no timestamp can be extracted.

### Health Service Integration
- `Health::succeeded(&self, id)` marks successful dispatch and resets consecutive failure counters.
- Bounded observation tracking protects against unbounded file growth.
- Sanitized IDs and error messages ensure credentials and API keys are never leaked to logs or stdout.

---

## Configuration Migration

`retold-next config migrate --root <PATH> [--dry-run]` provides automated, fail-safe schema migration:
- **Pre-migration backup**: Creates timestamped backups (e.g. `providers.toml.backup-<timestamp>`).
- **Atomic upgrades**: Staged through temporary files and committed atomically via filesystem rename.
- **Rollback on error**: If parsing or validation fails at any point during migration, the pre-migration state is preserved and restored.
- **Stale staging cleanup**: Automatically identifies and removes orphaned `.tmp` and `.staging` files left by interrupted operations.
- **Idempotency**: Repeated runs on up-to-date configuration report success without modifying files or creating redundant backups.
- **Dry-run mode**: `--dry-run` reports planned changes, backups, and target versions without writing to disk.

---

## Verification & Quality Gates

- **Unit & Integration Tests**:
  - `crates/retold-providers/src/error.rs`: Unit tests for timestamp parsing and failure classification.
  - `crates/retold-engine/tests/provider_health.rs`: Cooldown admission, consecutive failure tracking, and logged-out recovery tests.
  - `crates/retold-cli/tests/provider_health.rs`: CLI inspection and formatting for all health states.
  - `crates/retold-cli/tests/configuration_migrate.rs`: Schema migration, dry-run mode, backup creation, idempotent re-run, and staging file cleanup.
- **Clean Code & Linting**:
  - `cargo fmt --all --check`: clean.
  - `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings under pedantic + nursery.
  - `clean_code` strictest preset: improved all 3 metrics (`function-does-one-thing`, `no-magic-numbers`, `one-level-of-abstraction`), ratcheting baseline down.
  - `python3 scripts/check-architecture.py`: workspace layer boundaries preserved.
  - Zero live network provider calls, login modifications, or video exports.
