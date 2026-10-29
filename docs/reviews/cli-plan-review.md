# CLI plan strict review record

Scope: `docs/plan-cli.md`, with `docs/plan-series-and-quality.md` for shared contracts.
Planning review only; no implementation, paid calls, video generation, commit or delivery.

## Round 1 — both reviewers requested changes

Architecture reviewer: `/root/architecture_review_r1`.
Product/compatibility reviewer: `/root/product_review_r1`.

| Finding | Revision 2 response |
|---|---|
| Shared concurrent budgets absent; accounting introduced after studio | CLI §19.2 transactional accounting authority; phase 2 enforcement before phase 5 studio |
| Saved approvals and `--yes` could grant authority; provider tests bypass common guard | CLI §19.1 fresh job envelope, default zero, all paid callers guarded; companion §4.4 aligned |
| Export stage/sample could bypass explicit export | CLI §5 and §19.1 export/project boundaries, regression matrix |
| Foreground/studio ownership, duplicate starts, branch repair and crash semantics undefined | CLI §19.3 owner lock, queue idempotency, state transitions, control and replay |
| Re-keying migration conflicts with side-by-side compatibility and rollback | CLI §16 verified mappings, staging/journal/backup, accounting identity, separate roots |
| Cutover before supported workflows/engine dependencies ready | CLI §17 ten-phase cutover, dependency gates and full command/argument fixture inventory |
| Approve and lock conflict between documents | CLI §19.4 single action, explicit unlock/stale/redo; companion §4.3 aligned |
| Scripts could prompt/hang; JSON/exit semantics undefined | CLI §19.5 closed-stdin behavior, typed NDJSON and terminal outcomes |

## Round 2 — one approval, one request for changes

Independent reviewers: `/root/architecture_review_r2` and `/root/product_review_r2`.
Each received the original brief, all prior findings, responses and the full revised documents.
Approval requires both to find no blocking objections to the same revision.

Product reviewer approved revision 2 with no blockers. Architecture reviewer found one remaining
blocker: run locks do not coordinate mutable shared series metadata and generation.

Revision 3 response: CLI §19.3.1 defines shared registry and asset-generation locks, immutable
publication, durable deduplication/recovery, explicit lock ordering, snapshot-consistent part pins,
series updates that preserve existing pins and a derived-only `used_by` index. Companion §3.3
references that contract. Adds concurrency and crash acceptance evidence for shared publication.

## Round 3 — both reviewers approved revision 3

Both reviewers approved revision 3, including the new series ownership contract.
Architecture: `/root/architecture_review_r2` (round 3 follow-up), APPROVE, no material blockers.
Product: `/root/product_review_r2` (round 3 follow-up), APPROVE, no blocking findings.

Reviewed SHA-256 values before the final status-only metadata update:

- `docs/plan-cli.md`: `e29b41ab5a825ccb5588eeace84eac6eb6bfc9b45166cc0cb1ec9a27f12911bd`
- `docs/plan-series-and-quality.md`: `d1a8b199855b40a7ccd7406c39a2a1ba83f75d01c6bb648c7ccea5142b110587`

Both verdicts concern planning readiness. Implementation acceptance tests remain unrun.
After approval only document status metadata and this review record changed.
Validation: whitespace check and reviewed-file identity checks; no Rust code changed.


## Expanded scope — revision 4

The user requested a larger, detailed product plan: complete features, a polished robust CLI and
structured output folders, followed by fresh strict reviews. Revision 3 approval is historical
and does not automatically approve the expansion.

Added `docs/cli-product-spec.md`: five complete journeys, required F01–F24 feature matrix with
phase ownership and proof, terminal design/accessibility, command grammar, workspace and delivery
layout/schemas, backups, diagnostic privacy and explicit cutover acceptance. Parent/companion
link to it. Plan §19 remains the safety authority; no code or production media is changed.

Expansion round 1: awaiting independent architecture/data-safety and product/completeness reviews.

