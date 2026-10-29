# Runtime, editing and integration invariants

Status: finalized for implementation planning after independent revision 11 approval (see `reviews/cli-plan-review.md`) · Revision 11 · 2026-10-18.
Normative refinement of CLI plan §19, product §4–§10 and `cost-quality-contract.md`.
Adds no feature families or external services. Application admission/store, existing process
supervision, layout and media adapters own these contracts. Prior authority/quality/UX rules hold.

## 1. Bounded events and responsive execution

Separate three event classes: durable failures/state/attempt/accounting/approval/terminal records;
durable periodic progress snapshots; and coalescible ephemeral per-item progress. Persist critical
records before acknowledging their state change. Ephemeral progress is not advertised as durable
history and may be replaced by the latest value for its item.

Each subscribed client has bounded storage (initial defaults: 128 envelopes and 256 KiB, whichever
is reached first). Oversized critical payloads are artifact/log references, not giant inline blobs.
Coalesce progress first. If a slow client still overflows, disconnect it and retain its durable
sequence cursor for reconnect/replay; don't stall execution waiting for stdout/socket consumption.
Streaming stdout/stdin disconnection is not a successful job cancellation or a second dispatch.
No terminal envelope can be guaranteed delivered to a broken pipe; durable job outcome/query
remains the authority. Replay requests are paginated/bounded and merge into live events by sequence
without gaps or double-executing work. No permanent in-memory backlog for disconnected clients.

Filesystem walks, hashing, image/audio decode, synchronous SQLite and subprocess waits run outside
TUI input and control executors in bounded worker classes. Reserve control capacity separately
from bulk hashing/decode/rebuild. Per-job output spooling has declared size/disk limits; if critical
journal/receipt persistence fails, halt new admission and retain uncertainty/holds rather than
fabricating successful durability or dropping a failure as progress.

Reference-fixture budgets, on the recorded healthy-I/O machine: keyboard response p95 ≤100 ms;
Pause/Stop request acknowledgement p95 ≤250 ms while two jobs and a stalled subscriber run.
Acknowledgement means the control request was persisted/accepted, not that a remote provider
stopped. Show Pausing/Stopping with in-flight counts until terminal evidence. Unconfirmed control
writes show Request not confirmed and an inspect/retry action, never a false stopped state.

Proof: burst events/replay plus a stalled pipe/client keeps RSS/client buffers bounded and input
responsive; critical outcomes survive reconnect; slow bulk I/O does not occupy the input/control
executor; terminal pipe failure never retries a model. Inject full-disk journal failure and assert
no further dispatch or fabricated persisted result. Existing startup/query budgets still apply.

## 2. Incremental, disposable library index

Each authoritative mutation has a stable commit ID, object version/hash and recoverable per-workspace
outbox/journal entry. File-manifest publication and projection updates are not assumed to be an
atomic cross-filesystem/database transaction. Journal the intended commit, atomically publish its
manifest, then idempotently finish the outbox; startup recovery resolves interrupted entries before
projecting them. A half-published commit never becomes an apparently current projection.

Apply outbox updates incrementally; unchanged warm startup/queries read the last complete index
and never traverse every manifest or decode media. Watcher notifications are hints, not the only
correctness mechanism. Periodic bounded reconciliation scans checkpoint by directory/object and
repair missing projected versions from authoritative manifests/journals. Do not send provider calls
or perform implicit index writes on dry-run/read-only commands.

Rebuild into a staging database from manifest versions plus a recorded journal watermark. Keep
serving the last complete index with Last indexed / Updating status. Replay concurrent mutation
commits through a cutover watermark, briefly serialize only projection publication/catch-up, then
atomically switch the active index pointer; apply newer commits afterward. If reconciliation cannot
prove completeness, keep the previous complete index and show the limitation. Cancel/kill discards
or resumes staging without replacing the usable index. Expose per-object freshness; zero results
from incomplete scanning must never masquerade as an empty library.

Every mutating action and exact-release query re-reads/revalidates authoritative identity/version,
not a stale index row. If no complete index exists, show Index not ready and bounded direct manifest
inspection; rebuilding is an explicit/background maintenance action, never hidden dry-run work.

