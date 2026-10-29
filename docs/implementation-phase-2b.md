# Phase 2B, account ledger core

Date: 2026-10-18. Status: implemented; local verification recorded below.

This slice adds the shared accounting and fresh authority primitives needed before executable
planning. It does **not** complete Phase 2's execution gate. `video plan` remains a read-only
inspection, and continue/export remain unavailable in `retold-next`.

## Available commands

```
retold-next account init --timezone Asia/Karachi --dry-run
retold-next account init --timezone Asia/Karachi
retold-next --json account show
retold-next account set-limits limits.json
```

Initialization is explicit, and all allowances initially equal zero. Preview validates the IANA
timezone and reports the selected directory without creating it. Show opens existing accounting
read-only. Set-limits atomically replaces ceilings without removing jobs, holds, charges or audit
history. Its JSON array is bounded to one MiB. Missing resource dimensions equal zero, including
on an otherwise present limit. Duplicate scope/window entries are rejected.

The canonical installation directory uses OS `ProjectDirs` for `retold-account`, independently of
video library, workspace and Retold configuration overrides. There is no CLI ledger-path option.
The low-level `Account` API accepts explicit paths and clocks for embedders and isolated tests;
production callers must use the canonical directory. This is installation-local coordination, not
protection against an OS account administrator copying or changing authoritative files.

Example limits, using integer units rather than floats (one USD is 1,000,000 `usd_micros`):

```json
[
  {"scope":{"kind":"account"},"window":"day","resources":{"requests":10,"usd_micros":1000000}},
  {"scope":{"kind":"account"},"window":"month","resources":{"requests":100,"usd_micros":10000000}},
  {"scope":{"kind":"workspace","id":"workspace"},"window":"lifetime","resources":{"requests":100,"usd_micros":10000000}},
  {"scope":{"kind":"video","id":"video"},"window":"lifetime","resources":{"requests":10,"usd_micros":1000000}}
]
```

Workspace and route registration, job admission, and attempt lifecycle are application APIs in
this slice. They do not yet have execution commands. A configured account alone cannot dispatch.

## Accounting behavior

- SQLite `BEGIN IMMEDIATE` serializes admission checks and durable reservations across processes.
  Writes use synchronous FULL. No provider I/O happens inside a transaction. Independent subprocess
  tests compete for a single request allowance and verify exactly one reservation succeeds.
- A workspace ID binds a canonical existing root. Conflicting copies and route rebinding are refused.
  Route aliases map to canonical provider-account references, so configured provider caps are shared.
- Each immutable job pins a plan hash, input-version hashes, workspace/video/optional series,
  routes, capabilities, action, optional deadline, total resource limits and operation limits.
  Saved run flags grant no authority. Continue cannot carry export capability. Export authority
  must name editions and formats. External verification of current bytes is still executor work.
- Integer dimensions cover USD micros, requests, input/output tokens, audio milliseconds,
  local milliseconds and vision pixels. Quotes require nonzero requests and an explicit USD amount,
  including zero. Local quotes additionally require a nonzero time bound. Provider adapters must
  supply complete conservative quotes; this layer does not invent price or native-quota conversions.
- Every quoted dimension must fit job and operation ceilings, account day/month ceilings,
  workspace/video lifetime ceilings, optional series lifetime ceilings and all other applicable
  configured limits. Unknown amounts retain their quoted holds. Checked arithmetic fails closed.
- Logical operation identity is single-flight within a workspace across jobs/routes. Exact attempt
  replay returns the existing attempt without creating a new hold or dispatch permit. A different
  request cannot reuse an attempt ID. Settled operations also remain protected against redispatch.
- Only the first reserved-to-dispatched transaction returns a non-cloneable dispatch permit.
  It rechecks pinned authority, workspace binding, deadline and the supplied plan hash. A crash after
  this transition retains the hold. Reserved attempts can explicitly become definitely-not-dispatched;
  dispatched attempts cannot release holds through cancellation. This is a durable local boundary,
  not a claim of exactly-once external effects or verified provider idempotency.
- Settlement requires artifact and receipt hashes. Known observations become charges, even if actual
  consumption exceeds the quote; later admission respects those charges. Missing/null observations
  remain unknown and held. Reconciliation may fill unknown observations but cannot change known
  amounts or the artifact identity. Receipt versions and transition audit history are retained.
  Hash shape validation does not itself verify artifact bytes or certify quality acceptance.
- Periods use the account's pinned IANA timezone and the reservation-start day/month. Unresolved
  holds carry into new periods. Known charges belong to the original period and always count toward
  lifetime scopes. The persisted maximum clock, including rejected admissions, prevents backward time from replenishing allowance
  or reviving expired authority.
  Timezone mutation is intentionally unavailable in this version.
- Missing, malformed, unsupported or inaccessible accounting blocks access. Initialization refuses
  any existing account directory, including one with lost history. It never silently repairs,
  restores or replaces accounting. Interrupted initialization requires explicit future repair/restore.

## Storage and integration limits

Version 1 stores one bounded JSON account state in a SQLite row. This keeps all checks and writes
atomic without a migration framework. The state is capped at 64 MiB and 100,000 attempts; exhaustion
blocks writes without discarding history. This design is for the first installation-local core,
not a high-throughput service. Normalized storage, maintenance barriers, backup/restore/rebinding,
retention and timezone changes require separately reviewed transitions.

[Phase 2C](implementation-phase-2c.md) now gates per-use engine dispatch through this ledger.
The remaining work connects executable planning and real adapters to it:
current-input/price/quote validation, workspace ownership and maintenance coordination, artifact
spooling and settlement recovery, provider reconciliation, cancellation/retry semantics, and fresh
edition/format-specific export gates. Existing engine execution paths do not yet use this ledger.
No paid provider evaluation, production account mutation or video export was performed for this slice.

## Verification

Targeted tests cover cross-process contention, replay, dispatch compare-and-set, restart holds,
unknown observations, receipt reconciliation/history, overages, default-zero limits, job/operation
and canonical-provider limits, workspace conflicts, route rebinding, expiry, zero-dollar requests,
period rollover and backward clocks. CLI tests cover nonmutating account preview and invalid inputs.
The shared output schema includes account diagnostics; protocol version remains 1.

Local results: the complete workspace/all-targets run passed 431 tests. After the final clock
checkpoint and limit-file regression changes, all 18 application tests and the CLI tests passed;
the synchronized subprocess reservation test also passed. Workspace strict Clippy, acceptance-feature
Clippy for engine/FilmCraft, the unchanged Clean Code baseline, formatting, architecture, dependency
usage, documentation with warnings denied, and whitespace checks passed. These are local results,
not CI, deployment or live-provider evidence.

Implementation references: [rusqlite Connection](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Connection.html)
and [SQLite transactions](https://www.sqlite.org/lang_transaction.html).
