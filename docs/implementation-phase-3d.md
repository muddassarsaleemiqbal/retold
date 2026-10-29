# Phase 3D: explicit provider routing

```text
retold-next providers route --root CONFIG_DIR
retold-next providers route TIER --root CONFIG_DIR --order PROVIDER:SELECTOR,... [--dry-run]
```

Inspection lists every configured tier and its ordered fallback chain. Plain output uses
`text.smart: codex:smart -> claude:smart`; JSON returns `data.routing`. An empty routing table
is valid and gets an explicit empty-state message. Updates replace exactly one tier's chain;
other routes and all typed provider definitions are preserved. The root and providers.toml
must already exist. Neither operation creates default providers or selects implicit routes.

Every entry uses `provider:selector`, including the first entry. Selectors remain the existing
adapter model aliases or model names. Tier and order must be supplied together. An order must
contain 1–32 unique entries within 8192 bytes. Surrounding whitespace is trimmed; empty entries,
embedded whitespace, control characters, missing selectors and unknown providers are refused.

Updates check adapter support: Claude and Codex support text/vision, Gemini and local text
support text, Codex also supports images, local voice supports TTS, local Whisper supports
transcription, and OpenRouter supports the configured remote capabilities. `text.local` accepts
only local text providers, so a cloud or subscription fallback cannot be added to that chain.
These checks do not establish the selected model's capabilities, file availability, login
health, quota identity or readiness. Existing model resolution and runtime authorization still
apply when work is actually dispatched. Inspection does not certify old routes as compatible.

Writes reuse [Phase 3C's provider-file transaction](implementation-phase-3c.md): canonical root,
nonblocking `.providers.lock`, re-read and validation under the lock, an exact-byte timestamped
backup before replacement, and flushed atomic publication. Target/input size and regular-file
restrictions apply. Configuration schema versions, unknown fields, references and concurrency
are validated before publication. Invalid or future files are never silently repaired. An apply
failure may leave the lock file; a publication failure can leave a backup or committed target,
so inspect the target before retrying. Legacy writers do not honor this lock.

Dry run validates without locks, backups or writes. Apply and preview return `data.route`
containing the tier, normalized order, dry-run flag and backup path (null for preview). Input
errors use `RG-INPUT-REQUIRED`; writer contention and storage errors use `RG-EXECUTION-FAILED`.
Diagnostics omit input values and parser excerpts. Plain inspection sanitizes terminal controls.
No operation starts provider binaries, contacts a provider, loads credentials, changes logins,
grants dispatch authority or exports media.

This completes the bounded route-editing slice. Provider health/test/login, guided provider
editing/removal, migrations and workspace backup/restore remain pending Phase 3 work.

Regression coverage includes exact backups, preservation of unrelated configuration, ordered
fallbacks, offline CLI operation, plain/JSON output, incomplete flags, incompatible adapters,
local-only restrictions, invalid/future source files, writer contention and order limits.

Validation: `cargo test --workspace --all-targets` passed 476 tests with no failures. Workspace
strict Clippy and acceptance-feature Clippy passed, along with the existing Clean Code baseline
(unchanged), formatting, architecture, unused-dependency checks, cargo-deny policy checks,
warnings-denied documentation and whitespace checks. CLI tests ran with an empty PATH and all
configuration changes used temporary roots. No real provider requests, login changes, editor
launches or production video exports were made. Changes remain uncommitted.
