# Phase 2E: explicit `video export`

Date: 2026-10-18. Status: implemented; local verification recorded below.

This slice adds the only command that may open the editor, save a project or render. With
[Phase 2D](implementation-phase-2d.md)'s plans and `video continue`, it completes Phase 2's command
deliverables: plan, continue and export, each behind fresh job authority.

## Try it

```sh
retold-next video export VIDEO_ID --edition en --format 9x16 --dry-run
retold-next video export VIDEO_ID --edition en --format both --plan-hash HASH
retold-next video export VIDEO_ID --edition hi --format 16x9 --project-only --yes
```

## Behavior

- **Explicit scope.** `video export <ref> --edition LANG... --format 16x9|9x16|1x1|both...
  [--project-only] [--spend USD] [--max-requests N] (--plan-hash HASH | --yes | --dry-run)`.
  - At least one edition and one format are required. `both` means 16x9 and 9x16.
  - `--until` is refused: export never generates upstream work.
  - `--dry-run` returns the export plan without writing.
- **Export plans.** These are the same hashed envelopes as continue plans, with three differences:
  - The authority's `export` scope (editions, formats, project-only) is part of the hash.
  - The stop is `project` or `export`, and the declared stages are `deliver`, `edit` and `metrics`.
  - The list of deliveries saved at ready is pinned as an input version.

  The `next` command repeats the exact scope with the plan hash. Continue plans carry
  `export: null` and never receive the export capability.
- **Readiness.** A plan is blocked with `RG-NOT-READY` (exit 3) unless every requested
  edition/format pair is among the deliveries saved at ready. Saved means a committed `timeline`
  step plus `deliverables/manifest.json`. The blocker lists the deliveries that do exist.
- **Admission.** The job's action is `export` with exactly the named editions and formats, and its
  capabilities add `export`. The account rejects export authority without editions or formats.
  Workspace, route, spend and ledger handling are identical to continue.
- **Execution** (`retold_engine::session::export`), under the run lock:
  - It revalidates the planned manifest and state.
  - It reads the committed ready timelines through the store's integrity-checked cache. It keeps
    only the manifest deliveries the request names, and refuses if any requested edition or format
    is missing.
  - It passes the job's permission in memory. The saved manifest's `video_approved` flag and old
    approvals grant nothing, and the file is not rewritten.
  - It then runs the engine's existing durable editor path (`export_prepared`): a project only, or
    a render plus thumbnails and packaging. The run's saved state records whether it finished.
  - Nothing upstream is regenerated.
- **Results.** On success the result data is `exported`, with the job ID, plan hash, scope and job
  usage. Failures use the same codes as continue, plus `RG-NOT-READY`.

`RG-NOT-READY` is an additive code in the published v1 schema. The legacy `retold` binary is
unchanged; it only gained the new `project_only` workflow option at its default.

## Deliberate limits

- Export presets, containers/codecs and `video package` are Phase 8 work; formats are aspect
  names only.
- The editor is the installed FilmCraft. The session request's `editor` override exists for
  embedders and tests; there is no CLI flag.
- An export attempt that stops as not ready may still have refreshed the saved state's record of
  the timeline step, so plan again before retrying.
- No live export was performed. FilmCraft rendering was exercised only up to the editor launch,
  using a nonexistent binary.

## Verification

- Engine (`tests/session.rs`):
  - a fake workflow run to `ready` exposes its prepared deliveries;
  - an unknown edition or format is refused as not ready;
  - a valid selection reaches the FilmCraft launch, which fails on a nonexistent binary;
  - an unfinished video has no prepared deliveries.
- CLI (`tests/continuation.rs`) runs against a real video in an isolated installation:
  - export without a scope, or with `--until`, is an input error;
  - `--dry-run` returns an export-scoped plan whose hash differs from the continue plan, with
    `RG-NOT-READY` and the exact export command;
  - `--yes` on an unready video is refused and admits no job.
- The protocol test for unavailable commands now uses `providers`, since `video export` exists.

- `cargo test --workspace --all-targets`: 454 tests passed, 0 failed.
- Workspace strict Clippy, acceptance-feature Clippy for engine/FilmCraft (compile only), the
  Clean Code gate at its existing baseline, formatting, docs with warnings denied, architecture
  layers, unused dependencies and whitespace checks pass.

These are local results. No paid provider calls, production account changes or video exports were
made.
