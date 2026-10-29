# Retold replacement completion gates

Goal: replace the legacy CLI with the application-backed CLI/TUI and verify every promised
production journey. Started 2026-10-20 from `22c167b` and the local application assessment.

An implemented command is complete only when its effects on real artifacts and subsequent
execution are verified. A successful response or benchmark fixture is insufficient.
GitHub Actions changes are outside this effort. Live paid calls and production video exports
require separate explicit authorization; fake-provider and temporary-fixture checks remain safe.

## Work sequence

- [x] Workspace foundation: one validated config/library/state context; init, switch, setup,
  inspection, guides, providers, plans and TUI use it consistently; two-root isolation.
- [x] Creation and authoring: new video creates an engine run; user-owned guide/channel
  scaffolds and readiness; no implicit provider requests or exports.
- [ ] Execution: foreground and studio invoke the same account-backed engine; real plan
  verification, durable queue/progress, pause/stop, crash recovery and bounded authority.
- [ ] Series: actual part runs, stable shared inputs, selectors, real plans and scheduling.
- [ ] Editing/review: immutable content versions and selected pins, actual content hashes,
  stale locks, selective dependency invalidation, meaningful undo/redo and atomic writes.
- [ ] Creative tools: read actual supplied/run artifacts, explicit benchmarks, accurate
  model selection, valid reuse/cost recommendations and honest quality evidence.
- [ ] Remaining feature matrix: verify F01–F26 against current product/runtime/stage contracts;
  complete authoring, editions, assets, audio, presets, delivery, backup and recovery journeys.
- [ ] Terminal polish: consistent command grammar/copy, useful errors and actions, usable
  small terminals, correct focus/key scope, accessible plain path and deterministic PTY proof.
- [ ] Cutover: new application owns `retold`; remove superseded CLI implementation only
  after complete user journeys pass; update README, help, completions, man page and schemas.
- [ ] Final local gate: formatting, strict Clippy including acceptance code, Clean Code ratchet,
  architecture, unused dependencies, policy/security, warnings-free docs and full test suite.

## Baseline evidence

- Default full suite: 737 passing, three PTY failures. An earlier activation test failed once
  with AccountBusy and passed in isolation and the complete rerun.
- PTYs omitted terminal dimensions; explicit sizing restored visible rendering. Raw terminal
  output also needs proper interpretation rather than assertions over incremental draw bytes.
- Formatting, strict Clippy, architecture, unused dependencies, cargo-deny, cargo-audit and docs
  passed. The custom lint baseline accepts 4,916 findings and must never grow.
- Confirmed disconnected behavior: edit/approval/undo/redo, studio worker, new/series run,
  workspace resolution and creative dossier/bible/coverage/storyboard/audio handlers.

Progress and verification evidence are recorded below as each complete slice lands. Do not
mark this goal complete until required implementation and acceptance work is finished. Record
provider/human acceptance that remains unavailable without claiming measured production quality.

## Foundation slice — implemented

- Shared, bounded workspace schema and confined physical paths; custom config/library paths
  and symlink/traversal rejection. Explicit roots, remembered roots and normal library queries
  share the same managed context and account identity.
- Setup, doctor, guides/channels/presets, provider/config/profile commands, plans, Home and
  terminal provider views resolve the selected workspace. Preference updates are typed,
  serialized with an OS lock and written atomically; invalid preferences are retained.
- Initialization preserves existing configuration and checks/dry runs do not create probe files.
- Workspace tests now isolate their home and preferences. The prior test-generated expired
  global selection was removed with a backup; no user workspace was relocated.
- PTY fixtures have explicit dimensions and validate terminal screen contents with a terminal
  parser, including incremental cursor updates. Four PTY tests pass.
- Foundation execution: 751 unit/integration tests passed, zero failed; doctests were rerun
  successfully after a source edit invalidated the initial doctest compilation. Workspace
  Clippy, opt-in acceptance compilation, architecture, dependency usage/policy and docs passed.
  The custom lint baseline fell by two findings. Final gates will run again after later slices.

