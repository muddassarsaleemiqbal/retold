# Resource efficiency and quality contract

Status: finalized for implementation planning after independent revision 11 approval (see `reviews/cli-plan-review.md`) · Revision 11 · 2026-10-18.
Normative companion to `plan-cli.md`, `cli-product-spec.md` and `plan-series-and-quality.md`.

## 1. Optimization goal and honest limits

Minimize total resources per accepted output while maintaining the user's task-specific quality
requirements. Subscription access is scarce even when an invocation has no incremental USD price.
Local inference consumes time/energy/memory; model evaluation consumes quota too. No route is
preferred merely because its billing label says free/subscription/local.

The target is **less than 1% preventable waste in each measurable resource dimension**, with zero
avoidable duplicate dispatches in deterministic replay/crash fixtures. It is not a guarantee about
opaque subscription metering, stochastic generation quality or provider-side exactly-once behavior.
Unknown quota/usage must remain unknown. Never advertise the target as achieved from a plan review,
a small fixture set, a $0 bill or a missing usage field. Maximum quality means quality constrained
by an explicit reviewed resource envelope, not an infinite search for an unknowable global optimum.

Maintain hard quality floors first; optimize resource use among routes/policies that pass them.
Spend less by avoiding unnecessary work, not by quietly accepting wrong facts, bad pronunciation,
inconsistent characters, missing media/licences or unreviewed delivery. A job that cannot meet
its quality floor inside the envelope blocks with saved evidence and a concrete next choice.

## 2. A resource ledger, not only a dollar ledger

Physical capacity, local hard-limit capability and surviving-worker reconciliation follow
`runtime-editing-contract.md` §3–§4; read-only dry-run never enters dispatch admission.
Extend the existing canonical-account accounting authority; do not create a separate optimization
service. Every provider dispatch reserves a resource vector against account/window, workspace,
video/series, job and operation ceilings. Applicable dimensions include:

- Incremental USD micros, and subscription request/image requests or declared native units.
- Input/output tokens, cache write/read tokens and reasoning tokens when the adapter reports them.
  Record native definitions and overlap: reasoning included in output is not added again; cached
  input subsets are not counted twice. Do not convert these to quota percentages without a verified
  provider conversion. Token estimates are labeled estimates, never authoritative quota counters.
- TTS characters or audio milliseconds, transcription audio milliseconds, encoded vision bytes/pixels.
- Local wall time, concurrency, memory limit, temporary disk and CPU/GPU estimates where observable.

Each value has `unit`, `source: provider|measured_local|estimate|unknown`, a conservative reservation
and a separately recorded observed amount. Preserve unknown observations without settling them to
zero; retain conservative usage holds until reconciliation/reset under the recorded policy. A
provider native request quota may be known while its token quota is unknown; report both honestly.

Canonical billing/quota account identity is separate from executable/profile/route ID. Profiles
pointing at the same subscription share windows, headroom and pool slots across workspaces. If
identity is unknown, conservatively group that provider family's unverified profiles; the user
must establish distinct accounts before separate allowances are granted. Multiple profiles never
manufacture quota. Known reset timestamps are persisted; an inferred time is labeled and no paid
or subscription probe is sent just to discover remaining quota.

### Reviewed usage policies and admission

`--spend` continues to bound money only. `--spend 0` never means unlimited subscription work.
Add `--usage-policy NAME` to generation/test/library/series/export use cases. Named user policies
contain finite per-job/per-operation request/token/modality/time bounds and window ceilings;
first-run policy selection explicitly reviews the proposed operation tree. There is no implicit
unlimited default. Once chosen, a valid policy can be reused without asking for every call, but
each job persists its own limits/plan hash; templates/preferences never persist approval or grants.

CLI: `config usage-policy new <name> --file PATH`, `show/list`, `use <name>`; selected policy is
configuration, not a way to reuse another job's remaining balance. TUI: Setup → Settings → Usage
policies; View plan displays **Money, Subscription usage, Local resources, Uncertainty** separately.
Closed-stdin generation without a selected compatible finite policy returns `RG-USAGE-REQUIRED`
(exit 3), no dispatch. Local-only execution also needs bounded resource policy. Read-only queries,
workspace setup and plan construction require no generation allowance and make zero provider calls.