Expansion round 1: both reviewers REQUEST_CHANGES.

- Architecture: per-workspace financial authority could reset caps/pools; backup had no
  cross-process admission barrier or selected-video reference closure.
- Product: edition edits, templates, views, export presets/selection and exact package-release
  selection lacked executable CLI/TUI/machine contracts.

Revision 5 response: product §8.1 canonical installation account, bindings, shared accounting/
pool/health, inactive clones and activation reconciliation; §8.2 account-wide OS admission/activity
barrier with nonblocking busy rejection, lock order and reference-complete backups; §5.1 typed
action contracts and closed-stdin parity journey for all identified operations. Parent ownership
and accounting sections link these protocols. Expansion round 2: awaiting independent review.


## Expansion round 2 — both reviewers approved revision 5

Architecture: `/root/expanded_architecture_r2`, APPROVE; no material architecture blockers.
Product/CLI: `/root/expanded_product_r2`, APPROVE; no material product/CLI blockers.
Both reviewed all three current specification documents, prior findings and revision 5 responses.

Architecture verified canonical shared accounting/pools/health across workspace roots, inactive
clones and reconciliation, plus admission/activity ownership, backup closure and crash recovery.
Product verified executable edition/template/view/preset/export/package contracts, phase coverage,
terminal/accessibility quality gates, machine contracts and structured-delivery requirements.

Reviewed SHA-256 values before the final status-only metadata update:

- `docs/plan-cli.md`: `9418f7ef4c08b4b9e8dca7c02868a47b2871a88389dfc3a60e5ceca0886818c3`
- `docs/cli-product-spec.md`: `9c98a1841b2802a17f9ba9e9d24d05523c6dbc48756b9e6d44e539eadfe075bd`
- `docs/plan-series-and-quality.md`: `28088110f2374d95cd9eb3e244572c4618e24bb432dc80305e1e6833696ec50a`

After these verdicts only status metadata and this review record changed. Reviewed document
bodies were verified unchanged by hash after reversing that metadata update. All 24 feature rows,
five journeys, local Markdown links, JSON examples, code fences and whitespace were checked.

Planning approval only: implementation, automated acceptance, PTY/visual evidence and production
acceptance remain outstanding. No Rust code, paid calls, media generation, commit or push performed.


## Senior UI/UX specialist review — round 1

User requested a very strict review using the perspective expected of a principal UI/UX architect
with 30 years of professional-tool experience. This is a modeled review perspective, not a
claim about the agent's actual biography. Reviewer `/root/senior_ux_round1`: REQUEST_CHANGES.
Scope: all three planning docs and prior review history; no rendered implementation exists.

| Finding | Revision 6 response |
|---|---|
| J1 skips script/cue review and implies free production | J1 now explicitly reviews, separates authority and handles paid-only/no-route cases |
| Text shortcuts/dirty forms/focus/editor failures unspecified | Product §4.2 scoped keys, named draft choices, modal return and conflict recovery |
| Single lifecycle conceals mixed running/blocked/partial outcomes | Parent §4 and product §4.3 composite states, action precedence and preserved output |
| Generic Accept/Skip may imply bypass | Parent §6.2 and product §4.4 blocker/action matrix |
| Feature inventory lacks navigable hierarchy/launch orientation | Product §4.1 root/object hierarchy, Home launch and Back restoration |
| Final human review has no persistent result | Product §4.5 release/hash review records and explicit review-copy packaging; companion §11 aligned |
| Accessibility fallback lacks equivalent task path | Product §4.6 schema-driven linear authoring and version-pinned external review |
| Snapshots do not prove novice task success | Product §4.7–4.8 participant protocol, task criteria, issue/retest gates |

Also repaired the closed-stdin example so script/voice preparation and explicit approvals precede
translation/caption edits, and caption edits pin existing alignment. No implementation or real
media was executed. Round 2 pending: specialist must review the revised contracts in context.


## Senior UI/UX specialist review — round 2: APPROVE revision 6