## Current slice — real draft creation and authoring

The replacement now has a real application draft-creation path and shared top-level/nested
`new` command. Drafts preserve the selected guide, channel, source file, edition and cumulative
budget; creation grants no provider or export authority. Money in run manifests remains integer
microdollars (for example, `2.50` USD becomes `2500000`). Invalid inputs and duplicate creation
leave existing files unchanged.

Guide/channel `create`, `check` and `schema` support explicit user-owned scaffolds and imports.
Configuration is validated before atomic publication and never overwrites an existing resource.
Guide loading has bounded text, depth and entry counts and rejects internal symlinks. Channel
imports preserve regular assets with bounded inventory and reject symlink cycles. Placeholder
configuration is reported as unfinished instead of claiming runtime readiness.

Focused proof: five authoring, six real creation and sixteen workspace integration tests passed;
the fresh-workspace journey creates a non-recap explanatory draft without provider processes.
Strict Clippy passed. The Clean Code baseline fell further without increasing any rule.
Legacy cutover has not occurred; background execution and artifact editing remain open gates.

The complete `scripts/check.sh` gate passed after this slice, including acceptance-code Clippy,
architecture, dependency usage/policy, warnings-free docs, the full workspace suite and doctests.
Its final trim removed 5.0 GB of stale generated build artifacts; no source or user data was removed.

## Studio execution core — implemented, qualification continuing

- Foreground and studio share reviewed-plan confirmation, account admission and the actual engine.
  `video continue/export --background` persists a finite, immutable execution intent. Explicit
  idempotency keys return the original receipt after progress; changed requests cannot reuse them.
- Execution runs outside the socket control loop. Pause/Stop requests persist before cancellation;
  active work reaches Paused/Stopped only after a cancellation outcome. Local stage futures drain
  before releasing ownership, and completed cache results survive a cancellation during a write.
- Queue outcomes and final events commit together in SQLite. Critical progress persistence errors
  stop the engine; interrupted owners require review rather than automatic retry. Replay has bounded
  pages and stable durable sequences. Queue admission is bounded to 1,024 pending jobs.
- Source bytes now participate in the plan hash and engine revalidation, with bounded regular-file
  reads. A source changed after review blocks dequeue before engine work starts.
- Reordering preserves other pending positions and refuses active jobs. Prefixes are literal and
  ambiguous prefixes fail. Queries do not initialize queue databases; corrupt storage is reported.
  Startup requires a live acknowledgment and preserves unrelated files at the socket path.
- The maintenance barrier spans durable worker completion. A real worker blocks portable backup;
  after confirmed stop, backup and offline verification succeed. The fixture exposed and fixed
  rejection of the engine's normal state backup and damaged-state recovery files.
- Focused fixtures passed: nine background journeys, four studio CLI and four queue CLI checks,
  six queue durability/capacity/reordering cases, five studio lifecycle/IPC checks, and session/local
  stage cancellation tests. Providers were local fakes; no paid calls or production exports occurred.

Remaining gates include full queue/TUI qualification, aggregate physical admission and owner-death
supervision, real artifact editing and approvals, series production, and the rest of F01–F26.
The legacy binary still owns `retold`; the persistent replacement goal remains active.

The complete local `scripts/check.sh` gate passed for this core, including all workspace tests
and doctests, strict workspace/acceptance Clippy, the smaller lint baseline, architecture,
dependency usage/policy and warnings-free docs. The next creation gate is atomic publication
under failed I/O/concurrent creation before expanding series production.

## Atomic draft publication — implemented

Drafts are assembled in a private staging directory beside their destination. The frozen guide,
channel, source and manifest are checked and synced before an atomic no-replace rename. A failed
write leaves no visible partial run; existing files, directories and symlinks are preserved.
Parent paths are confined to the library, and recovered inactive roots remain protected.

