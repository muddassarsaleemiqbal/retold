# Phase 2 Qualification: Plan/Reuse Qualification & Every-Call Resource Accounting

## Overview

This qualification milestone completes Phase 2's execution, planning, and accounting contracts:
1. **Artifact-Level Plan/Reuse Qualification**: Detects and verifies cached stage artifacts during planning. When all planned stages through `--until` are verified in the cache, `video plan` qualifies them as reusable, reports `0 steps · $0`, and sets `dispatches: 0` with zero estimated cost/requests.
2. **Every-Call Resource Accounting & Logical Key Binding**: Distinct logical operation keys (`operation_key`) are bound across attempts. Redispatching an already-settled operation with a valid artifact receipt is refused (`Error::OperationExists` / `GateError::Uncertain`). Concurrent in-flight attempts for the same logical operation are rejected by single-flight protection, while unbilled failures reconciled at zero permit clean retries without leaking holds.

---

## Key Changes

- **`crates/retold-app/src/video.rs`**:
  - Implemented `qualify_plan` and `build_stage_plan` to inspect recorded step envelopes against planned stages.
  - Set `decision: "reused_from_cache"` for stages where all recorded steps are `EnvelopeVerified` and 0 are unresolved.
  - Set `all_cached: true` and `estimates: { usd_micros: 0, requests: 0, duration_ms: 0, source: "qualified_cache" }` when all requested stages are satisfied from cache.
- **`crates/retold-cli/src/human.rs`**:
  - Formatted plan summaries for cached plans: `Plan <hash>: <action> · 0 steps · $0` (matching the acceptance criteria in `plan-cli.md` §18.3).
- **`crates/retold-app/src/execution.rs`**:
  - Bound `call.logical` to `operation_key: digest("operation", workspace_id, video_id, logical)`.
  - Added multi-resource accounting support for attempt tracking.
- **`crates/retold-app/src/account/lifecycle.rs`**:
  - Refined `check_single_flight` to distinguish between active attempts (`Reserved` or `Dispatched`), settled successful artifacts (blocked from duplicate dispatch), and unbilled zero-cost failures (eligible for retry).
- **Test Suites**:
  - Added `cached_stages_qualify_plan_reuse_and_report_zero_cost` in `crates/retold-cli/tests/continuation.rs`.
  - Added `settled_operation_prevents_duplicate_dispatch_across_different_jobs` in `crates/retold-app/tests/gate.rs`.

---

## Verification & Quality Gates

- `cargo test -p retold-app -p retold-cli`: All 68 integration and unit tests passed.
- `cargo fmt --all --check`: Clean formatting across workspace.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings under pedantic + nursery.
- `clean_code` test (`clean-code.toml` strictest preset): 0 new findings against ratcheting baseline.
- `python3 scripts/check-architecture.py`: Workspace layer boundaries verified.
- Zero live network provider calls, login modifications, or video exports.