Proof: 1,000-run reference and ≥10,000-run stress fixtures; unchanged warm query performs no full
manifest/media walk; edits/archive during rebuild survive cutover; crash between manifest publish
and outbox completion recovers once; canceled rebuild preserves old index and sends zero calls.

## 3. Aggregate physical resource admission

### 3.1 Shared ceilings and acquisition

Canonical-account request concurrency sits above image/vision/text modality pools. Device/host
memory, inference/model residency and filesystem staging limits apply across accounts/providers,
workspaces and foreground/studio owners. Aliases never multiply capacity. A singleton-loaded
model counts its resident memory once in the host registry, with per-job incremental working-set
reservations; a separately launched model instance counts again. Estimates remain estimates.

All provider/local dispatches pass through the application-owned admission registry; foreground
requests use the same persisted tickets as studio dequeue. No new external broker is required:
short canonical registry transactions arbitrate tickets and OS-backed physical claims enforce them.
A ticket waits without scarce execution permits or open database transactions. Resolve logical
single-flight/result/uncertainty before trying physical resources. Atomically select the full
resource vector in the registry, then acquire all corresponding OS claims nonblocking in canonical
ID order; on any conflict/cancellation, roll back all provisional claims before waiting. Commit
resource/financial/usage holds only after all required physical claims are obtainable and authority
is revalidated. Failure to reserve releases physical claims; consumed/unknown holds are never
released merely because execution permits were returned. No incremental hold-and-wait cycles.

Lock order remains maintenance gate → logical/run/asset ownership per existing rules → complete
physical claims → short registry/accounting transaction; never wait on a filesystem/pool lock
inside a database transaction. Shared registry locks are not held while waiting for permits,
provider I/O or run locks. Ticket arbitration itself performs no network/large filesystem I/O.
Pool reductions drain in-flight work and prevent excess new admission; they do not kill a provider
or pretend memory already freed. One job cannot exceed aggregate or per-modality limits.

### 3.2 Hard-limit capabilities and media bounds

Per adapter declare which CPU/time/process/memory/disk bounds are enforceable, estimated or unknown.
Demanded hard memory/GPU/time/disk caps block unsupported adapters before launch; monitoring/RSS
polling or a predicted upper bound is not enforcement. No claim to constrain unrelated external
applications or a remote provider's hidden work. Enforce supported subprocess groups and bounded
allocations with platform capabilities; record tested macOS/Linux behavior separately.

Preflight decode metadata (dimensions/channels/frame count/audio duration), reject declared limits
and decompression bombs before allocating full media, and use bounded row/frame/chunk queues.
Do not load an entire movie/audio/PNG sequence into RAM. Validate observed stream size as well as
headers. Every task declares working-set/model/staging headroom. Reserve concurrent temporary and
publication space by canonical filesystem identity, including a configured emergency journal margin
(initial 16 MiB); account for copy/verify/publication coexistence, not only final output size.
External disk use can still cause ENOSPC; stop safely, retain receipts/holds and verified/staged
recoverable output, and never delete authoritative history automatically to free space.

Proof: opposite resource combinations across two workspaces cannot deadlock/oversubscribe; canceled
waiters release provisional capacity; large decoders and competing temporary writes block or remain
inside supported caps. Same-model shared residency is counted once, separate workers twice.
Unsupported hard caps fail before work; exhaustion retains lineage/output and never retries blindly.

## 4. Physical attempt supervision and scheduling

### 4.1 Surviving work and capacity uncertainty

Before launch persist a physical-capacity record tied to attempt ID, slot/resource vector, supervisor
identity and worker process-group/session ID. Record process start identity to avoid PID reuse.
Owner OS-lock disappearance is not terminal work evidence. Supervise/reap managed subprocess trees;
Stop terminates cancellable trees and verifies exit before reclaiming local capacity. A surviving
worker after owner death keeps its durable physical claim pending reconciliation; replacement
owners cannot trust freed OS locks alone. Quarantine stale output; only the current fenced owner
may publish recovered artifacts through normal validation.

