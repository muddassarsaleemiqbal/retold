# Algorithm strategy: more useful evidence and quality per unit of usage

Status: finalized for implementation planning after independent revision 11 approval (see `reviews/cli-plan-review.md`) · Revision 11 · 2026-10-18.
Normative companion to the production, resource, runtime and CLI contracts. No algorithm or real
savings is implemented/verified by this document. Prior factual/UX/authority/quality gates hold.

## 1. Objective and existing implementation seams

Aim for the strongest task-quality result inside a finite user-owned resource envelope. Reduce
unnecessary inference through deterministic algorithms, then spend scarce AI capacity on actual
semantic/creative/perceptual work. No universal smartest model/algorithm or globally maximal quality
is promised. Every optimization must beat or usefully complement the qualified baseline for its
work/language class; speed/token savings cannot waive hard factual, identity or media requirements.

Reuse `crates/retold-algo` for pure local algorithms, existing research/dossier/source indexing,
engine graph/store/typed events and provider qualification. Currently present: BM25, MinHash/LSH
near-duplicate detection, cadence/readability functions, blur/color/luminance analysis and loudness/
peak/envelope helpers. Their presence is not proof of multilingual correctness, perceptual judgment
or standards conformance. Verify the intended application; do not label local computation free.

Core deliverables are conservative dedup, deterministic passage retrieval/packing, dependency-based
reuse, bounded planning and honest quality/resource evaluation. RRF/diversity reranking, local
embeddings or additional estimators are candidate policies, enabled only where qualification shows
value. No new vector database, agent swarm, continual training, online bandit or optimization daemon.

## 2. Research algorithms

### A1. Stable indexing and meaning-preserving duplicate grouping

Store immutable captured source bytes, extraction/offset maps and work/edition/lineage IDs before
reduction. Exact normalized-body hashes may group copies while retaining every citation alias.
Normalization preserves original strings and a reversible offset mapping; never delete negation,
numbers, names, punctuation changing meaning or diacritics just to increase duplicate matches.

Use existing MinHash/LSH over shingles to propose near-duplicate groups cheaply, then compare
original passage changes. Approximate overlap is a candidate-retrieval signal, not permission to
drop evidence. Before suppressing any changed text, inspect relevant differences in actor/object/
number/negation/modality/time/attribution and ordered clauses. If semantic equivalence is unproven,
retain both passages/difference contexts and flag the claim as potentially conflicting. A locally
recognized critical difference always survives, but failure to recognize one never proves equality.
Default near-duplicate handling groups candidates and eliminates repeated identical spans; it does
not irreversibly erase nonidentical paragraphs before claim extraction.

Offsets and source snapshots remain authoritative. Similar texts from independent publishers may
be independently useful or copied; lineage uncertainty remains visible. Word-shingle thresholds
are versioned/benchmarked, not factual confidence. Cap buckets/candidate comparisons and checkpoint
large corpora under the existing local memory/time limits; a fallback exact-hash pass can preserve
correctness if approximate grouping exhausts its budget.

Proof: long passages differing only in 'not', actor order, car/body, witness attribution or date
both remain retrievable. Copied sources are not independent votes; identical content is sent once
with aliases; short passages/unknown tokenization are preserved rather than silently discarded.

### A2. Exact-work filters, language-aware lexical retrieval and rank fusion

Filter work/adaptation/part/reveal/privacy scope before ranking. Index source sections as bounded
passages with neighboring context and stable offsets. Reuse BM25 as baseline; add separate ranked
lists for exact entity/event aliases and explicit critical-relation queries derived from typed
coverage gaps. These are deterministic query templates, not a model-generated query fanout loop.

Tokenizer/normalization is versioned by language/script. Preserve negation and names; use approved
alias/transliteration maps without silently translating facts. Whitespace/English alphanumeric
splitting is not assumed adequate for Hindi/Urdu/combining marks or CJK. Missing language support
falls back to preserved full passages/explicit review and is reported, not a zero-relevance claim.

