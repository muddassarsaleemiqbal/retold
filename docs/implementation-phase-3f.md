# Phase 3F: explicit provider definition editing

```text
retold-next providers edit ID --root CONFIG_DIR --file DEFINITION [--dry-run]
```

Replaces one existing provider definition from a complete typed TOML definition,
using the same input format and limits as `providers add`. Omitted optional fields
return to their schema defaults; this is a whole-definition replacement, not a patch.
Unknown IDs are refused; use `add` explicitly for new providers.

Every existing route referencing this ID, including fallback positions, must be
supported by the replacement adapter. The local-only restriction on `text.local`
still applies. Model selectors, model files, credentials, quota identity and login
health are not verified. Routes and the order and definitions of other providers
are preserved. No adapter is constructed or invoked.

Edits reuse the provider-file transaction: canonical root, nonblocking writer lock,
re-read and validation under lock, exact-byte recovery backup and atomic publication.
Invalid/future source configuration is refused. Dry run validates without locks,
backups or writes. JSON returns `data.edited` with ID, dry-run flag and backup path.
Input failures return `RG-INPUT-REQUIRED`; storage errors/contention return
`RG-EXECUTION-FAILED`. Errors omit parser excerpts and input values.

Phase 3C's publication-failure caveats apply: inspect the target before retrying.
Legacy writers and already-running jobs do not participate in this lock protocol.
Editing changes future configuration only and grants no dispatch or export authority.
No credentials, profiles, provider binaries or model files are changed.

Regression coverage includes preview/apply, exact backup bytes, preservation of
routes and unrelated definitions, incompatible fallback adapters, missing IDs,
future files, writer contention, redacted invalid definitions and offline CLI JSON.

Interactive editing, provider health/test/login, migrations and workspace
backup/restore remain pending Phase 3 work.

Local validation: all application/CLI targets passed 64 tests, provider-settings and
provider-route engine suites passed 16 tests, and the unchanged Clean Code baseline
passed. Strict workspace Clippy, formatting, architecture and whitespace checks
passed. The full workspace runtime suite was not rerun for this slice. Configuration
writes used temporary roots; no real provider calls, login changes or video exports
were made. Changes remain uncommitted.
