# Phase 3G: configured provider inventory

```text
retold-next providers list --root CONFIG_DIR
```

Reads validated `providers.toml` and lists configured providers in configuration
order. Plain and JSON output include the ID, adapter kind, maximum concurrency,
adapter-supported tiers and existing route assignments. Assignment positions are
one-based fallback positions. Unassigned providers and an empty configuration have
explicit plain states. Missing, invalid and future-schema configuration is refused
with `RG-INPUT-REQUIRED`; it is never replaced with an invented empty inventory.

The engine builds an explicit safe projection, not a serialization of provider
settings. It omits binary/model/profile paths, selector/model names, endpoint URLs
and credential-variable names. Plain labels have terminal controls sanitized.
Configured IDs remain visible; users should not put credentials in IDs.

Authentication and health are explicitly `not_checked`. Adapter support does not
prove selected-model capability, readiness, quota or login health. The command
constructs no adapter, starts no process, loads no credentials, sends no requests
and performs no writes or locking. This is a configuration snapshot, not a durable
health ledger. JSON returns `data.providers` as an array, including when empty.

Regression coverage verifies ordering, fallback positions, supported tiers,
redaction, unchanged source bytes, empty/missing/invalid/future inputs, single JSON
terminal envelopes, plain empty states and terminal-control sanitization. CLI tests
run with an empty PATH and configuration fixtures use temporary roots.

Provider health/test/login, interactive setup, migrations and workspace
backup/restore remain pending Phase 3 work.

Local validation: all app/CLI targets passed 66 tests; provider inventory/settings/
route engine suites passed 18 tests. The final plain/JSON list tests also passed
after renderer cleanup. Strict workspace Clippy, the unchanged Clean Code baseline,
formatting, architecture and whitespace checks passed. The full workspace runtime
suite was not rerun. No real provider calls, login changes or media exports were
made. Changes remain uncommitted.
