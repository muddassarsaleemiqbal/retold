# CLI rewrite: Phase 1 implementation

Date: 2026-10-18. Scope: application/protocol foundation and immediate branch failure events.
Status: implemented and locally verified. Broader companion C1 work remains pending.

The user explicitly removed legacy command compatibility from the current implementation scope.
No legacy command inventory, hidden aliases or adapters were implemented. The existing binary
remains available while the new interface is incomplete; the replacement is `retold-next`.

## Delivered behavior

- `retold-app` owns read-only usage, schema and diagnostic responses, without printing, prompts,
  presentation dependencies or provider dispatch.
- `retold-cli` provides `retold-next`, plain output, `--json`, `--quiet`, `--no-color`, help,
  version, `schema` and `explain <code>`. Bare invocation prints usage and finishes without a TUI.
- Every JSON invocation, including help/version/parser failures, emits one version-1 terminal
  envelope with a request ID. Exit codes are 0 success, 2 invalid inputs, 3 unavailable/blocked,
  and 4 execution/output failure. Quiet retains the terminal result. No ANSI or raw input values
  leak into parser-error envelopes. A broken stdout returns failure without a printing panic.
- The published [schema](schemas/retold-output-v1.json) and golden envelope are checked against
  the Rust protocol. Job IDs/sequences are optional; replay/admission arrive with later phases.
- New production command families currently return `RG-NOT-AVAILABLE` without creating files,
  opening editors, dispatching providers or exporting media. They are explicitly unavailable,
  not working production commands.
- Engine events carry optional typed details. Branch failures include stage, optional edition,
  optional one-based item, failure kind, code, safe reason and argv-form diagnostic action.
  Unknown branch failures remain unclassified execution failures and confer no retry authority.
- Image/language branches, individual language tasks and character/frame branches report failure
  before their joins wait for peers. Independent peers continue and successful results remain
  available under existing engine behavior. Generic observer tests use a channel barrier to prove
  the failure is durably logged while the peer is still running; they verify redaction as well.
- Architecture checks recognize both binaries and forbid presentation dependencies in the app.
  Small media-coordination helpers keep asynchronous futures and function sizes within lint limits.

## Try it

```sh
cargo run -p retold-cli -- --help
cargo run -p retold-cli -- --json schema
cargo run -p retold-cli -- explain RG-EXECUTION-FAILED
```

## Next implementation boundary

[Phase 2A](implementation-phase-2a.md) now implements saved-video discovery, show and read-only
inspection plans. Execution/accounting below remains the next boundary.

Phase 2 owns real video commands/reference resolution, cache-only plans, safe continue/export,
fresh execution authority and shared transactional resource accounting. Production execution is
not wired into this first CLI slice. Studio replay, TUI, migration and richer task-specific failure
classification remain in their assigned phases. The wider companion C1 health/repair/cache work
is not claimed complete by the branch-event change. No implementation or measured savings for the
revision 11 algorithm candidates is claimed here.

## Verification

- `cargo test --workspace --all-targets`: passed, 401 tests passed, 0 ignored,
  across 55 test binaries (including zero-test targets). Thirteen new protocol/event tests pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo clippy -p retold-engine -p retold-filmcraft --all-targets --features acceptance -- -D warnings`: passed; compilation only, no live acceptance runs.
- `cargo fmt --all --check`, architecture checks and `git diff --check`: passed.
- Clean Code ratchet: passed. Only this slice's one-level-of-abstraction allowance was lowered
  from 393 to 392; pre-existing baseline edits were preserved and no allowance was raised.
- `cargo machete`: no unused dependencies.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps`: passed.
- Manual smoke: plain usage, JSON diagnostic result, and closed stdout (exit 4, no panic/stderr).
- Published schema/golden envelope, parser-error flag order, secret canary suppression, no-mutation
  unavailable actions and closed-stdin behavior are covered by the new tests.

Terminal logs from this run are `/tmp/retold-phase1-tests.log`,
`/tmp/retold-phase1-clippy.log`, `/tmp/retold-phase1-acceptance-clippy.log` and
`/tmp/retold-phase1-docs.log`. They are local ephemeral evidence, not CI or production proof.
No production model/media/export or paid benchmark was run. No commit or push was performed.