Reviewer `/root/senior_ux_round1` reviewed the full revised specification, parent/companion and
prior findings/responses. Verdict: APPROVE; all eight material UX planning findings resolved,
no remaining blocking UX objections. The reviewer confirmed the improvements preserve existing
authority and accounting boundaries. This is a modeled senior/principal review perspective, not
a claim that a human with an actual 30-year employment history performed the review.

Scope: J1 checkpoint/authority flow, scoped keys/focus/drafts, composite states, blocker safety,
root/object navigation, persistent exact-release viewing status, accessible version-pinned
external/linear paths and novice/expert task-quality gates.

Reviewed SHA-256 values before the final status-only metadata update:

- `docs/plan-cli.md`: `fc26f13debafb2faae941cb8773c976607e213f1ff2a9277291d77449b5202bf`
- `docs/cli-product-spec.md`: `da1808499a72cf08f2bb1f97d9ba9deb536a0c60960a49ebb9c1d5afa0a07673`
- `docs/plan-series-and-quality.md`: `8c2ff3033675338132f0eeaecb7c28adf67713731362f4cdb4086cf8f0086e8b`

Only status metadata and this record changed after specialist approval; body identity was checked
by reversing the status update and comparing hashes. Links, code fences, embedded JSON, whitespace,
24 feature rows and eight interaction-contract sections validated.

Revision 6 is finalized for implementation planning. Architecture/product revision 5 approvals
remain recorded for that baseline; this specialist approval covers the revision 6 UX refinement.
No implementation, terminal rendering, assistive compatibility or participant usability results
are claimed. Those remain mandatory implementation/cutover evidence. No production calls/media,
commits or pushes were performed.


## Extreme cost-and-quality review — round 1

User requested minimal subscription waste (target <1%), lowest feasible cost and highest
quality. Reviewer `/root/cost_quality_critic_r1`: REQUEST_CHANGES, nine material findings.

| Finding | Revision 7 response |
|---|---|
| USD0 leaves scarce subscription work unbounded | Resource contract §2 shared account resource vectors, finite usage policies, aliases/windows/unknowns |
| Subscription crash/timeouts duplicate work | §3 durable every-call identities, raw-response spool, safe recovery and explicit duplicate-risk exception |
| <1% undefined | §9 attributed per-unit waste/overhead/unknown ratios, sample/coverage caveats and zero-duplicate fixtures |
| Billing-dependent candidate/judge/repair loops | §4 one bounded tree, default one candidate, no comparative judge for one, shared repair ceiling |
| No empirical quality-constrained routing | §8 qualified task/language routes, hard floors, human/reference evidence and repair-inclusive frontier |
| Prompt/reference/audio payloads lack bounds | §5 sufficient cited evidence, payload caps, qualified crops/local checks and measured optional provider caching |
| Semantic drift and repeated rejecting QA | §6 distinct producer/provenance/checker keys and reuse of defect verdicts without activation |
| Speech granularity/timing contradictions | §7 pinned contextual groups, seam QA, duration uncertainty and common synthesis/STT/fallback envelope |
| Costly work before stable inputs; local compute called free | §4 branch approval barriers/speculation slice and §2/§7 local/format reuse resource bounds |

All four normative documents link the resource contract and align conflicting rules. Provider
decisions are explicitly labeled historical/current baseline versus future execution policy.
Strict round 2 pending. No provider benchmarks/evaluations, paid calls or media were run.


## Extreme cost-and-quality review — round 2

Reviewer `/root/cost_quality_critic_r1`: REQUEST_CHANGES; nine original findings substantially
resolved, two remaining blockers:

1. Intent-only changed-property QA could miss new defects after repair. Revision 8 resource
   §6.1 defines a conservative checker-impact closure over actual changed bytes/inputs, full hard
   gates after stochastic redraw/rewrite, qualified combined QA and complete validation budgeting.
   Companion §6.3 aligned; insufficient allowance keeps the candidate unverified/blocked.
