# Phase 3J: explicit `providers test`

Date: 2026-10-19. Status: implemented; local verification recorded below.

Every other provider command is offline. This slice adds the one command that actually sends a
request to check a provider: one tiny structured text probe per provider. It uses the same
authority rules as [`video continue`](implementation-phase-2d.md): a hashed plan, explicit
confirmation, a fresh account job and a default spend of zero. Results feed the durable health
ledger from [Phase 3H](implementation-phase-3h.md) and [3I](implementation-phase-3i.md).

## Try it

```sh
retold-next providers test --root CONFIG_DIR                      # plan only; sends nothing
retold-next providers test codex --root CONFIG_DIR --plan-hash HASH
retold-next providers test openrouter --root CONFIG_DIR --spend 0.10 --yes
```

## Behavior

- **Plan first.** Without `--plan-hash` or `--yes`, the command returns `data.test_plan` and
  sends nothing. For each selected provider (one ID, or all) the plan shows:
  - its kind, billing (`subscription`, `local` or `per_use`) and supported tiers;
  - the decision: test the first text route that names it (tier and selector), or a skip
    reason. The reasons are:
    - `no_text_capability`: only text is tested;
    - `unrouted`: no text route names it, so there is no selector to test;
    - `needs_spend`: it is per-use and `--spend` is zero.
  - the USD and request ceilings, the per-use routes and their provider accounts, the blockers,
    and the exact command to run the plan.

  The plan hash pins the probes, the ceilings, the routes, the provider settings digest and the
  settings folder. Planning writes no files.
- **Confirmation.** A changed `--plan-hash` returns `RG-PLAN-CHANGED`. `--yes` accepts the plan
  computed now and never raises an allowance. Blockers stop the command before admission:
  - a missing account;
  - for paid probes, any missing positive `usd_micros` and `requests` limit among account day,
    account month and each provider account's lifetime.

  A plan in which every provider is skipped admits nothing and sends nothing.
- **Admission.** This uses the new `provider_test` job action:
  - The settings folder is registered as a workspace (`settings-<hash>`).
  - The job is recorded under the subject `providers:test`, not a video, and names exactly the
    providers it tests.
  - Its only capability is text, with one operation node. The account refuses export authority
    on any non-export job.
  - Paid probe attempts need account day, account month and provider-account lifetime ceilings.
    No workspace or video lifetime ceiling applies.
- **Probes** run in order. The first Ctrl-C cancels; a second also ends provider processes.
  - A recorded cooldown that has not ended skips the probe (`cooldown`). Unreadable health
    storage refuses it.
  - Free providers receive the request directly.
  - A per-use provider is quoted, reserved and dispatched through the account gate before the
    request (attempt `JOB:PROVIDER`). Its reply is then settled with artifact evidence.
    Confirmed unbilled failures settle at zero. Any other failure keeps its hold (`uncertain`)
    until `account reconcile`.
  - A reply passes only if it contains `"ok": true`.
  - Success marks the provider `ok` in the health ledger. Failures record `limited`,
    `logged_out`, `unavailable` or `failing` there.
- **Results.** On success the data is `tested`: job ID, plan hash, per-provider status, a detail
  line of at most 200 characters, the account attempt ID for paid probes, and the job's usage.
  - If accounting refused a probe or a paid charge is unresolved, the command returns
    `RG-ACCOUNT-REQUIRED` (exit 3).
  - Any other failed probe returns `RG-EXECUTION-FAILED` (exit 4).
  - Both errors carry the results in their context. Plain output prints those per-provider
    lines before the error.

No error codes were added. The `provider_test` action is additive in the account's stored job
records.

## Deliberate limits

- Only text is probed. Image, speech and transcription tests would produce billable media and
  remain pending; their providers are reported as `no_text_capability`.
- Subscription and local probes are not reserved in the account, as with Phase 2C runs. They
  still run only under an admitted job.
- `--usage-policy` bounds and cross-alias canonical provider identity are not implemented.
- `providers login` and portable workspace backup/restore remain pending.

## Verification

- Engine (`tests/provider_test.rs`, offline fakes):
  - plan decisions: routed, unrouted, needs spend, no text capability and unknown provider. Planning writes nothing.
  - Free and paid probes record health. A paid probe reserves, dispatches and settles in that
    order.
  - A refused reservation never reaches the provider.
  - Login failures are classified. An ambiguous paid timeout is neither settled nor reconciled.
  - Recorded cooldowns skip without sending.
- Account (`retold-app/tests/gate.rs`):
  - a provider-test job carrying export capability is refused;
  - its paid attempt is refused until a provider lifetime limit exists, then reserves, dispatches
    and settles.
- CLI (`tests/provider_test.rs`) runs with an isolated home and account, using a fake
  llama-completion script as a local provider:
  - plans need an account and refuse a stale hash, admitting nothing and writing no files;
  - a confirmed free test admits one job, reports `ok`, and `providers health` then shows `ok`;
  - paid tests need `--spend` and list the provider-scoped missing limits;
  - an unknown provider is an input error;
  - a failing provider exits 4 with its results in JSON and plain output.
- `./scripts/check.sh` passes in full:
  - formatting;
  - workspace strict Clippy, plus acceptance-feature Clippy;
  - the Clean Code gate at its existing baseline;
  - architecture layers, unused dependencies, licences/advisories and docs with warnings denied;
  - `cargo test --workspace`: 523 tests passed, 0 failed.

No live provider requests, production account changes or video exports were made.
