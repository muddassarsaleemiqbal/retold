# Phase 6C: read-only `spend` reports

Date: 2026-10-19. Status: implemented; local verification recorded below.

`retold-next spend` breaks down what the installation account has recorded, as the plan's §8
requires: by provider, video, series and day. It reads the same canonical account that every
paid dispatch reserves against ([Phase 2B](implementation-phase-2b.md)), so provider tests and
video runs appear in one place. It never reconciles, changes limits or grants authority; see
[Phase 6A](implementation-phase-6a.md) for attempt inspection and
[Phase 6B](implementation-phase-6b.md) for reconciliation.

## Try it

```sh
retold-next spend                      # all time, by provider account
retold-next spend --today --by video   # today in the account timezone
retold-next spend --month --by day
retold-next --json spend --by series
```

## Behavior

- **Windows.** The default is all time. `--today` and `--month` select the current day or month
  in the account's IANA timezone. They conflict with each other.
  - The current period is computed from the later of the clock and the last recorded account
    change, so a clock moving backwards cannot reopen an earlier period.
- **Attribution.** A charge belongs to the period its reservation started in, matching
  admission (§19.2). A request reserved at 23:59 and settled after midnight stays on the earlier
  day.
- **Grouping.** `--by provider` (default) groups by canonical provider account, so route
  aliases share a row. The other groupings are `video`, `series` and `day` (the reservation
  day). Attempts outside any series form one row with no key.
- **Each row and the total** show:
  - the attempt count;
  - the attempts still unresolved (reserved, dispatched, or settled with an unknown
    observation);
  - charged, held and unknown-observation totals per resource.

  Money and requests are always present; other dimensions appear only when used. A held amount
  is the attempt's quote, never a guess of the actual cost.
- **Carried holds.** In a day or month report, `carried` totals the unresolved attempts reserved
  in earlier periods. Their holds still reduce this period's headroom until `account reconcile`
  settles them. Reconciliation then charges the attempt's own earlier period, exactly once.
- **Headroom.** `ceiling` shows, for each limited resource, the limit, the usage admission
  would count now (charges in the window plus every outstanding hold) and what is left.
  - The account ceiling uses the account limit for the report's window.
  - Provider, video and series rows show their own limit for that window when one is set.
    Days have no scope of their own.
  - Plain output says when no day or month money limit exists, since paid work is then blocked.
- **Output.**
  - JSON: `data.spend` with `timezone`, `window`, `period`, `by`, `rows`, `total`, `carried`
    and `ceiling`.
  - Plain: one line per row with money rounded up to whole cents; remaining headroom is
    rounded down. Then the total, the ceiling, carried holds, how to inspect and settle
    unresolved attempts, and "Report only; nothing was changed."
- **Errors.** A missing account returns `RG-ACCOUNT-REQUIRED` (exit 3); unreadable accounting
  returns `RG-ACCOUNTING-UNAVAILABLE`. Invalid flags are input errors (exit 2).

No error codes, account fields or storage formats were added.

## Deliberate limits

- Only the account is reported. Per-run ledger stage costs remain in `video costs`.
- Rows show stored identifiers (provider account, video ID, series ID), not library titles.
- Cap configuration is still the whole-file `account set-limits FILE`; targeted cap edits and
  the TUI spend views remain pending.

## Verification

- Account (`retold-app/tests/spend.rs`), with a controlled clock in Asia/Karachi (UTC+5)
  across a day and a month boundary:
  - today's and this month's reports exclude the previous period's charges but carry its
    unresolved hold into `carried` and the ceiling's `used`;
  - lifetime reports group by provider (with the provider-account ceiling), video, series
    (including the no-series row) and day;
  - reconciling a carried hold clears it and charges its own earlier day once;
  - a backwards clock cannot reopen January, a later month still carries the hold, and the
    account database bytes are unchanged by reports.
- CLI (`retold-cli/tests/spend.rs`), isolated home, attempts seeded through the account API:
  - a missing account exits 3;
  - conflicting windows and unknown groupings exit 2;
  - the empty report names the missing day limit;
  - JSON and plain reports show the charge, the hold, the unresolved attempt and the
    headroom, without changing the database.
- `./scripts/check.sh` passes in full: formatting, strict Clippy, the Clean Code gate at its
  existing baseline, architecture layers, unused dependencies, licences/advisories, docs, and
  `cargo test --workspace` with 530 tests passed and 0 failed.

No live provider requests, production account changes or video exports were made.