New accounting identities use a 128-bit digest incorporating a fresh random staging nonce and
full timestamp. Deleting and recreating a path cannot inherit the earlier run identity. Resolved
cast ownership is recorded correctly when a guide omits an empty folder level.

All guide origins share bounded filename and content validation, including traversal, case
collisions, hidden paths, control characters and platform-reserved filenames. Source reads are
bounded before draft filesystem changes. Durable directory publication is shared by draft,
authoring, backup and restore paths.

Focused proof: ten creation failure/concurrency/persistence cases, 26 guide checks and 56 CLI
creation/authoring/workspace/backup cases passed. Strict Clippy passed; the lint baseline shrank.
The complete local `scripts/check.sh` gate passed, including all workspace tests and doctests,
strict workspace/acceptance Clippy, the smaller lint baseline, architecture, dependency
usage/policy and warnings-free docs. No paid calls or production exports occurred.

## Bounded queue inspection — implemented

Queue list and IPC return bounded pages with status filters and cursors. The cursor detects
changed ordering/status, inserts and other queue identities; ordinary progress events preserve
it. Job details read status, saved engine outcome and immutable execution options together from
one SQLite snapshot. Stored text, event/outcome records and client/server replies have hard bounds.

The schema expansion adds a queue identity/revision and triggers. It runs in one transaction,
is idempotent and retains all existing job rows. Old readers and writers continue using those
rows; their mutations still update the revision. Read-only inspection can page old databases
without installing an upgrade, with a bounded fallback scan until studio next opens the writer.
Rollback uses the older application against the retained tables; no destructive contraction or
financial-account migration is included.

Focused proof includes seven pagination/migration cases (10,000 history rows, unchanged old
database bytes, repeated upgrades, old writer changes and full rollback after a forced schema
failure), five queue CLI cases, real background outcome inspection and malformed/oversized
reply rejection. The live-follow attachment workflow remains the next execution slice.

The complete local `scripts/check.sh` gate passed for bounded queue inspection, including the
full workspace suite and doctests, strict workspace/acceptance Clippy, unchanged lint ratchet,
architecture, dependency usage/policy and warnings-free docs. Live attachment remains open.

## Live attachment — implemented

`queue attach` now replays and follows committed events until a terminal job or explicit detach.
Plain progress is readable; NDJSON uses typed `job_progress` events with one request ID, immutable
job ID and durable sequences before one terminal client outcome. Saved execution failures retain
their codes; confirmed stops retain cancellation evidence while displaying a stopped job.
`--once` reads one bounded page, including offline completed history and a resume cursor.

Attachment is read-only and never admits, retries, renews or controls work. Acknowledged cursors
move forward only for the attached job. Missing jobs, invalid/future cursors and a disappearing
owner produce explicit outcomes without creating storage or retrying execution.

A bounded output worker keeps a blocked pipe outside the observer/control loop. Ctrl-C/SIGTERM
detach, broken pipes disconnect cleanly, and normal slow consumers receive their complete saved
outcome. Progress/output waits never retain SQLite transactions or account activity ownership.

Focused proof: four application reader/ownership cases, eleven CLI streaming cases (complete
multi-page offline replay, live updates, plain/quiet modes, two observers, cursor resume,
Ctrl-C, closed/stalled pipes, and a slow reader receiving a large complete outcome), plus ten
background journeys with real research artifacts and local fake-provider cancellation/failure.
The published v1 schema includes the additive progress event, with golden envelope round-trip
proof. No paid calls or production exports occurred.

The complete local `scripts/check.sh` gate passed, including 822 workspace test passes with
doctests, strict workspace/acceptance Clippy, unchanged lint ratchet, architecture, dependency
usage/policy and warnings-free docs. An isolated follow-up audit confirmed that studio startup
can currently create queue/lock files below an inactive restore marker; closing that startup
write boundary is the next execution gate.

## Inactive startup and queue boundaries — implemented

Studio startup validates both the selected library and state root before creating files,
including nonexistent descendants and symlinked ancestors. Foreground binding and background
startup acknowledgment join the account maintenance barrier; idle studios release that barrier.
Blocked startup preserves typed inactive/account-busy diagnostics. Status and saved queue history
remain available without initializing storage.

