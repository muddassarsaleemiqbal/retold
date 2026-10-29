# Phase 3E: safe provider removal

```text
retold-next providers remove ID --root CONFIG_DIR [--dry-run]
```

Removes one existing, unreferenced provider definition. Any reference in any route,
including fallback entries, blocks removal with `RG-INPUT-REQUIRED`; replace those
routes explicitly first. Unknown or invalid IDs are refused. Routes, remaining provider
order and typed definitions are preserved. Credentials, login profiles, binaries and
model files are never deleted or inspected.

The application use case delegates to the existing provider-file transaction: canonical
root, nonblocking writer lock, re-read and full validation under lock, exact-byte backup,
and atomic publication. Existing invalid or future-schema files are refused. Dry run
performs validation without locks, backups or writes. JSON returns `data.removed` with
ID, dry-run flag and backup path. Storage errors and writer contention return
`RG-EXECUTION-FAILED`. Publication failures have the same inspect-before-retry limits
as Phase 3C. This does not coordinate with legacy writers or already-running jobs.

Regression coverage exercises preview/apply, exact backup bytes, preservation of unrelated
providers and routes, referenced/missing/invalid IDs, writer contention, future-schema
rejection and offline CLI JSON outcomes. No provider calls or media exports are needed.

Provider health/test/login, guided editing, migrations and workspace backup/restore
remain pending Phase 3 work.

Local validation: `cargo test --workspace --all-targets` passed 479 tests with
zero failures. Strict workspace Clippy, the unchanged Clean Code baseline,
formatting, architecture and whitespace checks passed. All configuration mutations
used temporary roots; no real provider requests, login changes or media exports
were made. Changes remain uncommitted.