A candidate fusion policy uses reciprocal rank fusion (RRF): combine rank positions instead of
adding incompatible BM25/entity/optional embedding scores. Missing documents contribute nothing;
duplicate ranked lists do not count as independent votes. Use deterministic ID tie-breaks and a
bounded candidate pool. Hyperparameters and query templates are pinned per qualification; no claim
that the RRF paper's settings are universally optimal here. A single useful ranking stays single.

Local embeddings/rerankers are optional only when available, licensed/qualified and inside local
resource limits. No hidden API embedding calls or model downloads. Approximate retrieval may find
support candidates, never satisfy entailment or approve a cached answer. Exact constraints/critical
fact lookup always remain available alongside any semantic-neighbor path.

Proof: critical-event queries recover relevant passages under paraphrases/aliases without crossing
remakes; wrong-language/negation/cause/witness traps and search boilerplate cannot pass merely
because their retrieval score is high. Compare BM25 alone versus qualified fusion per language.

### A3. Diversity and budgeted evidence packing with mandatory coverage

Select a small useful evidence set rather than top-N near-identical pages. A candidate MMR-style
reranker balances relevance with novelty relative to selected passages and source lineages. It
never interprets popularity or source multiplicity as truth. Supporting, contradictory and scoped
uncertain passages for critical claims are protected from novelty suppression.

Use a deterministic greedy coverage/packing heuristic: first reserve mandatory claim evidence,
conflicting context, attribution and local context windows; then add passages yielding the highest
uncovered material-claim/lineage coverage within the actual model input budget. Optional weighted
marginal gain/token rankings operate only after mandatory obligations. This is a heuristic—not
an optimal set-cover/knapsack guarantee or a semantic truth proof.

Coverage has two states: candidate evidence found versus entailment checked. Lexical matching
can satisfy only the first. A relevant URL, keyword or LLM confidence cannot mark the second.
If critical support/conflict context does not fit, split into declared bounded extraction units,
reduce optional material or block for a revised plan; never truncate away contradictions/negations
or silently declare the reduced packet complete. Keep citations/offsets and omission reasons.

Preserve coherent multi-passage context when a pronoun/claim spans boundaries. Original order and
speaker/quotation boundaries remain reconstructible. Measure tokens with the actual adapter's
counter where available, otherwise use a conservative labeled estimate. Pack against effective
input/output/reasoning limits after reserving instructions/schema, not a guessed token allowance.
No padding or keepalive traffic to chase provider-cache discounts.

Proof: repetitive high-ranked synopses do not crowd out the only corrective scene passage; the
Drishyam car/body/road witness distinctions survive packing. Insufficient critical coverage yields
incomplete evidence, not a smaller but falsely confident script.

### A4. Gap-directed retrieval and explicit stop rules

Start from the pinned draft's required events/claims; prioritize unresolved material gaps and
contradictions, including their potential downstream redraw/revoice cost. Source-specific retrieval
first targets existing indexed snapshots; network search/fetch happens only if a named gap cannot
be resolved there. Rank next source actions by likely new relevant/independent evidence and declared
network/model/resource bounds; unknown benefit is not fabricated expected value.

Stop when required coverage/independence/entailment is met; when the finite request/token/time
allowance ends; or when remaining candidate actions repeat already inspected lineage without new
relevant evidence. Repetition stops blind crawling but does not prove a missing fact: preserve the
gap and offer explicit source inspection/addition. No automatic policy increase, no LLM browsing
every result, no source collection to improve a cosmetic count. Unresolvable material ambiguity
blocks dependent work before costly media.

The existing research recipe (3 discovery searches/12 total fetch requests by default) remains the
envelope. All conditional/redirect/retry calls count. A larger topic may need an explicitly reviewed
larger recipe; algorithmic priority does not manufacture allowance or permit paid search without money.

