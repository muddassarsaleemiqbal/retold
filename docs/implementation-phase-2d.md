# Phase 2D: executable plans and `video continue`

Date: 2026-10-18. Status: implemented; local verification recorded below.

This slice turns `video plan` into an executable, hashed plan and makes `video continue` run a
saved video up to `ready`. It builds on the [Phase 2B account](implementation-phase-2b.md) and
[Phase 2C gated dispatch](implementation-phase-2c.md). [Phase 2E](implementation-phase-2e.md)
adds `video export`.

## Try it

```sh
retold-next account init --timezone UTC
retold-next video plan VIDEO_ID                        # free routes only, stops at ready
retold-next video plan VIDEO_ID --until script --spend 2.50 --max-requests 50
retold-next video continue VIDEO_ID --plan-hash HASH    # or --yes to accept the plan computed now
```

## Executable plans

`video plan <ref> [--until STAGE] [--spend USD] [--max-requests N]` reads the library, the
installation's provider settings and the account. It writes nothing and dispatches nothing.

- **Hashed authority.** The plan hash pins:
  - the video and the library's workspace ID;
  - the stop;
  - the export scope, which is `null` for continue;
  - the stages that stop may run, prerequisites included;
  - the USD and paid-request ceilings and the per-video cap;
  - each per-use route and the provider account it draws on;
  - input versions: the run manifest hash, saved run state hash, snapshot ID and provider
    settings digest.

  Any change to these produces a different hash.
- **Allowance defaults to zero.** The default `--spend` is 0, which considers no per-use
  route. A positive spend defaults to at most 1,000 paid requests unless `--max-requests` is set.
- **Stops.** `--until` accepts `created` through `ready`. `project` and `export` are refused:
  only a separate export request may create projects or render.
- **Blockers.** The plan names what would refuse execution:
  - a missing or unreadable account;
  - for paid work, any missing positive limit among account day, account month, workspace
    lifetime and video lifetime, with the exact scopes to add.
- **Estimates stay unknown.** The ceilings bound spending; they are not predictions. The plan also
  returns the stage evidence from Phase 2A and the exact `continue` command.
- **Routes and accounts.** Routes that share an OpenRouter key map to one provider account
  (`openrouter:KEY_ENV`), so aliases cannot multiply a provider cap.

## Continue

`video continue <ref> [--until STAGE] [--spend USD] [--max-requests N] (--plan-hash HASH | --yes)`:

1. Recompiles the plan. A `--plan-hash` that differs returns `RG-PLAN-CHANGED`. With neither a
   hash nor `--yes` it returns `RG-INPUT-REQUIRED`. Plan blockers stop it before anything is
   admitted. `--yes` accepts the current plan and never raises an allowance.
2. Registers the library as a workspace and the per-use routes, then admits a fresh job. The job
   is pinned to the plan hash and input versions, with continue-only capabilities, and has one
   operation node per planned stage and capability. Saved `approvals.json` flags are never read.
3. Runs the engine session (`retold_engine::session`) under the run lock:
   - It rereads the manifest and saved state and refuses with `RG-PLAN-CHANGED` if either moved
     since the plan.
   - It loads the run's own guide snapshot (refusing a modified one) and the channel saved with
     the run.
   - Provider keys come from the environment, then the settings folder's `.env`, without writing
     to the process environment.
   - Every per-use call goes through an `AccountGate` for the job. Per-use routes are considered
     only when the spend is above zero.
   - It never goes past `ready`.
   - The first Ctrl-C cancels gracefully; a second also ends provider processes.
4. Reports one terminal envelope with the job ID, plan hash and the job's charged/held/unknown
   totals:
   - review pause: `RG-REVIEW-REQUIRED` (exit 3);
   - ceiling, authority or unresolved paid attempt: `RG-ACCOUNT-REQUIRED` (exit 3);
   - changed run: `RG-PLAN-CHANGED` (exit 3);
   - failure or cancellation: `RG-EXECUTION-FAILED` (exit 4), with the engine message reduced to
     one line of at most 500 characters.

`RG-REVIEW-REQUIRED` and `RG-PLAN-CHANGED` are additive codes in the published v1 schema.

## Deliberate limits

- Reviews are skipped only when the video was created to skip them (`skip_reviews` in its
  manifest). That is a content setting, not spend authority. Approving reviews from this CLI is
  not implemented yet.
- Subscription and local work still bypass the account (Phase 2C limit). `--usage-policy`
  bounds for them are not implemented.
- The operation tree has one node per stage and capability, and each node is a slice of the whole
  job ceiling. Per-item or per-repair budgets need a dry-run planner. Plans do not claim which
  saved results will be reused.
- If the run changes between admission and the run lock, the admitted job stays recorded and
  unused. It holds nothing.
- Continue runs in the foreground with no live progress output; events still go to the run's
  event log. The studio, detach and `video logs` belong to Phase 5.
- No account command reconciles unresolved paid attempts yet; the reconciliation-only gate from
  Phase 2C is the API for it.

## Verification

- CLI (`tests/continuation.rs`) runs against a real video created from the bundled guide, with an
  isolated home, settings folder, library and account, and no providers configured:
  - plans are stable, pin the ceilings and stop, list missing paid limits, and write nothing;
  - continue refuses an unconfirmed plan, an export stop, a missing account, a stale hash and
    paid work without limits, and admits no job for any of them;
  - a reviewed plan admits exactly one job and completes;
  - replaying the old hash after the run's state moves on is refused;
  - an engine failure reports its job with no attempts;
  - the plain plan shows the cent-rounded ceiling, the hash, blockers and the exact next command.
- Engine (`tests/session.rs`):
  - stop closures;
  - export stops and changed runs are refused before any provider call;
  - the run lock is enforced.
- Existing library CLI tests now isolate `HOME` and the settings folder, so plans never read the
  developer's own account or providers.
- `cargo test --workspace --all-targets`: 451 tests passed, 0 failed.
- Workspace strict Clippy, acceptance-feature Clippy for engine/FilmCraft (compile only), the
  Clean Code gate at its existing baseline, formatting, docs with warnings denied, architecture
  layers, unused dependencies and whitespace checks pass.

No paid provider calls, production account changes or exports were made. Live paid continuation
through OpenRouter has not been exercised; the gate's paid path is covered by Phase 2C's fakes.