2. Job-bound identity could evade single-flight/unknown-attempt recovery after restart. Revision 8
   §3 separates stable logical key from physical attempt/job authority and immutable result identity;
   admission checks unresolved lineage before any new-job dispatch. Explicit creative redo and
   duplicate-risk retry remain distinct and cannot clear prior holds or borrow old allowance.

Round 3 pending; no real usage/savings/quality evidence is claimed.


## Extreme cost-and-quality review — round 3: APPROVE revision 8

Reviewer `/root/cost_quality_critic_r1` reviewed the complete resource contract, parent/product/
companion integrations and historical-provider baseline distinction. Verdict: APPROVE; no
remaining blocking cost/quality planning objections. All nine original gaps and the two round-2
blockers have explicit design contracts and acceptance proof requirements.

The reviewer confirmed stable cross-job logical admission with retained physical-attempt
uncertainty/authority, conservative full post-repair checker coverage, bounded resource use and
quality-qualified optimization without weakening prior UX or approval/export boundaries.

Reviewed SHA-256 values before final status-only metadata updates:

- `docs/plan-cli.md`: `2e5bbbb6a775af02a3c07d2a971e10b7e8f930809b55a066a3cbc050feeb8792`
- `docs/cli-product-spec.md`: `1cf4ef25f5a259f8f5bfd3cea90cc6430537d09b5ace1312f828c00ded682766`
- `docs/plan-series-and-quality.md`: `4fe97736baaba92732855ef55b9d911bb50161baf3a201c385f88abeba29395e`
- `docs/cost-quality-contract.md`: `4b0138e72b99a18ec0b75cba5678ae3d1fef061cc6a694dfc892c03e7952178b`
- `docs/provider-decisions.md`: `e8211130d1f08e8b13f7c6c92934613d777e6f63133ea824d3cb3ba66fe97bd2`

After approval, only document status metadata and this review record changed. Reviewed bodies
were hash-verified unchanged after reversing the metadata update. Local links, JSON examples,
code fences and whitespace validated, including untracked plan files.

Revision 8 is finalized for implementation planning. Historical architecture/product revision 5
and senior-UX revision 6 approvals remain attributed to their respective scopes. No actual
savings/quality/opaque-quota accuracy or <1% observational SLO achievement is claimed. No code
implementation, provider evaluation, paid/media calls, video export, commit or push occurred.


## Five-field review panel — revision 9 finalized

See [five-field review record](multidiscipline-plan-review.md) for five independent specialist
rounds (performance, parallel execution, editing, reliability/data safety, developer experience),
eleven repaired blockers and five final same-revision approvals. Ten field-review passes total.
The five normative bodies were hash-verified identical across final approvals; only status
metadata and audit records changed afterward. Runtime/editing/security/testability refinements
are required at the mapped phases. No implementation or empirical acceptance is implied.


## Deep stage-flow review — round 1, revision 9 baseline

User requested maximum stage quality with less usage, specifically false Drishyam co-presence/
witnessing; then asked for broad multi-source scraping/retrieval instead of token-heavy AI browsing.
`/root/flow_fidelity_harsh_r1`: REQUEST_CHANGES (six related event/entailment/shot/cast/pixel/readiness
gaps). `/root/flow_production_harsh_r1`: REQUEST_CHANGES (five related evidence/actual-pixel/reference/
critical speech-caption/readiness gaps).

Revision 10 response: new `production-stage-contract.md` supplies per-stage artifacts/gates/usage
reductions; API/HTTP-first bounded multi-source collection and private snapshot/index/dedup/lineage;
compact cited events/claims with witness/knowledge/time/reveal/unknown scope; clause entailment;
coherent shot participants; only used look-state drawing; actual-pixel QA against canonical event
constraints; critical-meaning checks beyond ≥95% ASR; local current-version readiness manifest.
Parent/product/series/resource/runtime integrated; old architecture plan labeled baseline.