Policies require request bounds even if token counts cannot be bounded by an opaque CLI adapter;
unknown native token/quota limits are disclosed, not guaranteed enforceable. A policy that demands
a hard token ceiling may use only adapters that can enforce it, otherwise it blocks before dispatch.
Provider-enforced output caps are used when available; kill-after-timeout is not proof of unconsumed
provider resources. Headroom known insufficient waits/blocks; no quiet paid fallback or account
rotation to evade an exhausted limit. Rate-limit health applies to the canonical quota account.

Reserve before dispatch atomically with existing financial checks and lock ordering. Unknown
outcomes keep their request/usage holds; retries/aliases/window changes cannot reset job totals.
Window attribution follows the parent accounting contract, with per-dimension provider reset
metadata; unresolved old-window consumption remains visible and conservatively held as configured.
Two concurrent workspaces/tests/library jobs cannot each consume the same known remaining quota.

## 3. Durable call lifecycle for every billing type

Persist three distinct identities before provider I/O:

1. **Logical operation key**: reuse namespace, declared semantic input versions and effective
   producer/template/schema/model/settings configuration. It is stable across jobs, UI sessions,
   selected usage policies, route aliases and restarts; job ID/policy/attempt ordinal are never
   part of this key. An explicit creative generation revision is a persisted semantic input,
   created once by its reviewed redo action, never a random nonce on automatic resume.
2. **Physical attempt ID**: immutable link to that logical key, original job authority/policy,
   resource/financial reservation, provider/session ID and attempt ordinal. A new job does not
   rename old attempts, borrow their allowance or release their holds.
3. **Artifact/result ID**: immutable content/receipt/provenance identity produced or recovered
   by a physical attempt, attached to the logical key after validated publication.

Admission performs logical-key single-flight acquisition and a durable lookup of all pending/
unknown attempts and verified results **before any dispatch**, including for a new job/policy.
Recover/publish/reuse a saved result once, or expose the original unknown attempt/holds and block.
A different job or finite policy cannot bypass that lookup. Retry/fallback route changes retain
predecessor logical-operation/attempt lineage and uncertainty checks; changing provider settings
is never an automatic way around an ambiguous accepted call. Explicit creative redo creates a
reviewed new generation version with a predecessor link and separate finite authority; the plan
shows old unresolved work/holds and any duplicate-risk exception still requires §3 acknowledgement.
Old holds continue to reduce account/job headroom; new authority never refunds previous usage.

The generation key in §6 is this stable logical key; checker decisions use the same lifecycle
with their checker-specific inputs. Default reuse namespace is one workspace; cross-workspace
reuse is explicit opt-in with identical data/rights/privacy/approval boundaries. Quota accounting
is account-wide regardless of reuse namespace. Duplicate clients attach to saved state; they
never dispatch merely because the old owner process exited. No external exactly-once claim is
made when provider idempotency/recovery is absent.

States distinguish planned/reserved, definitely not dispatched, dispatched/outcome unknown,
raw response durably spooled, parsed/validated, artifact published and settled. Capture request ID,
provider/session ID, timestamps, attempt bounds and available usage. Spool responses as bytes arrive
and fsync the complete response/receipt before parsing; provider logs are private/redacted. Keep
recoverable response data only as long as its referenced attempt/evidence retention requires.

- Definitely not dispatched: release safe unused reservations; retry inside the same operation tree.
- Confirmed rejected without consumption: retain evidence, release the corresponding holds only.
- Known consumed failure: charge actual/conservative resource use; a further planned attempt needs
  remaining allowance and the named failure-specific repair, not a replay of the same invalid input.
- Dispatched/ambiguous: recover saved/provider/session output if supported. No automatic same-profile
  retry, profile switch or paid fallback. If recovery is impossible, show **Outcome unknown; retry
  may consume twice**; require explicit duplicate-risk authorization and a fresh bounded attempt.
  The original hold remains; acknowledging risk never erases it.