## 3. Generation and validation algorithms

### A5. Dependency slices and memoization, not approximate answer caching

Use reverse dependency indexes to compute the smallest true invalidation closure from changed
source/event/claim/script/look/voice/rubric versions. Stable operation keys and physical attempt
lineage remain separate under the resource contract. Reuse requires exact relevant inputs and
current accepted provenance; a similar question/source/embedding vector never authorizes reuse
of an old factual answer, audio take or checker verdict as if identical.

Incrementally extract only changed source passages/affected claims. Preserve source membership and
aliases; a source deletion/retraction or scope change can invalidate earlier evidence even if its
paragraph bytes survive elsewhere. Reevaluate verifier evidence without regenerating media when
only the verifier changes. A narrative edit stales actual translated/visual/cue descendants;
pronunciation-only edit keeps unaffected frames; a cast-state correction does not redraw the cast.
Actual repair scope, not intended defect, determines conservative post-repair validation closure.

Structural delta/graph traversal is local and bounded; missing dependency coverage forces a safe
conservative closure, not a guess to maximize cache hit rate. Readiness is a cached consistency
join; it never starts a final agent discussion. Replays of completed/rejected/unknown operations
follow the existing zero-duplicate rules, with local resource use still recorded.

Proof: unrelated edits invoke zero upstream model calls; source retraction or changed witness
invalidates affected claims; similar-but-different facts never share semantic result identity;
changed-rubric QA rechecks exact affected evidence without resynthesizing unchanged media.

### A6. Constraint-guided outline, scene and timing planning

Before text generation, allocate required beats/claims under the guide's duration/section/reveal
constraints with a bounded local constraint pass. Check hard constraints first (work scope, essential
coverage, legal event order/reveal dependencies, participants/state, resource feasibility), then
score optional pacing/variety preferences. A bounded greedy/dynamic-programming policy may propose
beat allocation; it cannot prove narrative quality or drop essential facts to make the target fit.
If no feasible allocation is found within the solver/time bound, report infeasible/undetermined
with scope/duration choices, not a false proof that no solution exists or another hidden model call.

Use these typed constraints in one initial outline/draft/storyboard candidate, then existing bounded
semantic QA. A deterministic constraint violation is repaired before media, preserving exact IDs.
Shot coverage may split/recut under true event/state equivalence but cannot add/move a witness.
Temporal/cue resolution and duration-interval fitting reuse measured per-edition alignment; local
layout optimization must preserve spoken words, word timing and exact event/knowledge semantics.

Caption line breaking may use bounded dynamic programming over measured shaped text width, reading
load and legal phrase/word boundaries; hard critical-token/meaning and timestamp constraints always
win over prettier balance. Reuse approved words; no model rewrite for each line break. Unknown
script shaping blocks the relevant layout gate or uses the approved external review path.

Proof: short target cannot silently omit the only explanation of a body/car distinction; spoiler
reveal constraints survive chronology sorting; legal compact captions remain verbatim under valid
many-to-one script mappings; no local optimization falsely declares facts/meaning correct.

### A7. Cheap checks first, qualified semantic/perceptual checks when needed

Run existing schema/path/reference/media integrity, supported blur/clipping/loudness/peak/seam and
text/layout checks locally before model requests. Reuse deterministic DSP/vision/linguistic helpers
where qualified. English readability/cadence formulas are advisory or English-specific, not Hindi/
Urdu correctness gates. Artistic blur, intentional darkness and designed silence use guide-aware
thresholds; a cheap metric cannot reject or certify style without qualification.

A local color-distance/blur/filename/ASR score cannot prove character identity, witness absence,
causal fidelity or critical meaning. All required semantic/actual-pixel/audio/claim gates still run
unless their exact previously valid evidence is reusable. No confidence-only shortcut or sampling
of required frame checks to lower cost. Optional noncritical polish checks may be deferred under
an explicit guide policy; record the omission, never label it checked.