Drishyam research distinguishes 2015 Hindi car/lake, body/police-station and reported road sighting,
with 2022 sequel witness separately scoped. Dialogue mirror, contemporaneous TOI and GQ recap are
secondary evidence; no original generated image or film blocking was inspected, no fabricated
timecodes or universal absence claim. URLs/limitations recorded in the stage contract.

Round 2 pending: both specialists must review stage contracts and existing invariants together.
No code, provider generation/evaluation, video/media export or measured savings performed.


## Deep stage-flow review — round 2: both APPROVE revision 10

`/root/flow_fidelity_harsh_r1`: APPROVE; all six factual/narrative/visual-fidelity blockers resolved.
`/root/flow_production_harsh_r1`: APPROVE; all five production-quality/efficiency blockers resolved.
Both inspected the full stage contract and integrations into the five prior normative documents
and original architecture baseline header. No remaining blocking objections in this scope.

The review also covers the user's research follow-up: deterministic API/HTTP/scraping/extraction/
dedup/index/snapshot collection first; bounded AI for selected semantic extraction, reconciliation,
writing and independent-evidence QA; finite search/fetch/body/time/per-origin caps, independent
source lineage and critical-claim coverage instead of page count.

Reviewed SHA-256 values before final status-only metadata updates:

- `docs/plan-cli.md`: `85feefa829e48b0bdd022049a42e18d0d1461dffdcfb0ecfcf2943cabe8486bc`
- `docs/cli-product-spec.md`: `7bd4f01b4a548f24f87f8bae604206545c2df8756b860d3da16d539e1e550fdd`
- `docs/plan-series-and-quality.md`: `bdc85e7edcb65e59a36a7af31b1e6376686292c6e6c2daeddaf07e6edfeab595`
- `docs/cost-quality-contract.md`: `68ce4a2d66678bcee7164b268eb182d0ea3b5e9d9ce8833bb55cda62435c21b0`
- `docs/runtime-editing-contract.md`: `fe267edf2c9c3721b5e2095f680ff9ecba4af68de797ef33b8a109cfd85bab5c`
- `docs/production-stage-contract.md`: `7fa9970428e17e8d49970fdbeca1aadd840f43e4d4c47bbc35d3f0cbbd7f022b`
- `docs/plan.md`: `aa2be19c2099ee885b1c9ce15f4b400cbd50a0f08ec1366f61b4987c73c4f5a7`

Only Status metadata and this audit record changed after approval; reversing metadata reproduced
the reviewed body hashes. Local Markdown links, embedded JSON, code fences, whitespace, 24 feature
IDs and five journeys validated. Revision 9 five-field approvals remain historical for their scope.

Revision 10 is finalized for implementation planning. No claim of perfect semantic/vision accuracy,
maximum real generated quality or measured savings/<1% SLO achievement. No original generated image
or licensed film scene inspected, no production provider/model/media/export calls or benchmarks,
no implementation/commit/push performed. Web research verified the cited event distinction only.


## Algorithm intelligence/efficiency refinement — revision 11 pending review

User requested researched algorithms that lower AI usage while maximizing quality. Inspected existing
`retold-algo` retrieval/linguistics/vision/DSP interfaces and primary IR research/textbook sources.
New `algorithm-strategy.md` reuses BM25/MinHash/local helpers with conservative meaning guards;
adds candidate RRF/diversity/mandatory-coverage packing, gap-directed stop rules, exact dependency
reuse and bounded outline/caption constraints; retains all semantic/perceptual hard gates.

Core safeguards/harness are mandatory; optional algorithms are evidence-qualified, not required
to be enabled despite worse/inconclusive results. Per-task/language held-out ablation includes
critical-evidence/contradiction tests and complete accepted-output resource use. No production
model/embedding/benchmark/media calls or empirical savings claim. Two independent strict reviews
will check algorithm/evidence correctness and resource/quality integration before finalization.


## Algorithm refinement review — both APPROVE revision 11

Completed 2026-10-18. Independent read-only reviewers `algorithm_review` and
`integration_review` approved revision 11 with no blocking objections.