Expose `video costs <ref> --request ID` for inspection/reconciliation and `video continue <ref>
--retry-unknown ID --ack-duplicate-risk --usage-policy NAME [--spend USD]` only for the explicit
exception. Tests/library requests use the same application function with their owning command
and operation ID. `--yes` alone never acknowledges duplicate risk. This explicit exception is
reported as duplicate-risk overhead, not proof of exactly-once or preventable-waste success.

Malformed returned JSON first uses bounded deterministic decoding/schema diagnostics of spooled
bytes. No blind regex rewrite of factual content. If semantic repair is needed, plan a separate
capped request with only relevant evidence and the observed validation error. Late results are
quarantined/recovered by original attempt ID and current owner fencing; never committed over a
newer accepted artifact or forgotten before accounting. Crash after provider completion but before
local persistence remains uncertain when no provider recovery/idempotency support exists.

## 4. Bounded operation trees and early stopping

Compile an operation tree before admission: initial generation, candidates, comparisons, semantic
checks, repairs, restaging, transcription, retries and fallbacks all count toward the same reviewed
job and operation limits. Child budgets are slices of the parent's remaining ceiling, never new
independent allowances. A maximum is an upper bound, not a goal to consume. Dynamic branches may
run only if declared in that tree and still within the envelope; expansion requires a new plan.

Default generation is one candidate, regardless of subscription/paid billing. No comparative
judge for one candidate. Run deterministic hard gates first and exclude structurally invalid
candidates before any semantic comparison. Necessary semantic/fidelity QA still runs; comparative
judge elimination is not permission to skip safety/factual/identity checks.

One targeted repair per failed artifact is the initial policy ceiling, followed by a validation
pass over the conservative checker-impact closure in §6.1; defaults are task-qualified and
explicitly versioned. That pass may combine local checks and bounded semantic QA; every physical
call is separately enumerated/reserved in the operation tree before dispatch. The same
repair budget covers alternate providers, safety restaging and model schema repairs; nested loops
cannot each grant another retry. After exhaustion preserve the best usable candidate, all valid
verdicts and blocked reasons, then ask for a bounded new plan. Failed hard gates remain blocking.
A request proven not dispatched may retry only within the declared attempt count. Unknown outcomes
use §3, never automatic retry. No speculative parallel races or best-of-N for speed by default.

A policy may offer an additional candidate only for a named failed requirement with qualified
expected benefit, or explicit user-requested creative exploration with its own ceiling. A passed
artifact stops generation; optional exploratory variants never replace approved work automatically.
Report search/QA overhead and downstream repair consumption, including discarded successful takes.
Saved candidates and judgments resume by ID; never restart a tournament because the UI detached.

Preflight before expensive media: current script/cues approved for voice, storyboard/shared visual
pins approved for full image generation, required licences/fonts/paths/aspects/references valid,
and sufficient declared resources. Cross-branch work remains parallel after each branch's stable
input barrier. Work on unapproved mutable inputs requires a separate explicit speculation slice
naming maximum calls and invalidation risk. Pausing/review must not implicitly spend that slice.

## 5. Minimal sufficient context and provider-cache use

Each step declares an evidence/payload manifest: immutable source/excerpt IDs/hashes and offsets,
required facts, guide fields, schema version, actual rendered prompt hash, tokenizer/estimator
version, reference hashes/bytes/pixels, and input/output/reasoning caps. No inherited full chat or
entire guide/source folder is appended by default. Use small structured outputs with exact fields;
render human summaries locally. Cap explanation/prose length without truncating required content.

Retrieve relevant source passages from a deterministic local index; materialize cited evidence
bundles rather than repeated whole manuscripts. Structured summaries retain provenance and are
reused. A locally checkable coverage manifest lists required facts/citations; for ambiguous claims,
use full source inspection or a separately budgeted retrieval pass. Lexical matching/LLM confidence
is not proof of truth. Missing evidence blocks factual approval; lossy compaction never silently
removes required plot beats, names, numbers, negations or spoiler constraints.