Recovery inspects supervisor/group state, terminates/reaps orphaned cancellable workers, recovers
spooled results and only then frees verified local capacity. Unknown remote accepted requests
retain conservative remote capacity uncertainty separately from money/usage holds. Default is
no automatic expiry/retry; adapter-supported terminal/cancellation evidence can reconcile it.
An explicit user policy may acknowledge a bounded uncertainty expiry, but the UI/report then says
Remote concurrency unverified and cannot claim a hard remote cap. It neither releases financial/
usage holds nor authorizes a new attempt; duplicate-risk/finite authority checks remain separate.

Quiescent backup requires no surviving managed writes, not merely absence of parent locks. After
exclusive maintenance acquisition, reconcile durable physical claims/owners; if managed workers
or remote-result writers cannot be proven quiescent, reject as busy/unknown with recovery guidance.
Jobs spawned by an owner retain their activity/supervisor claim until actual completion/recovery;
backup never copies output while a stale child is writing it.

Proof: kill owner while a fake child keeps running; no replacement oversubscription or stale
publication; backup refuses until reaped/quiescent; a remote timeout reports uncertainty and never
fabricates cancellation, capacity release or redispatch.

### 4.2 Fairness, bounded queueing and lazy graphs

Use one persistent canonical-account admission order for foreground/studio tickets. Initial bounds:
≤64 pending physical-operation tickets per job, ≤4,096 per account; limits are configurable but
finite and visible. Persist the logical DAG lazily from stable input versions, not every future
asset/edition combination in an in-memory ready queue. Overflow returns Queue capacity reached
with Wait/Pause other job, no dispatch and no leaked provisional reservations.

Round-robin active jobs, one eligible operation per job turn, with a stable creation-sequence/ID
tie-break. New jobs join the tail and cannot jump ahead of existing waiters. Within a job, prefer
unblocking/QA tasks but promote any eligible ordinary ticket after at most eight own-job grants.
A resource-fragmented head may permit at most eight conflicting compatible grants before taking
a drain turn: stop new conflicting admissions until its full resource vector fits; unrelated
resources may proceed. Never reserve part of the vector while draining. Queue reordering affects
pending preference only; it cannot override aging, authority, single-flight or physical/account caps.

With N finite eligible jobs and reusable capacity, a runnable head gets a turn within N job turns;
priority aging/drain bounds prevent endless high-priority arrivals from starving it. This is an
admission-opportunity bound, not a wall-time promise when providers hang, unknown remote work holds
capacity or prerequisites fail. Show the precise waiting reason and recovery path. Canceling a
waiter removes its ticket idempotently without touching already consumed/uncertain attempt holds.

Proof: continuous QA arrivals across workspaces still admit an ordinary eligible task inside the
stated turn bound; large series expansion stays inside queue/RSS bounds; drain turns resolve
fragmentation, cancellation and overflow leave no leaked claims or unauthorized dispatch.

## 5. Semantic edits and edition timing

### 5.1 Explicit edit classes and dependencies

Stable source section/segment IDs map to source narrative, each translation, storyboard shots,
visual facts/character state, voice groups and editorial cues. Distinguish:

- Pronunciation/spoken-form-only edit: original meaning/text/visual facts unchanged; stale the
  affected speech group, alignment and downstream edition timing, not images/translations.
- Narrative-content edit (actions, objects, facts, speaker/state): stale mapped translations,
  storyboard/visual/cue descendants and their QA; re-approve current content before generation.
- Caption display/spelling-only edit: current alignment/text-language constraints checked; stale
  caption/burn-in/layout derivatives without revoicing or reinterpreting source facts.

A translation edit cannot silently change authoritative facts. If classification/dependency impact
is ambiguous, default conservative affected-set review with side-by-side meaning/visual evidence;
never certify no visual impact from a model's confidence. Editing commits no generation. Locked
stale descendants keep exact pinned versions under the existing Keep/Redo protocol; an acknowledged
old look is not permission to export an internally contradictory fact/action. Unresolved source/
translation/visual fidelity mismatches block compatible delivery until mapped content is repaired
or a separately approved source/guide revision resolves the actual requirement.