- Algorithm reviewer verified current retrieval/linguistics/vision/DSP seams, all four primary
  references, conservative deduplication, mandatory counterevidence, finite collection, exact reuse
  and independently labeled held-out qualification.
- Integration reviewer checked all seven documents for phase ownership, resource/authority limits,
  critical evidence retention, semantic/perceptual hard floors and optional-policy adoption.
- No substantive revision was required by this review round. Only Status metadata changed after
  approval. Each reviewed body was verified unchanged below its metadata.
- Local validation passed: seven documents, nine Markdown local links, one JSON example, balanced
  code fences and no trailing whitespace. All four research references resolved and were checked
  against their limited attributed claims.

Reviewed SHA-256 values before status-only metadata updates:

- `docs/plan-cli.md`: `5dc08d292b9e0998bd1eb9692fc15f4715a821776282cb47425280ffd1dce4f2`
- `docs/cli-product-spec.md`: `6f1944398b2d97d45040b506a5c28de9641a3a30e4839501af87442d67cc75be`
- `docs/plan-series-and-quality.md`: `9ecb092cff1a9b46b676c33b8be0aa0cf1e777b6fb1a9ee1b66fb2fff8c3f03e`
- `docs/cost-quality-contract.md`: `7ee8f57b9adbed7669aed14e2fbc59ae8297c83bfe66649af1e76f0d2d748e58`
- `docs/runtime-editing-contract.md`: `9d537f787e3ff708c41eca1fda1eb2d510590800e3842e2ff36c18e994ec5765`
- `docs/production-stage-contract.md`: `70f9a40353c6fe2c107829effec84719bcd649d40466cbed36cf45ee1d6030c0`
- `docs/algorithm-strategy.md`: `d659328be573c1b1d4dde78e856b558f8907ec0a560ca05bd6ea000589052018`

Final SHA-256 values after status-only metadata updates:

- `docs/plan-cli.md`: `b0274c16b942b3e0681d8da9903ad826a404dfbf59c26edee5dcdd48804958b8`
- `docs/cli-product-spec.md`: `089e923e20fe9d71246ab9642caac246682487caa4b909e8cebfb4c5a82afcbb`
- `docs/plan-series-and-quality.md`: `41c0dcb7873e67254c5e21789c145b7dfed4477a6dfe0a80e90086cc62c64640`
- `docs/cost-quality-contract.md`: `96d83f49fb1641213132f9ef309bd1af9c1f921516de2bdd86441960561c7ba2`
- `docs/runtime-editing-contract.md`: `cffde7704805717b20c0485b86cf3b4a7e9d7cc6bbc6cb30b1027fdd1a264511`
- `docs/production-stage-contract.md`: `be568544bdc38dee79e0dac3d7b6bb696d96d7fade7b483916d8dc793135e7ab`
- `docs/algorithm-strategy.md`: `8cacdd153d1b7e6731fd50958e38406840c6f32bd5f11fae2da7a462b95d6424`

Revision 11 is finalized for implementation planning. Optional algorithms must demonstrate useful
resource or latency improvement with quality noninferiority before adoption; inconclusive candidates
retain the qualified baseline. No implementation, empirical savings, production quality improvement
or <1% waste result is claimed. No production provider/model/embedding/media/export or benchmark
was run, and no commit or push was performed. Existing unrelated workspace changes were preserved.

## Subsequent implementation scope update — 2026-10-18

The user requested the first implementation slice, then explicitly deprioritized legacy
compatibility. The plan/product-spec now record that legacy command inventories, aliases,
adapters and parity fixtures are outside the current implementation scope. Existing-state,
accounting and permission safety remain required when accessing existing state. Reviewed hashes
above identify the planning baseline before this subsequent user-directed change, not the current
implementation or amended document bytes. Phase 1 delivery and local evidence are recorded in
`../implementation-phase-1.md`; no independent code-review approval is claimed by the plan reviews.
