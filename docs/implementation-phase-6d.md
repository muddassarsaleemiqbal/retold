# Phase 6D: targeted `account limit` changes

Date: 2026-10-19. Status: implemented; local verification recorded below.

Until now the only way to change caps was `account set-limits FILE`, which replaces every
limit at once. `account limit` changes one scope and window, and shows what is already used
against the new limit. It is the cap configuration half of the plan's Phase 6, alongside the
[Phase 6C](implementation-phase-6c.md) spend reports.

## Try it

```sh
retold-next account limit account day --usd 5 --requests 200
retold-next account limit account month --usd 40 --requests 2000 --dry-run
retold-next account limit provider:openrouter:OPENROUTER_API_KEY lifetime --usd 20 --requests 1000
retold-next account limit video:VIDEO_ID lifetime --usd 2.50
retold-next account limit series:SERIES_ID lifetime --remove
```

## Behavior

- **Scope.** The scope is one of:
  - `account`;
  - `workspace:ID`, `video:ID` or `series:ID`;
  - `provider:ID`. A provider account ID may contain further colons, so
    `provider:openrouter:KEY` names `openrouter:KEY`.

  IDs must be bounded identifiers, never paths. The window is `day`, `month` or `lifetime`.
- **Amounts.**
  - `--usd` sets the money ceiling and `--requests` sets the request ceiling.
  - `--resource UNIT=AMOUNT` (repeatable) sets any other accounted dimension, such as
    `local_millis=600000`. Each unit may be set once.
  - Amounts merge over an existing limit, so `--usd 7.50` keeps its request ceiling.
  - A new limit must set both `--usd` and `--requests`. Every paid quote carries both, and a
    missing unit would quietly block all work in the scope.
- **Removal.** `--remove` deletes the limit and takes no amounts. Removing a limit that does not
  exist is an input error. Removing a required limit (account day/month, or a lifetime limit on
  a workspace, video or provider account) fails closed: paid work that needs it is blocked.
- **Dry run.** `--dry-run` computes the same result on a read-only copy and writes nothing.
- **History is never touched.** The change runs in the same transaction as reservations, so
  it cannot race one, and it is audited as `limit_changed`. Recorded charges, holds and
  receipts are unchanged. Lowering a limit below what is already used blocks new paid work in
  that scope until the limit rises or, for day and month limits, the period ends.
- **Result.** JSON returns `data.limit_changed` with:
  - `scope`, `window`, `before`, `after` and `dry_run`;
  - `headroom`: for each limited unit, the limit, what admission counts now and what is left,
    computed exactly as in `spend`;
  - `exhausted`: the units with nothing left.

  Plain output shows the old and new amounts with money first, current use and what is left,
  a warning for exhausted or removed limits, and whether anything was written.
- **Errors.** Every invalid input is `RG-INPUT-REQUIRED` (exit 2) and writes nothing: an unknown
  scope or window, a path-like ID, an unknown unit, a duplicate unit, amounts given with
  `--remove`, or a missing amount. A missing account is `RG-ACCOUNT-REQUIRED` (exit 3).

`account set-limits FILE` still replaces the whole set. No error codes, account fields or
storage formats were added.

## Deliberate limits

- `video edit budget` (the per-video manifest budget) is a content edit for Phase 7. A
  `video:ID` account limit is the accounting ceiling and is independent of it.
- The TUI cap and spend views wait for the Phase 4 TUI.

## Verification

- Account (`retold-app/tests/spend.rs`, controlled clock):
  - lowering the day limit below today's use reports money as exhausted and keeps the request
    ceiling. The next reservation is refused, and recorded charges and holds are unchanged.
    Raising the limit again admits it.
  - A preview reports the change and leaves the database bytes unchanged.
  - A new money-only limit and removing an absent limit are refused.
  - Removing the account month limit blocks new reservations.
  - Scope parsing accepts provider accounts with colons, and refuses paths, unknown kinds and
    empty IDs.
- CLI (`retold-cli/tests/account_limit.rs`, isolated home):
  - set, merge, dry run (shown in `account show` as unchanged), provider-account limits, plain
    output and removal;
  - six invalid forms each exit 2 and leave no limits.
- `./scripts/check.sh` passes in full, including the Clean Code gate at its existing baseline;
  `cargo test --workspace`: 536 tests passed, 0 failed.

No live provider requests, production account changes or video exports were made.