Proof: change 'he hides the blue envelope' to 'she burns the red letter': mapped translation/shot
stale, old incompatible release not silently repointed, unrelated images reusable, zero editing
calls. Pronunciation-only edit changes one context group/timing descendants with zero image calls.

### 5.2 Anchored editorial intent and resolved timelines

Store music/SFX/ambience/shot/title/transition/chapter intent against stable section/segment/word
anchors and explicit signed offsets/allowed ranges, not guessed pre-TTS absolute seconds. Each
edition resolves its own immutable timeline from its final qualified measured word/group alignment,
voice assembly/version and approved visual/cue pins. Store exact rational/integer timebase and
rounding policy; aspect-specific layout reuses that edition timeline, never another language's timing.

Precedence: required narration integrity and word/caption alignment → guide holds/shot boundaries
and cue semantics → optional musical beat snapping inside permitted ranges. Snapping cannot cut
required speech or move a cue across a prohibited story/word boundary. Reject orphan anchors,
negative/out-of-duration ranges, overlapping incompatible transitions and truncated required speech.
Crossfades are bounded by actual neighboring cue durations and explicit handle/overlap policy;
a fixed 2–4-second crossfade is not valid for every short cue. If a permissible clamp changes intent,
show it in the plan for confirmation; otherwise fail with an edit action.

A middle speech-group duration edit invalidates resolved downstream timing/captions/score/SFX/
titles/chapters/assembly for that edition, while retaining unchanged source media. Resolve from
anchors, not accumulated floating-point shifts. Other editions and immutable saved exports retain
their IDs; a new export produces a new release and unchecked human-review status. Retiming is
local; it never starts rendering, composition generation or extra provider calls implicitly.

Proof: duration-changing group edit shifts all dependent cues/shots/captions/titles/chapters
consistently in one edition; the other language unchanged; orphan/short-cue/transition failures
are explicit; both aspects share timing with separate safe-zone/layout QA and explicit exports.

## 6. Expected-target commits and read-only dry-run

Every revision-changing content/config edit has expected target version AND content hash, in
addition to source/alignment preconditions. CLI flags `--expect-version ID --expect-hash HASH` or
schema fields `base_version`, `base_hash` supply the same precondition; conflicting sources reject.
TUI/live HTML captures the same base at form open. Validate under ownership immediately before
commit. Newly created objects require expected absence/overwrite refusal. Immutable append-only
notes/events do not overwrite a target, but retain exact referenced object/version identity.

A stale target returns `RG-EDIT-CONFLICT` (exit 3) with base/current IDs and Compare current,
Save recovery draft/Copy edits/Reload choices. No artifact, approval, invalidation or dispatch
change occurs from the rejected commit. Preserve readable proposed input as a private recovery
draft on ordinary execution; dry-run writes no draft and identifies the unchanged source file
or returns the proposal only if explicitly requested. Do not expose sensitive text in routine errors.
All authoritative edit schemas/examples must include this precondition, including translation,
captions, pronunciation, voice assignment, take selection, budget and named settings updates.

`--dry-run` selects read-only planning **before mutation admission**. It never persists execution
envelopes/jobs/attempts/reservations/audit rows, registers roots/accounts, creates lock files or
directories, saves drafts/plans, repairs indexes, migrates, backs up/restores files or calls providers.
Read only bounded available manifests/config; do not open/create a write-mode SQLite database.
Return a prospective action/affected set and prerequisites (migration/account/ownership/etc), or
an actionable typed prerequisite; mark stale/live snapshots and revalidate at actual execution.
Busy roots may yield a read-only busy prerequisite, never wait by creating mutation state.

Synchronous config/content commits record a versioned audit commit, not a synthetic generation
job. Only execution/job admission persists job authorization envelopes and operation/resource
reservations. Every execution is still default-zero money/finite usage and explicit-export gated.
A preference change cannot gain spend authority just because it has a commit record.