Consolidate compatible hard semantic checks into a qualified bounded request using small evidence
packets. Keep observed facts/constraint results separate from aesthetic suggestions. Additional
candidate/judge/repair branches need the existing finite operation tree and a named failed criterion
or explicit creative exploration. One candidate gets no comparative judge. Stop at passing quality
or bounded exhaustion, preserving best evidence; no endless self-refinement/self-consistency loop.

Local text projections/caption formatting can avoid inference, but semantic content edits still
invalidate required checks even when a diff looks small. A misplaced negation remains critical.
The opportunity is fewer unnecessary calls—not replacing perceptual evidence with cheap heuristics.

### A8. Quality-constrained routing and bounded escalation

Use the existing offline-qualified task/language route table. Compare all consumed USD/request/
token/media/local-time/repair resources per accepted artifact/release, including failed attempts
and judging. Choose within the nondominated frontier by the user's explicit quality/resource/latency
preferences; retain disjoint resource units rather than invented blended 'intelligence' scores.

Cheap/small/local routes are candidates only after hard quality floors and declared noninferiority
criteria pass for that task. A route selected for metadata cannot automatically handle Urdu speech,
character identity or scene entailment. A real drift/unknown class uses the proven route, explicit
bounded unqualified exploration or a blocked prerequisite; no silent downgrade and no new-probe
model call just to score itself. Prefer a proven one-pass route to a cheap route with expensive
repair loops when the full accepted-output evidence supports that choice.

No runtime bandit, auto benchmark, model training or self-changing prompt/rubric. Offline policy
updates are versioned/qualified/adopted deliberately; they never rewrite approved artifacts or
refresh a job's quota. An optional local embedding/reranking model is not a prerequisite to ordinary
lexical research; absent hardware/model capability must not strand existing supported workflows.

## 4. A small explainable algorithm recipe

One versioned recipe binds tokenizer/index/shingle/hash versions, exact-work filter, ranking lists,
optional fusion/diversity settings, mandatory evidence obligations, packing caps, solver bounds,
qualified checks and route-policy version. Persist its actual chosen passages/omissions/decisions
and hashes. Deterministic replay has stable tie-breaks; algorithm changes trigger scoped index or
verdict invalidation, not a blanket media redo. Pin shingle/hash implementation/seed for persistence;
Rust's unspecified default-hasher behavior is not a portable index format contract.

Keep normal UI human-readable: Sources reused / New evidence needed / Fact still uncertain / Why
this shot must change / Estimated usage saved only when based on recorded comparable measurement.
Inspector exposes algorithm versions, evidence selections and receipts. No generated dashboards,
optimization recommendations or verbose model explanations consume subscription just for UI copy.

Existing Setup → Usage policies and Plan/Inspect expose recipe selection/limits. A recipe reference
in settings is preference, not execution authority. All dry-run planning stays read-only; models,
embeddings, downloads, refresh/network traffic and filesystem cache creation are not secretly run
by showing a plan. Present cached plans or prerequisites when computation would require mutation.

## 5. Qualification, ablation and bounded release decision

Compare the current qualified BM25/conservative source flow against candidate combinations:
A1 guarded grouping, A2 ranking/fusion, A3 packing/diversity and A4 gap selection; separately evaluate
A5 reuse, A6 planning/layout and A7/A8 check/routing policy. Add one change at a time or record an
explicit factorial ablation; don't attribute savings to an untested mixture of changes.

Versioned held-out fixtures include: duplicated/mirrored sources, dissenting near-duplicate 'not',
actor/object/witness swaps, Drishyam event/reveal traps, false same-title remakes, citation without
entailment, source retraction, short/long context and evidence at different positions, Hindi/Urdu/
Romanized/CJK queries, caption shaping, noncharacter creative guides, blur/style and audio edge cases.
Separate development/tuning and held-out evaluation sources/works; no tuning on the acceptance set.