Oversized prompts/reference payloads fail before dispatch with exact limits and a scoped evidence
selection action. Stable prefixes put invariant instructions/examples ahead of variable inputs,
without stale hidden conversation history. Provider-specific context caching is an optional
adapter optimization: verify support, scope/isolation, minimum size, TTL, write/storage/read costs
and usage reporting at implementation. Never pad context, send keepalive/warmup calls or lengthen
TTL just to claim cache hits. Reserve on a no-discount bound; count savings only when reported.
Do not extrapolate API prompt-cache pricing to subscription quota discounts.

Vision: deduplicate identical references, crop/downscale only within task-qualified quality floors;
keep full-resolution originals and coordinate maps. Low-detail screening cannot certify tiny
lettering/face defects; escalate relevant crops/full resolution within the same budget. Count
encoded bytes/pixels, not only image count. Respect provider-specific reference limits.
Audio: cache aligned transcripts/analysis by audio hash, language, model/settings and alignment
schema; reuse qualified provider timestamps/local analysis before any additional external STT.
Per-edition timelines resolve editorial section/segment/word anchors from final measured alignment
(runtime §5.2); changed durations retime only their true descendants, not immutable source media.
Source verification, waveform/loudness/scene/layout checks and metadata formatting stay local where
reliable. No new generative call is needed just to produce a progress message or folder README.

## 6. Distinct generation, provenance and checker identities

Generation key: declared semantic input versions, accepted producer/template version, modality,
model/voice/settings/seed where relevant, output schema and privacy/reuse namespace. Record actual
rendered prompt/template/schema/toolchain/settings hashes as immutable provenance, even when a
logical semantic key remains stable. Changing effective producer behavior requires explicit
version adoption and an impact plan. Unversioned prompt/template drift blocks fresh execution,
not a silent mixture of producers; already approved artifacts remain pinned to their recorded
producer. A cosmetic edit does not relabel old provenance or invalidate approved media by itself.

Checker key: artifact byte/content hash, evidence versions, rubric/checker/schema versions,
model/settings and applicable language/guide fields. A successfully obtained **defect verdict**
is cached immutable evidence too: reuse its major/minor/reject decision and remain blocked as
appropriate. Do not call the checker again just because it rejected content. Transport/execution
failure is not a checker success and cannot be cached as a passed artifact. Reevaluate only after
affecting inputs/rubric change or explicit bounded reevaluation. Keep rejected generated versions
for comparison/recovery; do not activate them as usable results.

Factual hard gates remain distinct from subjective style preferences. Necessary fixes are
versioned local/model edits, not approval of an LLM's unchecked correction. Repeating the same
judge on the same artifact/context is preventable waste, including after job restart. A checker
version change may invalidate only checker evidence; do not regenerate voice/images if their
actual producer inputs and content did not change (including an ASR/model-update-only event).

### 6.1 Conservative post-repair validation closure

A prior verdict certifies only its exact checker inputs, never different bytes by association.
Compute checker-impact closure from the actual edit/generation boundary, not the repair's stated
intention. A stochastic whole-image redraw invalidates every image-content hard gate, including
identity, anatomy, lettering, composition and reference consistency; a whole-script rewrite
invalidates source/factual/identity and other content-dependent gates. Tempo/cut/assembly changes
recheck all potentially affected pronunciation, alignment, seams, loudness and delivery properties.

Reuse a verdict only when typed dependency metadata and hashes prove all its inputs unchanged;
independent licence/source metadata checks can survive a redraw if their own inputs did not change.
Unknown dependency coverage means revalidate conservatively, not assume independence. Prefer one
qualified combined semantic QA call plus local checks over redundant independent model judges,
but do not omit a required gate merely to meet an invocation count. Combined QA must itself be
qualified to detect every included critical property for the task/language/modality.

