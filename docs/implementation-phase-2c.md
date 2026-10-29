# Phase 2C: account-gated paid dispatch

Date: 2026-10-18. Status: implemented; local verification recorded below.

This slice connects the engine's per-use provider calls to the [Phase 2B account](implementation-phase-2b.md).
It does **not** complete Phase 2's execution gate. No command admits a job or runs work yet:
`video continue` and `video export` were unavailable in `retold-next` at this point. The legacy `retold`
binary still runs ungated, with its per-run budget only.

## What changed

- `retold_engine::gate::PaidGate` is the engine's external accounting boundary. A per-run
  `Ledger` built with `with_gate` sends every per-use call through it. Paid text fallback,
  images, speech and transcription all go through `Ledger::begin_paid`, called right before the
  provider request. It takes the next attempt slot, holds the quote in the per-run ledger,
  reserves in the gate, then makes the gate's one-time dispatch transition.
  For media, the step begins and its progress event is written before the reservation. Nothing
  local can then fail between dispatch and the network call.
- A gated ledger refuses the ungated `reserve`/`settle` calls, so no call path can bypass the
  account. Settlement uses `settle_with_evidence` and forwards the committed artifact hash. That is
  the image/audio/transcript content hash, or the hash of the paid text response value.
  The gate settles before the local file is updated. If a crash comes between the two, the
  existing saved-result recovery re-settles idempotently.
- Outcomes:
  - Gate refuses the reservation for a ceiling: the per-run hold is removed, and the image path
    tries another allowed route, as it does for the per-run budget.
  - Gate reports an unresolved prior attempt: the local hold is kept.
  - Dispatch is refused (for example expired authority or a changed plan): the account
    reservation is withdrawn, and the local slot is retired as an exact $0. A withdrawn account
    attempt is never reused.
  - Ambiguous provider failure: nothing is released.
  - Provider-confirmed unbilled failure: reconciled through the gate. The account settles it at
    $0 and one request.
  - In gated mode, a $0 reconciliation also retires the slot instead of making it reusable, so a
    retry always gets a fresh account attempt.
- `retold_app::execution::AccountGate` implements the gate on the shared account:
  - **Attempt identity.** Each per-run attempt slot maps to one deterministic account attempt
    (`hash(workspace, video, slot)`), which is also its single-flight key. A lost per-run ledger
    replays into the same attempt: an already-dispatched attempt reports an unresolved outcome
    rather than dispatching again. A new job cannot claim another job's attempt ID.
    `for_reconciliation` opens a video's attempts for billing resolution and cannot dispatch.
  - **Reconciliation by state.** Reserved attempts are released only at $0. Dispatched attempts
    settle with a no-artifact marker. Settled attempts may fill an unknown amount; a known amount
    never changes.
  - **Quotes.** Each quote is one request plus the provider's worst-case USD micros. Text tokens,
    audio and local time are not yet quoted dimensions.
- Account reservations gain an optional `node`, the declared operation-tree node whose job
  ceiling the attempt draws on. Engine request keys depend on upstream outputs, so a job cannot
  list them before admission. Instead it declares per-video stage/capability nodes
  (`execution::operation_node`) with finite ceilings; the cost contract §4 calls these the
  operation tree. Work outside a declared node is refused. Without a node, the operation key
  remains the ceiling key, as in Phase 2B.

## Deliberate limits

- Only per-use dispatch is gated. Subscription and local work still bypass the account; the
  contract (§21) extends reservations to them later.
- Single flight is per physical attempt slot. Logical-operation lineage across retries is still
  held by the per-run ledger's slot sequence.
- Nodes are per stage and capability. A finer tree (per item, per repair) needs the executable
  planner.
- Paid text recovery after a crash between cache commit and settlement still leaves a hold:
  text cache entries do not record their attempt ID.
- `allow_paid` is still the engine's opt-in for per-use routes. Job authority adds a ceiling
  but does not set that flag.

## Next slice

[Phase 2D](implementation-phase-2d.md) adds executable plans and `video continue`.

## Verification

- Engine (`tests/paid_gate.rs`, offline fakes):
  - an image reserves and dispatches before the provider is called, then settles with the
    asset's content hash and exact cost;
  - paid text settles with its response hash, and an unknown cost keeps the full quote held;
  - a refused reservation never reaches the provider and leaves no local hold;
  - a refused dispatch withdraws and later uses `-retry-1`;
  - an ambiguous speech failure keeps its hold;
  - a gated ledger rejects direct reserve/settle.
- Application (`tests/gate.rs`, real account, no provider I/O):
  - two videos with separate per-run budgets share one account day ceiling;
  - an undeclared or exhausted node is refused;
  - a lost per-run ledger cannot redispatch, under the same job or a new one;
  - an unknown cost stays held until reconciliation through a reconciliation-only gate;
  - an unbilled failure settles at $0 and frees only a new slot;
  - expired authority dispatches nothing and holds nothing.
- `cargo test --workspace --all-targets`: 444 tests passed, 0 failed.
- Workspace clippy with warnings denied, acceptance-feature clippy for engine/FilmCraft
  (compile only), docs with warnings denied, unused-dependency and whitespace checks,
  the Clean Code gate with an unchanged baseline,
  formatting and the architecture layer check pass. `retold-app` still depends only on
  `retold-engine`; the gate re-exports its money type.

These are local results, not CI, deployment or live-provider evidence. No paid provider calls,
production account changes or video exports were made.