Proof: two editors submit A→B and A→C with unchanged upstream script/alignment: reject stale C,
retain its proposal, zero approval/invalidation/provider changes. Compare filesystem bytes and
all durable database rows around dry-run init/new/continue/edit/backup/restore on fresh/legacy/
busy/migration-required roots: exactly unchanged, zero provider/external-launch calls.

## 7. Safe browser review boundary

Offline/static bundles are read-only, with no credentials or live action capabilities. Live
review is an explicit `video review <ref> --serve` use case in the existing application layer,
not authority embedded in a shareable file. Bind loopback only; validate expected Host and same
Origin, reject cross-origin and `null`-origin mutation, and never enable permissive CORS.

Use an ephemeral random session capability plus CSRF protection on every explicit non-GET
mutation. Establish the same-origin browser session through an explicit local handoff; tokens
never enter shareable bundles, referrer leakage or routine logs. Expire/revoke on session/server
termination; stale tabs must reconnect deliberately. GET/file requests never mutate. Localhost
alone is not authentication. Session capability is scoped to workspace/run and selected review
actions, never money/usage/export authority.

Actions call existing use cases with exact workspace/run/item/version/hash preconditions and
ownership. Review cannot issue a render/export or grant/expand paid/subscription allowance. An
explicit Redo request may preview/queue a proposal, but dispatch still needs separate fresh job
authority under the CLI/TUI contracts; --yes or browser capability cannot supply it.

Render source/provider/notes as escaped inert text; safely encode embedded JSON, restrict URL
schemes and use restrictive CSP without remote resources, unsafe inline scripts or executable
user/provider HTML. Serve only manifest-approved media via asset IDs, validated canonical roots
and safe MIME types; deny arbitrary paths, traversal, symlink escapes and unapproved root assets.
Browser content cannot ask the server to open external links/paths as privileged commands.

Proof (P7 plus relevant P10 release checks): hostile Host/Origin/null origin, missing/expired
capability, CSRF, stale target versions, `</script>`/HTML/javascript payloads, traversal and
symlink escape cause zero mutations/provider/export calls. Authenticated current-version action
changes exactly its target; static bundles remain inert when copied to another machine.

## 8. Ownership, phases and evidence

| Area | Existing owner / phase | Required gate |
|---|---|---|
| Bounded events/executors and projection freshness | app events/index, P1/P4/P8 | Slow consumer/burst/rebuild/crash and reference-machine responsiveness |
| Aggregate physical resources and orphan supervision | app admission/process adapters, P2/P5/P10 | Combination/cancel/owner-death/capability/media-size/ENOSPC tests |
| Fair tickets/backpressure/lazy graph | app/studio/engine graph, P5/P9 | Stated admission bound, overflow and large-series RSS fixtures |
| Narrative edits/anchored edition timelines | engine/app editing/assembly, P7/P9; C3/C4/C7/C8 | Meaning-change, pronunciation-only, cue/format retiming and fidelity |
| Expected-target commits and strict dry-run | app/CLI schemas, P1/P2/P3/P7/P10 | Cross-interface stale updates; full state comparison with no dry-run writes |
| Live/static review boundary | app review, P7/P10 | Host/origin/session/CSRF/content/path confinement negative tests |

These gates supplement all existing feature/UX/resource requirements. Planning approval is not
proof of runtime performance, secure rendering or generated quality. No production provider/media
calls or paid benchmarking are authorized by a specialist approving this document.


Production-stage semantic refinement: `production-stage-contract.md` supplies material event/
knowledge/reveal/source constraints for the semantic edit closure in §5.1 and the measured edition
timelines in §5.2. Critical source/audio/caption negations and relationships are hard gates; no
Keep pinned acknowledgement can make contradictory artifacts ready. Research fetching obeys the
same bounded resources, untrusted-content handling and read-only dry-run contracts.


Algorithm refinement: `algorithm-strategy.md` applies bounded dependency traversal, local retrieval/
packing and optional caption/beat planning under the existing executor/resource limits. Recipe/index
versions and stable hashing are persisted; unavailable optional algorithms leave baseline flows
usable. Large local loops checkpoint/cancel, never bypass account/host admission or mutate on dry-run.