Existing queue handles recheck workspace activity before enqueue, transition, reorder, event,
intent or outcome writes. An inactive marker introduced after opening a queue blocks subsequent
writes while read-only attachment remains available. In-memory isolated queues remain supported.

Focused proof: three application boundary cases and eight CLI startup/lifecycle cases passed.
Fixtures isolate installation paths and verify maintenance rejection, no startup files in inactive
roots, idle maintenance access, alias protection and byte-preserving offline history reads.

The complete local `scripts/check.sh` gate passed, including workspace tests and doctests,
strict workspace/acceptance Clippy, the unchanged Clean Code baseline, architecture, dependency
usage/policy and warnings-free docs. Graceful studio signal shutdown is the next execution slice.

## Graceful studio signals — implemented

SIGINT and SIGTERM now request a studio shutdown through a scoped signal flag. The control loop
stops dequeuing work, persists the pause request, cancels the active workflow and joins it before
releasing the studio lock and account activity. Existing process-group cancellation reaps the
provider and its helpers. Queue attachment shares the signal registration utility and continues
to detach its observer without controlling the job.

Focused proof: both signals interrupt a running local fake provider; each job reaches Paused,
the studio exits and the provider is confirmed absent without writing its completion marker.
All 13 real studio execution cases and 19 attachment/startup/lifecycle cases passed.
The additional failure fixture makes queue storage inactive during a running provider: shutdown
still cancels and joins physical work, preserves the unsettled durable job and reports failure
instead of claiming that a pause/outcome was saved.
Hard owner death and durable orphan/physical-resource reconciliation remain separate open gates.

The full gate exposed provider-start fixture contention under its macOS temporary-directory
setting. The fixture now reports the durable queue on timeout and gives pre-dispatch research
setup a separate 15-second allowance; the one-second control acknowledgment bound is unchanged.
Three consecutive 13-case execution runs passed with the full gate's temporary-directory setting.

The complete local `scripts/check.sh` gate passed after the shutdown/error-path changes,
including 833 test passes (gate tests and doctests included), strict workspace/acceptance Clippy,
the unchanged Clean Code baseline, architecture, dependency usage/policy and warnings-free docs.
Foreground signal cancellation remains the next execution boundary.

## Foreground interruption — implemented

Foreground `video continue` and `video export` now run with explicit engine cancellation control.
A scoped signal watcher handles SIGINT/SIGTERM while the main thread plans, admits and executes;
completion or failure disconnects and joins the watcher. The call returns only after the engine
has drained local work and settled or retained its accounting evidence through the existing gate.
Background submission and observer detachment keep their distinct authority and lifecycle.

Both real-process signal fixtures pass: the fake provider exits before one NDJSON cancellation
error is returned (exit 4, known job ID); completed research survives and no queue is created.
These fixtures make no paid calls and do not render/export production media.

The complete local `scripts/check.sh` gate passed with 835 test passes (gate tests and doctests
included), strict workspace/acceptance Clippy, unchanged Clean Code baseline, architecture,
dependency usage/policy and warnings-free docs. Hard owner death, aggregate physical admission,
artifact editing/review, series and the remaining product/cutover gates are still open.

## Provider termination escalation — implemented

The process runner now waits for the direct child after SIGKILL when the SIGTERM grace period
expires. Previously it could return from termination while the child was still alive, relying
on later destructors to send SIGKILL without awaiting exit. Failure to reap is an explicit
execution failure rather than a successful cancellation/timeout cleanup claim.

A SIGTERM-resistant shell fixture first failed against the prior implementation and now passes:
termination returns only after `try_wait` confirms the child has been reaped. All eight process
runner cases and fifteen real foreground/studio execution cases passed. Descendant supervision,
hard owner-death recovery and durable aggregate physical-resource claims remain open; this
change establishes direct-child cleanup and does not claim those broader guarantees.

