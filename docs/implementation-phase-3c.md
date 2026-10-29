# Phase 3C: typed provider import

```text
retold-next providers add <id> --root CONFIG_DIR --file PROVIDER_TOML [--dry-run]
```

The explicit configuration root must already exist. The input is one provider definition,
using the existing strict provider schema without a `[providers]` wrapper. For example:

```toml
kind = "codex_cli"
config_dir = "/Users/you/.codex_personal"
max_parallel = 2

[tiers]
smart = "your-configured-model"
```

Import validates the definition and existing provider configuration, rejects duplicate IDs,
unknown fields, unsupported schema versions and invalid concurrency/routes, then validates
the combined file before publication. IDs use 1–64 ASCII letters, digits, hyphens or underscores.
Inputs and serialized output have a 2 MiB limit. Input files, targets and existing lock files
must be regular files; symlinks are refused. Configuration roots are canonicalized so aliases
of the same folder use the same writer lock.

`--dry-run` reads and validates without creating locks, backups, configuration files or folders.
An apply acquires `.providers.lock` using a nonblocking OS lock and re-reads the target under
that lock. Contention returns an execution error; it never starts a second writer. The lock
file remains after use; the OS releases ownership when its file handle closes, including after
a process dies. Existing legacy writers do not honor this new lock and must not run concurrently.

Before replacing an existing file, import durably publishes a timestamped, non-overwriting
`providers.toml.backup-<unix-nanoseconds>` containing its exact prior bytes. If the backup fails,
the target is not replaced. Publication reuses the engine's flushed temporary-file/atomic-rename
path. A later failure may leave a valid backup or a committed target; inspect the target before
retrying. Failed apply may leave a lock file. Invalid-definition and invalid-ID failures occur
before locking. Missing `providers.toml` starts an empty provider configuration with no routes.

Existing providers and route chains retain their typed values; normal TOML serialization may
change formatting and comments in the active file. The backup retains the original formatting.
JSON includes the provider ID, dry-run flag and backup path (null for a first import or preview).
Errors are redacted and never include source excerpts. Plain output reports the operation's
result; JSON exposes the backup location.

The command never dispatches requests, starts a binary/editor, changes logins, loads credentials,
or selects new routes. API-backed definitions use the existing `key_env` reference; literal key
fields are refused by the schema. Profile paths and model names are stored, not health-checked.
No canonical quota identity, accounting limits or dispatch authority are created by an import.

This is a bounded Phase 3 slice. Guided provider creation, Keychain integration, health/version
checks, provider tests, route editing, migrations and portable backup/restore remain pending.
The timestamped provider-file backup is recovery for this write, not an F18 workspace backup.

Regression tests cover read-only previews, empty initialization, exact backups, preservation of
existing providers/routes, duplicate refusal, invalid/future configuration, writer contention,
symlink refusal, invalid IDs, oversized definitions, redacted errors and CLI operation with
an empty PATH. All fixtures use temporary folders; no real configuration is changed.

Validation: 467 workspace tests passed (`cargo test --workspace --all-targets`).
Strict Clippy passed for engine, app and CLI, including test targets. Clean Code
passed without changing the baseline for this slice; architecture, formatting,
whitespace and warnings-denied documentation checks also passed.