The reviewed operation tree reserves the complete post-repair validation closure, including
potential additional model calls. If no remaining allowance can validate it, retain the repaired
candidate as unverified/blocked; never inherit old-hash passes or lower a quality floor. Tests:
a lettering repair that changes identity fails identity QA; a style rewrite introducing an
unsupported fact fails fidelity QA; a timing repair breaking alignment/seams remains blocked;
unchanged typed local metadata checks reuse their exact verdicts. No old-hash verdict may certify
new content. Record closure/gate IDs and reuse reasons in the plan/evidence for audit.

## 7. Speech continuity, timing and local reuse

Canonical synthesis units are stable contextual sentence groups/turns within an edition. Each
has text/context boundary, language, speaker/voice/sample hash, pronunciation revision, model and
settings identity. Default grouping is chosen once from the approved script and pinned. A one-line
spoken-form edit regenerates the declared context group and its alignment/true descendants,
not an entire language track. Narrative edits also stale mapped translated/visual/cue dependencies
under runtime contract §5, rather than assuming every textual change leaves visuals valid. The plan shows grouping and spillover before dispatch. Local assembly builds a
continuous edition track with seam/voice/prosody/loudness checks; cropping incompatible recordings
together is not assumed acceptable. A provider lacking qualified group continuity is not silently
used for selective edits; offer a reviewed whole-track alternative with its full usage ceiling.

Timing preflight estimates a duration interval from relevant measured speaker/language history,
punctuation and allowed tempo bounds. No invented point estimate when history is absent. If the
interval does not safely fit the guide window, revise text before synthesis or explicitly plan a
bounded uncertain-duration exception. Do not synthesize a hidden paid calibration sample. Timing
repairs after synthesis remain possible but consume the existing shared repair budget; never promise
that audio can always be generated only once. Text edits require renewed content approval.

Preserve current per-turn pronunciation accuracy ≥95% and configured speaker similarity ≥0.70
when applicable; do not lower thresholds to make cheaper/local routes pass. Pin ASR revision and
inspect per-turn differences; alignment heuristics alone do not establish correct pronunciation.
Provider timestamps/local alignment may eliminate paid STT only when separately qualified for the
language/voice/task. Expose synthesis/transcription/repair as distinct operations in the same tree.
Local audio mix/analysis/depth/images may be reused across aspect formats with identical inputs;
format-specific crop/safe-zone/layout checks remain separate. Each requested render still requires
explicit export intent; previews/highlights are not an excuse to render without authority.

## 8. Quality-constrained route qualification

Use an explicit versioned route-policy table over existing provider adapters, not autonomous
online experimentation or a new model marketplace. Pin model revision where available, otherwise
record reported model identity and refresh qualification on detected drift. Task/language/media
class matters: a route qualified for English metadata is not automatically qualified for Urdu
narration, character continuity or fact-sensitive script writing.

A fixture suite covers factual coverage/citations, narrative coherence/style-to-guide, translations
and multilingual pronunciation, speaker/character identity, layout/text defects, refusal handling,
caption alignment, audio continuity/loudness and final picture/delivery quality. Use deterministic
checks plus blinded human/reference judgments where semantics/perception require them. Candidate
self-scores or a single LLM judge are not the only quality evidence. Keep hard-gate and subjective
rubric results separately. No global 'quality score' masks a critical failure in one language.

For each route/policy record accepted-output rate, hard-gate failures, human preference/defects,
all candidate/judge/repair/failed-attempt resource use, latency and cost per accepted artifact and
per final accepted release. Compare baseline versus candidate on the same source/guide/seeds
where supported. Include failed runs and selection overhead, not just successful cheap calls.

Choose among quality-passing configurations using the measured nondominated cost/quota/latency
frontier and the user's declared preference, not a made-up conversion of tokens/image/time/USD.
A 'cheap' route needing more retries may be dominated. If evidence is absent or sample uncertainty
is too high, retain the proven route or block for explicit user selection; an exploratory override
is labeled unqualified, bounded and retains every hard gate. No automatic cost downgrade.

