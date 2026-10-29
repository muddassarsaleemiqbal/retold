# Native frontend contract

The desktop application depends on `retold-app`. Domain operations, accounting, manifests,
expected-version checks, invalidation, maintenance barriers and execution admission belong
below presentation. Do not reconstruct them in a GPUI callback or reuse placeholder review
history as if it changed actual content.

## State and effects

1. Render, input and navigation callbacks perform no filesystem walks, SQLite operations,
   model requests, decode, subprocess waits or synchronous socket requests. Use bounded
   background work and typed application requests. Kit render builders must be idempotent;
   create retained input entities before building a dialog.
2. Normal requests are serialized. Emergency Stop all has a separate lane and invalidates
   older planning/control observations. Preserve explicit unknown/unconfirmed outcomes.
3. Revalidate target identity and scope on replies. Do not let a delayed result replace a
   newer selection, workspace, control receipt or appearance choice.
4. Successful persistence is established by the shared operation's receipt/current data.
   A disabled button, running future, socket send or process existence is not completion.
5. Retain unsaved text on errors and conflicts. Disable editing while saving a captured
   draft so successful completion cannot discard text typed after submission.
6. Use atomic publication and existing writer/maintenance locks. GUI preference writes are
   serialized, coalesced and bounded; failed changes remain available for explicit retry.

## Authority and recovery

- Saving a draft, searching, opening a workspace, loading media or restoring window settings
  grants no provider, job, spending or export authority. Run inputs start at zero on reopen.
- A reviewed run owns the exact backend plan hash, options and stable idempotency key.
  Confirm again after any meaningful change. Export has its own explicit review.
- Starting a worker is an explicit action because accepted pending jobs may run. Opening or
  refreshing a workspace never starts it. Quitting Studio leaves accepted work independent.
- Emergency stopping targets the selected workspace's private queue and records its durable
  hold/receipt. Do not equate cancellation acceptance with terminal completion.
- A current script edit requires its expected document version and a verified baseline.
  Approval pins its current content hash. Changed content revokes approval and invalidates
  both visual and speech descendants; research/writer cache remains reusable.
- Read-only queries never create or repair their subject. Invalid, incomplete and stale
  observations must be visible. Preserve the last successful page when the next page fails.

## Presentation persistence

`desktop-preferences.json` stores appearance, text zoom, window dimensions and at most eight
recent absolute workspace paths. It contains no worker restart instruction, run allowance,
credentials, queue admission or export consent. Unknown fields and unsupported values fail
without replacing the original file. Recent-workspace restoration is read-only.

## Verification

Native behavior tests drive pointer/keyboard events against real GPUI rendering. Cover
normal, empty, partial, unavailable, blocked, conflict, saving and stopped states; compare
light/dark/system/high-contrast and compact/large-text layouts. Use fixture paths and zero
authority. Provider tests and media export require separately authorized isolated fixtures.

Root `scripts/check.sh` remains authoritative for shared code; the standalone desktop has
its own `check.sh` and pinned dependency-policy guard. Run timing checks on an idle host,
separate from renderer builds or media stress. Preserve thresholds. Record failed/partial
evidence and repeat only the affected checks after a correction.