An isolated real-process owner-death experiment also confirmed the next defect: after SIGKILL
terminated a foreground CLI (exit -9), its already-started fake provider remained alive. The
experiment used a temporary workspace/account and cleaned up its provider process group. Thus
OS activity-lock disappearance is not quiescence evidence; persistent supervisor/physical claims
must precede any owner-death or backup-safety completion claim.

The complete local `scripts/check.sh` gate passed with 836 test passes (gate tests and doctests
included), strict workspace/acceptance Clippy, unchanged Clean Code baseline, architecture,
dependency usage/policy and warnings-free docs.

## Independent local process supervisor — implemented, durability work continuing

The replacement CLI registers its own executable as a trusted supervisor entry. Local provider
and media calls through the shared process runner re-exec that entry; the supervisor owns the
actual child process group and reaps it. A private Unix connection identifies the owning CLI's
lifetime, rather than polling a reusable parent PID. Losing that connection kills the provider
group and reaps the direct child even when the CLI was killed with SIGKILL.

The supervisor joins account maintenance activity before spawning the provider and keeps it
through cleanup. Stopping the supervisor in a fixture, then killing its studio owner, leaves
backup correctly blocked as busy. Resuming cleanup reaps the provider; backup and verification
then succeed. Foreground/studio SIGKILL fixtures pass, and interrupted studio jobs recover as
blocked for reviewed plans rather than redispatching saved authority. All 18 execution cases pass.

Terminal/session signals cannot abruptly tear down the helper; provider cancellation continues
to target the actual child group. The private managed socket and its empty directory are cleaned
without recursive removal, including after owner death. Ordinary provider exit status, stdin,
stdout, stderr, timeout, cancellation and output bounds remain on the existing runner path.
Standalone engine/provider callers retain their direct launcher unless a trusted host registers
supervision. The legacy binary has not been cut over. Unix behavior is qualified on this host;
non-Unix direct launching does not acquire these supervision guarantees.

This closes the reproduced single-owner SIGKILL leak while the supervisor survives. It does not
close simultaneous owner/supervisor death, durable attempt/resource identity, fenced recovery,
remote uncertainty, aggregate admission or hard resource capability gates. Those remain explicit
execution requirements before production readiness/cutover.

## Durable supervision uncertainty — implemented, recovery still open

Before starting a supervised provider, the account writes and syncs a unique process claim
and holds its OS lock. Confirmed supervisor completion explicitly removes and syncs that claim;
ordinary destructor cleanup cannot erase it. Completion requires reaping the direct child and
confirming the process group is absent within a bounded five-second cleanup check. Unconfirmed
cleanup retains its bounded failure diagnostic, and confirmed groups are removed from later
signal dispatch. If both the CLI and supervisor die, the unlocked
claim survives. Account admission and exclusive maintenance reject it rather than treating
released OS locks or a reusable PID as proof of quiescence. Busy reports include the retained
claim and identify physical recovery as required. Malformed or oversized evidence fails closed.

A real local fake-provider fixture kills both owners, confirms the provider survives, and proves
that backup creates no archive and studio restart is refused. All 20 execution cases pass,
including a known pre-launch executable failure that clears its claim and permits restart.
Three account cases cover abandoned claims, confirmed cleanup and malformed evidence retention.
No paid calls or production video exports were made.

This is a conservative safety barrier, not the complete physical-attempt registry. Claims do not
yet carry the engine attempt/resource vector, worker start identity, fenced publication or remote
request identity. Automatic reconciliation and a supported recovery workflow remain required;
claims never expire or clear merely because a PID is absent. Do not delete retained claims to
resume work without independent quiescence evidence. Aggregate physical admission and hard
resource capability limits also remain open before cutover.

The complete local `scripts/check.sh` gate passed after these changes, with 844 test
passes (gate tests and doctests included), strict workspace/acceptance Clippy, the unchanged
Clean Code baseline, architecture, dependency usage/policy and warnings-free docs.