Quality thresholds and noninferiority margins are preregistered per task before evaluation; no
threshold changes after observing a cheaper model. Use exact factual/identity requirements and
existing voice floors; any stylistic preference margin is explicitly chosen by the guide/user.
Record sample count/coverage, uncertainty, fixture hashes, evaluator/blind-order protocol, settings
and complete usage. Baseline failures are reported, not treated as evidence that all routes pass.
Real provider evaluations and synthesis/render examples need separate current user authorization,
a finite evaluation usage envelope and export intent where applicable. This planning work runs none.

## 9. Waste accounting and the <1% target

Per attempt classify resource use with evidence into productive output, justified quality/search
or user-change overhead, preventable waste, or unknown. Justified overhead requires a reviewed
operation branch and named quality need/exploration request; it is visible in accepted-output cost
and cannot be excused merely because the request was planned. Unauthorized/redundant work remains
waste. A genuine first-attempt stochastic failure is overhead, not hidden productive output.

Preventable examples: duplicate accepted operation, unnecessary invalidation, invalid requests
that local validation could reject, identical judge replay, requests sent after known exhaustion,
unapproved speculation and extra candidates without justified/explicit creative purpose. Record
root cause, owner, artifact/attempt references and counted resource units once. Ambiguous outcomes
and unexplained usage remain unknown until reconciled, never silently classified productive.

For each provider account/window/modality/unit d, report:
`observed_preventable_waste_d / observed_dispatched_usage_d`, counts/coverage and the unknown/
estimated portions separately. Zero denominator is N/A. Do not sum input tokens with image calls
or invent a universal subscription-waste percentage. No padding traffic/keepalives may inflate
the denominator. Where unknown usage has finite bounds, report a conservative waste upper range;
otherwise mark the target unverified for that dimension. Preserve USD, total requests, output
acceptance and quality-search overhead beside the ratio, so a low ratio cannot hide expensive work.

Before any public '<1%' claim, require representative recorded authorized workloads, stated
sampling window and quality/measurement coverage plus an uncertainty analysis appropriate to the
sampling unit/correlated runs. A tiny sample or zero observed events is not sufficient evidence
of a population guarantee. If provider native quota is opaque, a request/token proxy is labeled
as such and never advertised as exact quota efficiency. Missing evidence leaves the claim pending.

Known preventable duplicates/local-gate bypasses are release blockers. The deterministic regression
suite must produce zero such dispatches. The observational <1% target is a separate measured SLO:
missed/unmeasurable dimensions remain visible release limitations with owners, no fabricated green
status. Quality may not be lowered to meet the SLO. User-requested exploration is still bounded and
reported separately; repeated non-useful exploration must be challenged in the acceptance-cost report.

## 10. CLI/TUI feedback, phases and acceptance

Plan/usage view: required productive steps versus optional quality/exploration branches; each
branch's rationale, quota-account identity, minimum/maximum request counts, USD/usage holds,
quality floor, producer/checker versions, uncertainty and early-stop rule. Human summary stays
compact; details contain token/payload vectors. TUI alerts say Usage limit reached or Outcome
unknown, not Unlimited free fallback. Unknown quota is 'not reported'; 0% used is not a substitute.

`spend --resources [--by account|workspace|video|stage]` and `video costs <ref> --resources` return
the same versioned report schema in NDJSON: observed/estimated/unknown units, reservations,
classified overhead/waste and named evidence. No automatic profiling calls. Usage-policy setup,
resource exhaustion and explicit duplicate-risk retry have keyboard/linear input paths with the
same noninteractive errors, draft protection and separate approval/export authority as product §4.