Measure relevant/critical passage recall, source-lineage diversity, verified claim coverage and
critical-contradiction detection, plus actual model input/output/cache/request/media units, CPU/RSS/
disk, latency, first-pass acceptance, downstream repairs and blinded human quality preference.
Retrieval 'recall' requires independently labeled relevant passages, not self-labeled model answers.
Mandatory supported critical evidence cannot be dropped to improve precision/token use; all critical
known fixture obligations must survive with zero hidden contradiction/reveal regressions.

Only qualify a candidate when all hard gates pass and it demonstrates a declared useful resource/
latency improvement with quality noninferiority under the preregistered task-specific margins and
uncertainty/coverage protocol. Report failed and inconclusive candidates, small sample limitations
and per-language results. If no candidate is convincingly better, keep the qualified baseline;
don't enable a technique merely because it is named in this plan. Successful local retrieval tests
cannot establish production model quality/savings or the observational <1% waste target.

Fake-provider/property tests cover determinism, bounded operation counts, protected evidence,
exact-key reuse, cache replay and no-expansion/authority rules. Real provider/model/embedding/media
qualification runs require separate current permission, a shared finite evaluation envelope and
explicit export intent when rendering. No evaluation/production generation is run by this task.

## 6. Implementation ownership and stop conditions

| Existing phase / owner | Required result |
|---|---|
| P2/P3, C1/C2; algo/research/dossier/store | A1–A4 conservative evidence/index/ranking/packing and coverage/provenance invariants; bounded source collection |
| P7/P9, C2/C3/C5; engine graph/story/editing | A5/A6 exact semantic reuse/invalidation and constraint-guided outlines/scenes; no truth shortcuts |
| P7/P9, C6/C8; assets/voice/lang/algo | A6/A7 local layout/DSP/vision/caption helpers with language/task qualification and complete semantic gates |
| P2/P6/P10; app/router/qualification tests | A8 recipe/resource reporting, held-out ablation, per-task quality floors and opt-in policy adoption |

Mandatory integration is conservative source handling, exact-key reuse, finite local/model work,
all prior stage gates and the qualification harness—not deploying every candidate algorithm.
Candidate algorithms may be disabled with a recorded result; supported baseline workflows must
remain usable. Existing feature IDs/phases, neutral user-owned guides, explicit approvals, finite
usage authority, secure review, migration and video-export gates remain unchanged.

## 7. Primary references and limits of transfer

Research checked 2026-10-18. These motivate candidate methods, not proof of Retold gains:

- [Manning, Raghavan and Schütze: near-duplicate detection](https://nlp.stanford.edu/IR-book/html/htmledition/near-duplicates-and-shingling-1.html):
  shingling/approximate similarity for candidate grouping. Meaning-preserving guards above are
  additional application requirements; high text similarity is not semantic equivalence.
- [Cormack, Clarke and Büttcher: Reciprocal Rank Fusion](https://cormack.uwaterloo.ca/cormacksigir09-rrf.pdf):
  rank-based fusion is a lightweight candidate for combining retrieval lists. Its reported datasets
  and tuning do not establish optimal settings or guaranteed improvement for this tool.
- [IR textbook discussion of marginal relevance](https://nlp.stanford.edu/IR-book/html/htmledition/critiques-and-justifications-of-the-concept-of-relevance-1.html):
  motivates selecting additional useful/diverse evidence rather than redundant high-ranked results;
  our constrained packing preserves mandatory counterevidence instead of optimizing novelty alone.
- [Liu et al., Lost in the Middle](https://aclanthology.org/2024.tacl-1.9/):
  supports testing how evidence position and context length affect use of retrieved information.
  It is not a universal result for every current model or a license to drop required context.

Keep provider-specific caching/usage constraints in their qualified adapters; API cache discounts
never automatically become subscription-quota savings. Algorithm/recipe adoption is evidence-led,
with actual total resource accounting and unchanged hard quality gates.
