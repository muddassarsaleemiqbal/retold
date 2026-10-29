# Native shell and shared contracts checkpoint

Date: 2026-10-25. This records implemented slices of G1–G4 and G7. It does not mark those
entire phases, the remaining G0 qualifications, or the full G0–G9 plan complete.

## Shared application changes

`retold-app::desktop` supplies typed workspace, installed-guide/question, activity and
reviewed-run projections. The effective configuration matches the engine's creation path,
including legacy libraries that use installation configuration. Opening is read-only;
queue corruption remains a section error rather than a false empty queue. Video snapshots
are explicitly limited to the newest 200 records and retain invalid-manifest diagnostics.

`creation::save_draft` returns a typed genuine engine-draft receipt. The legacy CLI response
adapts the same implementation. Validation, non-overwriting publication and the existing
account maintenance barrier remain shared. `studio::submit_job` similarly returns a typed
job receipt while the CLI keeps its existing payload. Reviewed native submissions retain
their exact plan/options/hash and stable idempotency key; the backend recompiles and confirms.

Native queue/run operations reject a `RETOLD_STATE` override outside the selected workspace's
private queue. Canonical directory aliases are accepted; escaping directory links are rejected.

## Durable workspace stopping

Stop all is a shared operation, not a GUI loop over rows:

1. One immediate SQLite transaction snapshots eligible jobs, stops queued/paused jobs,
   marks running/pausing work stopping, records transition events, stores the request receipt,
   and installs a dispatch/admission hold.
2. The same control ID replays its original receipt. Replaying an old request after enabling
   the queue does not stop later jobs or reinstall an old hold.
3. Inhibition survives restart. A compare-and-clear request ID prevents an older window from
   releasing a newer hold. Clearing it does not restart stopped jobs.
4. Database triggers also reject queued/running admission from older SQL writers while held.
5. The shared offline handler records inhibition before IPC and does not launch a daemon.
   Capability negotiation precedes worker acknowledgment. An unavailable socket remains
   unconfirmed; active jobs do not become stopped merely because a request was sent.
6. The daemon observes persisted cancellation requests on its control loop. A worker's saved
   outcome remains authoritative for completion, including work that finished during stopping.

CLI entry points: `retold studio stop-all --request-id ID` and
`retold studio enable-dispatch --request-id ID`, scoped by the existing workspace/library
selection. These use the same queue hold/receipt implementation as the native shell.

## Packaged worker and socket paths

`retold-studio --worker --library PATH` runs before GPUI initialization, registers graceful
termination signals, uses the existing singleton server and drains active work before exit.
`studio::start_worker` performs bounded readiness checks and reaps its owned child independently
of the GUI's lifetime. Starting is an explicit action because saved pending jobs may run.

Long Unix socket addresses use a deterministic short alias in `/tmp`, in an owner-matched
0700 directory whose link must point to the exact canonical workspace state. Observation never
creates aliases; unrelated files, incorrect targets or permissions are rejected and preserved.
Normal shutdown removes the owned alias. The real socket remains in workspace state.
This fixes the macOS pathname limit encountered by the packaged-worker test. It does not
establish Windows IPC portability.

## Native shell

The default window now opens the real workspace shell. Home/Videos/Activity/Settings/Help use
the planned slate/light palette, native controls and explicit empty/error/partial states.
The original fictional review/media lab remains under Help and keeps its session-only label.

Native folder choices lead to reviewed workspace creation or non-overwriting validated
guide/channel import. A video draft is built from installed guide questions, channel and
language choices. Choice questions have explicit selectors; per-guide field edits survive
guide switching. Its cap starts at $0. Saving blocks further form edits until the captured
request completes, preventing post-click typing from being discarded by a successful save.

Run review has independent zero spend/request defaults, exact hashes, account routes, stages
and blockers. The confirmation uses the shared typed submission API. Changed selection,
workspace, allowance or queue controls invalidate pending reviews. Unsaved setup has a native
quit prompt; closing the GUI leaves accepted background work independent of the window.

Activity supports bounded queue pages, job inspection, saved event replay, pause/stop/resume,
and moving queued jobs before the previous queued row. The visible event window retains at
most 128 events. Refresh/replay is explicit and labelled as observed saved state. A separate
Stop all lane remains available while the normal request lane is occupied. Late normal-lane
replies cannot overwrite a newer queue control snapshot. Failed workspace refresh preserves
the last successful workspace instead of replacing it with an empty library.

## Verification

- The authoritative root `./scripts/check.sh` passed after the shared changes, including the
  unchanged Clean Code ratchet, architecture, dependency audit, docs and workspace tests.
- That gate exposed existing Tokio 1.53.2's I/O-safety advisory. The root lock was updated to
  1.53.3, an upstream patched version ([RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html)).
  No root advisory exception or license-policy change was added.
- Focused queue tests cover atomic stopped/stopping/event distinctions, restart/idempotency,
  older writers, newer-hold protection, offline inhibition and IPC capability acknowledgment.
- Snapshot tests cover absent workspaces, no implicit queue/configuration creation and corrupt
  queue diagnostics without modifying corrupt bytes.
- Real native Metal tests cover 1440×900, 1024×700 and 760×640; multilingual guide text and
  reachable form actions; an isolated actual draft save with zero cap and review retention;
  zero run defaults and blocked planning without job admission; 66-job pagination; Stop all
  while the normal request lane remains occupied; and terminal saved-event inspection.
- Packaged-binary tests prove display-free start/control/shutdown/restart with a durable hold,
  custom/long library paths, and cross-workspace override rejection before state creation.
- The approved desktop policy audit passes. Six guard regression tests reject broader license
  permissions, another ignored advisory, maintenance filtering, changed pinned versions and
  versionless exceptions. See [dependency exceptions](dependency-exceptions.md).

Native captures are in `/tmp/retold-gpui-g0-evidence`. These are actual GPUI renders, separate
from the interactive HTML design concept. Tests do not resume historical jobs, contact paid
providers, or export production media. Root acceptance suites use their existing isolated fixtures.

## Remaining work

The package remains outside root `crates/*`. Remaining G0 media/IME/VoiceOver/performance and
OS qualification are not waived by the dependency exceptions. G1 still needs complete shared
operation coverage, cross-platform transport and additional failure/resource qualification.
The [navigation/presentation continuation](g2-checkpoint.md) adds command search, persisted
presentation, responsive navigation and shared full-catalog pagination. Complete destinations,
live activity subscriptions, complete provider/account administration and storage UX remain open.

The [native character review journey](character-review.md) now qualifies saved-draft reviewed
execution through generated sheets, exact-content approval, continuation to ready and export.
The [separate native export review](delivery-checkpoint.md) now wires explicit delivery
selection and hashed submission. Native export from CLI-prepared ready output now has a
zero-price fixture journey through completed MP4 streams on the development Mac. Full native
first-launch/source-backed draft creation and packaged-worker delivery remain unqualified.

The [authoritative script editing slice](script-editing.md) now supplies actual master-script
revisions, approvals, conflict handling, undo/redo, history and pipeline consumption. Semantic
shot/cast edits still need shared backend work: several older `review.rs` handlers
record edit history or placeholder approval hashes without changing production documents.
The GUI must not present those as successful content edits. The real editing slice must add
expected-hash checks, authoritative persisted content, correct downstream reuse/invalidation,
history/recovery and cross-client proof before exposing editing actions. Series reuse, asset
administration, complete administration, extended delivery and packaging remain later gates.