| CLI phase / companion phase | Required owning-layer additions | Acceptance proof |
|---|---|---|
| P1/P2; C1/C2 | Attempt/usage events, call IDs, resource reservations, operation tree and reuse keys | Same-account aliases/two workspaces cannot exceed any known ceiling; $0 not unlimited |
| P3/P6 | Usage policies, account mapping/window health, resource/waste reporting | Unknown quota never becomes 0; profile rename/reset cannot erase holds; no probe calls |
| P4/P5 | Plan/queue feedback, account pools and stable-input barriers | Subscription exhaustion waits; unapproved storyboard causes zero full-image dispatch |
| P7; C3/C6/C8 | Checker reuse, version drift, bounded repair and speech-group edits | Rejected artifact resume makes zero generation/QA calls; ASR-only change rechecks only |
| P8/P9; C4/C5/C7/C9 | Shared source/media/analysis/audio reuse and export-specific planning | One-line edit affects group only; dual-aspect output reuses qualified common work |
| P10 and each relevant phase | Qualification fixtures, resource/quality reporting, drift and crash suites | Quality floors preserved; no cost/waste claim without measured supporting evidence |

Mandatory test matrix: kill before/after dispatch, response spool, parse and publish; subscription
ambiguous timeout; late response; malformed-JSON deterministic recovery; single-candidate no
comparative judge; nested repairs/fallbacks stop at shared bounds; concurrent identical operations; new-job/new-policy restart finds original unknown attempt
and dispatches zero second calls; verified spool recovery publishes once; explicit creative redo
and duplicate-risk retries have separate lineage/authority; post-repair checker-impact closure;
account aliases/opaque quota; cached completed run zero calls; cached rejection zero checker calls;
unversioned semantic drift blocks fresh producer; payload oversize/coverage; no hidden warmup;
context-group edit/continuity/timing interval; cheap route with worse repair-inclusive cost rejected;
preflight invalid font/licence/aspect before generation; exact waste numerator/denominator/unknowns;
local ENOSPC/time/memory bounds; receipt/reconciliation idempotency and all prior safety/UX suites.

Real cost/quality efficacy remains unmeasured until separately authorized evaluation. Code/test
completion, provider qualification, quota visibility and observed SLO achievement are distinct
statuses. Do not label a fake-provider pass as real savings or a production quality improvement.

## 11. Technical reference boundaries

Provider caching mechanics are implementation evidence, not a savings promise for subscription
CLIs. Checked official references on 2026-10-18:

- [Claude prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching):
  reuse depends on matching cached prefixes; cache creation/read behavior and TTL require explicit
  accounting. Use response usage fields to distinguish cache hits; no assumed subscription discount.
- [Gemini context caching](https://ai.google.dev/gemini-api/docs/caching): explicit/implicit cache
  behavior has provider/model constraints and explicit caches can carry storage costs. Verify the
  selected adapter's actual support and measured usage before enabling it.

Do not pin old price tables/minimum-token thresholds in the generic engine or assume these API
features are exposed by CLI wrappers. Qualification records reference the current provider terms,
settings and billing mode used for that run. This adds no new provider integration by itself.


Runtime refinement: `runtime-editing-contract.md` §1–§8 supplies bounded physical/event/index
execution, queue fairness, surviving-worker claims, version-safe edits/dry-run and safe browser
review. Its tests supplement this contract's shared ceilings, no-duplicate and quality invariants.
Known capacity uncertainty is never silently released as if an owner OS lock guaranteed completion.


Production fidelity refinement: `production-stage-contract.md` extends the evidence manifest into
compact material event/claim records, with code/API/HTTP source collection before bounded AI work.
It uses the existing extraction/fact/combined vision/speech QA operation tree rather than adding
a judge per constraint. Required source/scene/pixel/critical-meaning/readiness proofs cannot be
skipped to save usage; if the finite envelope cannot obtain them, the affected work stays blocked.
Source refresh, look-state drawing and targeted evidence retrieval are explicit, version-scoped
work. No empirical quality gain or savings is claimed until qualified representative evaluation.


Algorithm refinement: `algorithm-strategy.md` adds candidate selection, context packing, exact-key
reuse and local check/planning techniques. Every method retains this contract's hard floors, shared
finite operation/usage tree and uncertainty/measurement rules. Adoption requires held-out task/
language-specific ablation with total accepted-output cost; disabling inconclusive candidates is
valid. No similarity/LLM confidence score permits a semantic/vision gate to be skipped.
