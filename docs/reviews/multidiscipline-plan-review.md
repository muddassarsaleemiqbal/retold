# Five-field rewrite-plan review

Status: finalized — all five specialists APPROVE revision 9, 2026-10-18.
User requested at least five specialist review rounds, iterative repairs and final approval.
These are independent agent review perspectives, not claims about human employment credentials.

Scope: `docs/plan-cli.md`, `docs/cli-product-spec.md`, `docs/cost-quality-contract.md`,
`docs/plan-series-and-quality.md`; provider decisions remain a historical adapter baseline.
Existing findings and approvals: `cli-plan-review.md`. Plan readiness only; no implementation,
provider calls, media production, empirical savings or production acceptance is implied.

| Round | Specialist field | Reviewer | Initial status | Final same-revision approval |
|---|---|---|---|---|
| 1 | Performance/resource engineering | `/root/panel_performance` | REQUEST_CHANGES | APPROVE revision 9 |
| 2 | Parallel execution/concurrency | `/root/panel_parallel` | REQUEST_CHANGES | APPROVE revision 9 |
| 3 | Editing/media production | `/root/panel_editing` | REQUEST_CHANGES | APPROVE revision 9 |
| 4 | Reliability/data safety | `/root/panel_reliability` | REQUEST_CHANGES | APPROVE revision 9 |
| 5 | Developer experience/API/testability | `/root/panel_devex` | REQUEST_CHANGES | APPROVE revision 9 |

First three read-only field reviews run concurrently. All field findings will be repaired in
planning docs, then every specialist must approve identical final document bodies. Any material
change after an approval requires a renewed check of that specialist's approval against the new
revision; earlier approvals are never silently carried to changed scope. Document status metadata
and this audit record may be updated after approval, with body identity verified by hashes.

## Findings and revision 9 responses

| Field / finding | Response in `runtime-editing-contract.md` and integrated docs |
|---|---|
| Performance: unbounded event delivery/blocking control | §1 bounded consumers/coalescing/replay, off-executor bulk work, acknowledgement budgets |
| Performance: rebuild freshness/publication | §2 recoverable commit outbox, incremental projection, staged catch-up/atomic switch |
| Performance: local limits merely recorded | §3 aggregate host/filesystem admission, adapter hard-limit capabilities and bounded media I/O |
| Concurrency: aggregate permit order | §3 all-or-none nonblocking claims/rollback, no hold-and-wait or transaction waits |
| Concurrency: owner death leaves physical work alive | §4.1 durable capacity claims, supervisor/process reconciliation, backup quiescence, remote uncertainty |
| Concurrency: fairness/backpressure | §4.2 shared tickets, round-robin/aging/drain bounds, queue caps/lazy DAG |
| Editing: meaning changes can leave images/translations wrong | §5.1 semantic source→translated/visual/cue mapping; pronunciation-only edit separate |
| Editing: absolute pre-TTS cues/variable group retiming | §5.2 anchored intent, measured per-edition timeline and narration-first precedence |
| Reliability: browser review mutation boundary | §7 loopback/Host/Origin/session/CSRF/inert content/manifest media confinement |
| DevEx: edit files can overwrite newer target | §6 target base version/hash in all request schemas and conflict/draft behavior |
| DevEx: dry-run can persist/migrate authority | §6 strictly read-only pre-admission planning, sync commits separate from execution jobs |

All five specialists must now review revision 9 in context, including interacting fixes. Initial
findings are planning gaps, not claims about an implemented rewrite. No runtime or production
validation occurred.

## Final same-revision approvals

Ten specialist passes completed: five initial field reviews and five final rechecks. All eleven
material initial blockers were repaired. All five reviewers explicitly approved revision 9 with
no remaining blockers in their respective fields after inspecting the interacting changes.

- Performance/resource engineering: `/root/panel_performance`, APPROVE revision 9.
- Parallel execution/concurrency: `/root/panel_parallel`, APPROVE revision 9.
- Editing/media production: `/root/panel_editing`, APPROVE revision 9.
- Reliability/data safety: `/root/panel_reliability`, APPROVE revision 9.
- Developer experience/API/testability: `/root/panel_devex`, APPROVE revision 9.

Every approval covers these identical reviewed SHA-256 document bodies before final status-only
metadata changes:

- `docs/plan-cli.md`: `924535528bee8d66993b0712352365ace90babae0739bdb579c5597af77055a0`
- `docs/cli-product-spec.md`: `5c6374d4fa5117055b2d7ea787978b736793c6703085b22af31222baed07e8a7`
- `docs/cost-quality-contract.md`: `49c54c55e55c29166c749eef7a65a0ba2d919952248881c3385bfff84c5f834e`
- `docs/plan-series-and-quality.md`: `02620e3673484d3ea736e21049464372ef77fe9280357fb2e60d2dfd2a15acf3`
- `docs/runtime-editing-contract.md`: `89f5a62770236bb7996875569ec58db79f6345f66c80777e27a1569de4dec952`

Only Status metadata and review records changed after approval. Reversing that metadata update
reproduced every reviewed hash. Links, embedded JSON, fences, whitespace, 24 feature IDs and five
journeys were checked; no production code or runtime checks were performed.

Approval establishes planning readiness only. Implementation, concurrency/crash/security tests,
performance benchmarks, media/assistive/usability checks and measured savings/<1% waste remain
required evidence. No provider calls, video/media generation, commit or push was performed.
