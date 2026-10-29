# Phase 6B: explicit billing reconciliation without dispatch authority

`retold-next account reconcile --attempt ATTEMPT_ID --cost <USD>` and
`retold-next video costs VIDEO_REF [--request REQUEST_ID --cost <USD>]` provide
safe, offline billing reconciliation against the canonical installation account
and run ledgers without granting or implying dispatch authority.

## Lifecycle reconciliation semantics

Reconciliation in `retold-app::account::lifecycle` implements strict safety rules
across attempt lifecycle states:
- **Reserved:** An un-dispatched attempt may only be reconciled at `$0.00`, transitioning
  to `NotDispatched` and releasing all held resources. Reconciling a reserved attempt at
  a positive cost is rejected with `Error::Transition`.
- **Dispatched:** Settles the in-flight attempt, recording an explicit receipt with the
  exact USD cost observed in micros.
- **Settled:** If an attempt previously settled with an unknown cost (`None` in receipt
  observations), reconciliation immutably records the actual cost. If the attempt already
  recorded a known cost, any conflicting cost is rejected with `Error::Transition`
  (preserving receipt immutability).

## CLI and Video costs

- `retold-next video costs VIDEO_REF` inspects the run ledger and account limits for the
  referenced video. It reports total spend against video budget, itemizes completed stage
  costs, and flags unresolved charges held at ceiling with instructions for settlement.
- `retold-next video costs VIDEO_REF --request REQUEST_ID --cost <USD>` deterministically
  derives the attempt ID for the given request and video, reconciling the charge in the
  account.
- Both JSON and human outputs explicitly assert `dispatch_authorized: false` and render
  "Inspection only. No dispatch authority granted" or "No dispatch authority granted."

## Verification

- `crates/retold-app/tests/account.rs`: `reconcile_attempt_across_lifecycle_states` tests
  `Reserved` ($0 vs positive cost), `Dispatched`, and `Settled` (unknown vs immutable known).
- `crates/retold-cli/tests/account_reconcile.rs`: Verifies JSON and human command outputs,
  argument validation, and explicit absence of dispatch authority.
- `crates/retold-cli/tests/video_costs.rs`: Tests clean and unresolved video ledger charges,
  reconciliation via `--request` and `--cost`, and parameter validation.
- All quality gates pass cleanly: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python3 scripts/check-architecture.py`, Clean Code ratcheting gate (`cargo test -p retold --test clean_code`),
  and the full test suite (`./scripts/check.sh`).
