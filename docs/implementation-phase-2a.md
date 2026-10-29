# Phase 2A: saved-video queries and read-only inspection plans

Date: 2026-10-18. Status: implemented and locally verified.

This slice delivers usable `video list`, `video show <ref>` and `video plan <ref>` commands.
It does not complete Phase 2's execution/accounting exit gate. The plan command explicitly returns
`plan_kind: read_only_inspection`, `can_execute: false` and unknown cost/request/duration estimates.
The [Phase 2B account core](implementation-phase-2b.md) now supplies ledger and authority APIs.
Actual continue/export and current-input reuse qualification remain pending integration.

## Delivered behavior

- `--library PATH` explicitly selects the library; `RETOLD_LIBRARY` and the existing read-only
  settings discovery supply defaults. An absent library lists as empty without creating it.
- Discovery uses current engine run manifests, without aliases, migration or legacy adapters.
  It bounds traversal to 10,000 entries and depth 12, each manifest to 2 MiB and total manifest
  bytes to 64 MiB. Invalid manifests appear in list issues. Incomplete discovery blocks reference
  selection rather than silently picking a different video. Directory read/limit errors fail closed.
- Lists return metadata summaries, immutable IDs, canonical references, editions, metadata hashes,
  snapshot identity, total and next cursor. They filter by channel/search and accept 1–500 items,
  default 100. Ordering uses parsed UTC timestamps, including fractional seconds, then ID/ref.
- References resolve by immutable ID, exact reference/path/slug, then unique prefix. `last` is an
  explicit read-only alias. Ambiguous refs return candidates; absent refs never infer a default.
- Cursors bind the canonical root, observed metadata set, filters and ordering. A different root,
  changed metadata/filter or malformed cursor requires restarting the list. Page size can change.
  These are bounded observed filesystem snapshots, not a transactional database or job admission.
- Show/plan read state and referenced cache envelopes directly; they never call `Store::open`,
  take an execution lock, repair an index, restore a backup, create a cache or contact providers.
  State is reread to detect a concurrent change, and manifest identity is rechecked after inspection.
- Inspection is bounded to 2,000 records, 8 MiB per state/cache file and 64 MiB total cache reads.
  Each recorded entry is `envelope_verified`, `missing` or `invalid`, checking the engine envelope's
  schema, step, key and serialized-value hash. Cache path validation is shared with the store.
- Envelope integrity does not establish current-input validity, external media-byte integrity,
  QA acceptance, source provenance or review readiness. Reuse is deliberately unqualified here.
  Stage summaries show recorded/verified/unresolved evidence and declared engine dependencies;
  they are not a guide-specific executable schedule. Estimates remain unknown rather than zero.
- Invalid state and symlinked metadata/cache locations are rejected without repair. Discovery does
  not traverse nested directory symlinks. This is not a filesystem sandbox against hostile concurrent
  path replacement; execution-time ownership/current-input checks remain required in Phase 2B.
- Raw saved error text, inputs and cached generated/source content are not returned in routine
  snapshots. Saved failure is a boolean. Plain lists preserve Unicode and remove control characters.
- JSON remains a single versioned terminal envelope, with typed reference/cursor/data diagnostics
  and stable exit behavior. The published v1 envelope schema adds the new diagnostic codes.
- `video continue` and `video export` remain explicitly unavailable and dispatch no work, including
  with `--yes` or historical run approvals. No new resource authority or paid/media permission exists.

## Try it

```sh
cargo run -p retold-cli -- --library /path/to/library video list
cargo run -p retold-cli -- --library /path/to/library --json video list --limit 10
cargo run -p retold-cli -- --library /path/to/library video show VIDEO_ID
cargo run -p retold-cli -- --library /path/to/library --json video plan VIDEO_ID
```

## Next boundary: Phase 2B

Implement the canonical installation-account transactional resource ledger and fresh job authority,
including cross-process reservation/idempotency, uncertain-outcome holds and window rules, before
wiring real provider dispatch. Executable planning must qualify exact semantic/current input keys,
external artifact integrity and review gates under execution ownership. Continue must stop at ready;
only a separately scoped export request may open FilmCraft or render. Those gates remain pending.

## Verification

- `cargo test --workspace --all-targets`: passed; 414 tests passed, 0 ignored,
  across 56 test binaries, including zero-test targets.
- All 13 new library command tests pass: fractional-time ordering, snapshot/filter-bound pagination,
  explicit/ambiguous/missing references and matching exit codes, no-write planning, missing/corrupt/
  tampered envelopes, malformed/oversized manifests, invalid state, absent roots, symlink rejection,
  valid media-like folder names, and Hindi/Urdu/plain control-character handling.
- Existing NDJSON parser/terminal/schema/golden-envelope tests still pass. New diagnostic codes
  are additive in the published v1 envelope schema.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Acceptance-feature engine/FilmCraft Clippy: passed; compilation only, no live acceptance runs.
- Clean Code ratchet passed with no baseline changes in this slice.
- Formatting, architecture, `git diff --check`, unused-dependency and documentation checks passed.

Local terminal evidence: `/tmp/retold-phase2-tests.log`, `/tmp/retold-phase2-focused.log`,
`/tmp/retold-phase2-clippy.log`, `/tmp/retold-phase2-clean-code.log`,
`/tmp/retold-phase2-acceptance-clippy.log` and `/tmp/retold-phase2-docs.log`.
These are local checks, not CI or production proof. Tests use temporary libraries and synthetic
saved artifacts. No production provider calls, paid evaluation or video exports were performed.
No commit or push was performed. Pre-existing workspace changes were preserved.
