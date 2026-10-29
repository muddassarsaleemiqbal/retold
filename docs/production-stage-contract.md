# Production stages: evidence, meaning, quality and efficient research

Status: finalized for implementation planning after independent revision 11 approval (see `reviews/cli-plan-review.md`) · Revision 11 · 2026-10-18.
Normative refinement of the five revision-9 planning contracts. This defines intended behavior,
not implemented accuracy. No actual generated Drishyam frame or licensed film scene was inspected.

## 1. A sequence of proof-bearing artifacts, not a chain of fluent guesses

Keep the understandable flow: draft → research → script → storyboard → cast → images/voice →
captions → ready. Internally it is a dependency graph: textual identity planning may occur early;
only used approved look references are drawn; images and voice run independently after their own
stable-input gates. Captions follow qualified final audio/alignment. No step authorizes export.

Every stage produces a versioned artifact, input/evidence IDs, gate results, coverage/unknowns,
resource receipts and an exact next action. Gates distinguish transport success, schema validity,
source entailment, narrative/visual meaning, perceptual quality and human approval. None implies
the others. A beautiful image that changes the story is a hard failure. Repeating a questionable
claim across script, prompt and QA is not independent verification.

Factual/faithful recap mode does not invent actions, witnesses or causal relationships. A guide
may explicitly allow fictional invention or symbolic illustration; mark it in the artifact and
user-facing review, preserve canon/factual constraints still selected, and never pass invented
content off as captured evidence. This remains a genre-neutral engine, not a bundled recap system.

## 2. Stage contracts and quality/usage improvements

| Stage | Required artifact and quality gate | Avoidable usage removed |
|---|---|---|
| Draft | Exact work/adaptation/language/year/edition; scope/part, intent/audience, duration, spoiler/reveal rules, source expectations and finite usage policy. Resolve ambiguous work before research. | No speculative research/media for the wrong remake/part; explicit reusable settings, no hidden sampling. |
| Research | Captured excerpts and compact event/claim ledger (§3–4), source lineage/conflicts, material coverage. URL/count alone does not establish truth. | Code/API/scraping first; cache/index/deduplicate before AI; retrieve only uncovered/disputed claims. |
| Script | One causal outline and one draft by default, clauses/segments mapped to claims/events; preserved actors, objects, negation, causal/temporal/knowledge qualifiers and reveal order. Fit duration before speech. | No routine best-of-N; repair the smallest semantic segment; only relevant cited packets reach model context. |
| Storyboard | Each shot depicts a coherent supported event slice with visible entities, action/object/state, viewpoint, reveal boundary and constraints (§5). | Catch false co-presence/object substitution before drawing; faithful shot reuse, not more frames per sentence by default. |
| Cast | Text identity/look/state registry; only states referenced by approved shots are generated, versioned and scoped to event interval. | Omitted/optional looks cost zero sheets; no whole cast redraw for a local state correction. |
| Images | Actual-pixel observation versus independent canonical event constraints, plus existing identity/anatomy/style/layout gates (§6). | Same bounded combined QA, cached verdicts, reference dedup; a prompt change is not a reason to redraw approved media. |
| Voice | Approved current narrative/translation/cues, pinned contextual speech group, measured pronunciation and critical-meaning checks (§7). | Reuse qualified audio/alignment; one group repair under the shared envelope, not whole-edition regeneration. |
| Captions | Approved display text mapped to actual aligned speech; protected meaning/entities, legibility/read load and aspect safe zones. | Local derivation/Unicode/layout checks; no revoice or model spelling pass for an ordinary display correction. |
| Ready | Local current-version compatibility/coverage manifest across source→script→shots→image and each speech/caption edition (§8). No unresolved critical contradiction. | Joins cached evidence/gates, zero routine final whole-film judge; reveal missing proof before rendering. |

Human review focuses on changed/material facts, scripts/cues, visual contracts and real release
viewing at their established checkpoints. Review/usage/export remain separate decisions; showing
a new matrix never grants another model call. Every added check fits a declared operation tree;
if it cannot fit, block or ask for a revised finite plan rather than weakening the gate.

## 3. Research should retrieve broadly with code and reason selectively with AI

The current repository already has HTTP/MediaWiki retrieval, source parts, chunking and near-
duplicate paragraph removal. Extend that seam; do not make an LLM browse/read every page or build
a new search service. More independent, relevant evidence can improve coverage; page count alone
can multiply the same mistake and increase context usage.

### 3.1 Source acquisition pipeline

1. Deterministic discovery from user URLs/files and configured public search/API adapters. Pin
   exact work ID before accepting results. Search snippets are discovery hints, not fact evidence.
2. Prefer structured APIs/RSS where suitable; ordinary HTTP HTML/text extraction otherwise.
   Separate main article/section/dialogue from navigation, recommendations, advertisements and comments.
   Preserve relevant headings, negation, attribution and the original captured text/offset mapping.
3. Record requested/final URL, publisher/origin, title, work identity, retrieval time, content hash,
   page revision/ETag/Last-Modified where available, extractor version and evidence excerpt IDs.
   Store bounded immutable snapshots when permitted; a transient URL alone is not a durable citation.
4. Group exact/near-identical content under algorithm strategy A1: approximate similarity cannot
   erase nonidentical critical evidence. Preserve changed negations/entities/relations and source
   offsets; record likely syndication/copy lineage. Retain
   citation aliases, but copied Wikipedia summaries across ten domains do not become ten independent
   confirmations. Shared facts are not automatically duplicates; uncertainty in lineage stays visible.
5. Build a local passage index and a material-claim coverage list, using the algorithm strategy
   A2–A4 ranking/mandatory evidence/contradiction-aware packing and explicit gap/stop rules. Select compact relevant passages
   with surrounding context; don't concatenate whole articles into every generation request.
6. Use a bounded existing extraction/reconciliation call only for selected text needing semantic
   structure, ambiguous pronouns/relations, conflicts or narrative synthesis. Code handles fetching,
   cleaning, hashing, metadata and index lookup. AI cannot promote missing evidence into a fact.
7. Retrieve another targeted passage/source only for a named material gap/conflict. Stop when
   configured independent-source requirements and claim coverage are met, or finite limits are hit.

Broad discovery may examine many candidates, but fetched/AI-read volume is bounded and shown.
Initial user-editable research recipe: at most 3 discovery searches and 12 total HTTP fetch
requests (including validation/redirect/retry requests), ≤4 per origin, ≤1 MiB extracted text/page
and ≤8 MiB extracted text total. Default HTTP wire cap 4 MiB/response, inflated body cap 8 MiB,
≤3 redirects/request chain, ≤2 concurrent requests/origin and 30-second request timeout. Reject
violations before downstream parsing/AI. Larger authorized source types require explicit bounded
overrides, not an automatic escape. Every redirect/retry also consumes its origin/global budget. These are ceilings, not required traffic or a claim that every topic fits. No endless
crawl or new source solely to meet a cosmetic 'lots of sources' badge.

Target at least 3 genuinely independent relevant source lineages for a normal multi-source recap,
including a primary/close-to-scene source where accessible; guide policy may set a different
explicit requirement. Source breadth never overrides critical-event evidence. A complete primary
source can support a specific claim better than many summaries; absent corroboration/primary access
is disclosed. Insufficient coverage returns Research incomplete with exact claims/source actions,
not invented consensus or autonomous budget expansion. Critical scene uncertainty may require a
human inspecting an authorized timecoded source, even when the general plot has many citations.

### 3.2 Caching, network costs and safe extraction

Pin source snapshots for the run. Explicit refresh/adoption produces a source-version impact
plan; unchanged resume does not refetch/reextract everything. Where refresh is requested and
supported, use conditional HTTP validation; an unchanged response reuses captured text/extraction.
Offline mode uses pinned snapshots or reports missing evidence. A revalidated/fresh web page is
not automatically a truer source. AI extraction is cached by content/extractor/schema/producer
identity; changing a style/voice/caption setting never repeats source collection.

HTTP requests/search charges, transfer bytes, extraction time/disk and AI usage are distinct
measured resource dimensions. A paid search API needs an explicit quote/allowance; free HTTP is
not free CPU/network. Honor access rules/rate limits; use per-origin bounded concurrency/backoff.
No paywall/auth bypass or automatic credential forwarding. Fetch only supported public HTTP(S)
URLs with public-address/redirect validation; block private/link-local/file URLs and unsafe schemes.
Local source files enter through the explicit validated file path flow. Downloaded HTML/scripts
are data and never execute; render/extract without running arbitrary page scripts by default.
A dynamic-browser fallback is explicit, bounded and opt-in, never an invisible LLM browsing loop.

Extraction quality checks detect empty/truncated/login/challenge/error pages, wrong work/part,
missing relevant sections, malformed encodings and suspicious source instructions. Do not send
boilerplate or challenge pages to an LLM as if they were plot evidence. Keep source content untrusted
and inert in all prompts/reviews. Robots/access failures produce a source-action error, not retries
through unrelated accounts or an instruction to bypass provider/site safeguards.

### 3.3 Entailment and source strength

Evidence has captured passage IDs/offsets, work scope, assertion status and lineage. Local checks
verify bytes/IDs/scope and coverage; they cannot prove a passage entails an action/witness claim.
The bounded semantic fact check (or explicit human evidence review) compares the actual passage
against the proposed claim, including subject/object, attribution, uncertainty and temporal scope.
A relevant URL plus an unrelated exact quote must fail. Search snippets, source-count thresholds,
model confidence and citation existence never substitute for entailment.

Prefer source material suited to the claim: dialogue/subtitle/screenplay and actual authorized
scene inspection for presence/knowledge/depiction; reliable scene-specific accounts for plot;
creator/publisher metadata for work identity. A mirrored transcript is useful but not automatically
authoritative visual blocking. Record disagreement/limitations instead of silently trusting the
most fluent or newest page. Broad summaries can ambiguously join two events with 'which he saw';
resolve from narrower evidence before directing a shot. No extra mandatory AI call per source.

## 4. Compact event, claim and knowledge records

Extend the existing dossier/shot schemas; do not implement a general-purpose knowledge graph.
Only material events/claims and scene-relevant knowledge need structured entries. A source record
may remain unstructured data until a selected claim needs it. Each material event carries:

- Stable event/claim ID, exact work/adaptation/version and source passage IDs/hashes/offsets.
- Actor/action/object/recipient, location/time interval or relative event order, identity/state
  before/after and causal links that are actually supported.
- Participants with roles, people merely mentioned, known observers and exactly what each could
  observe; presence and direct witnessing are separate relations.
- Event chronology versus audience reveal position/part; who knows/believes/reports what before
  and after, only where relevant. A later investigator is not retroactively a witness.
- Assertion kind: established by selected evidence, character allegation/belief, reconstruction,
  hypothetical/imagined, explicitly allowed fiction, disputed or unknown; evidence/approval versions.

Unknown presence is not proven absence. A positive known witness is not the only possible witness.
An unsupported observer may not be added to a factual depiction merely because nobody explicitly
said they were absent. Record source-supported absence separately from omitted/unverified people.
A reported observation proves at most what the source establishes; seeing someone in a car is not
seeing its disposal or knowing what was inside. Neither pictures nor captions may upgrade that claim.

Keep remakes, sequels and retellings distinct. A later sequel may add retrospective information
only through an explicit versioned adoption link, preserving where/when the audience learns it.
Part-1 generation does not get sequel-only context unless the guide's approved spoiler/reveal
policy allows it. Character design data is never proof that the person was present at an event.

The research→script gate validates these records against captured passages, not just against one
another. Key conflicting/unsupported plot events block approval with a scoped evidence-review
choice. Human corrections are versioned assertions with provenance; they do not erase a source
conflict or silently become independently verified evidence. Reuse accepted events everywhere
instead of re-extracting their meanings separately in each stage.

## 5. Faithful scripting, visual direction and cast

### 5.1 Script and editions

Map factual clauses to event/claim IDs; transitions cannot introduce unsupported causal connections,
witnesses, motives or knowledge. Preserve negation/modality/attribution ('suspects', 'claims', 'did
not see'), actor/object identity, relative time and reveal ordering through compression/translation.
Missing required coverage and fabricated connectors are separate failures. A script can mention
every required noun and still fail because it changes who saw or did something.

One outline and draft remain the default. Apply deterministic schema, duration/coverage/reveal
checks first; the existing bounded semantic fact pass checks clause entailment/critical relations.
Repair the smallest affected segment, then use runtime semantic-impact closure for translations,
shots/cues/media. Do not automatically regenerate upstream research or unrelated approved sections.
Fictional dramatization, metaphor and uncertain reconstruction require explicit guide permission
and a visible representation label; none can change a required factual relation without disclosure.

### 5.2 Scene contracts before media

Each shot selects event IDs and a coherent time/place slice, representation mode, viewpoint,
allowed on-screen people/entities, required action/object/state and known observer relations.
Record positive required facts, prohibited contradictory/unsupported relationships and permitted
illustrative freedom (e.g. neutral invented background design). Scene-critical missing evidence
cannot be filled with a plausible face/uniform/background observer.

Do not put every person named in narration into the frame. If a sentence spans a disposal and a
later investigation, split coverage, reuse a supported neutral insert, or mark a truthful montage
with distinct panels/times; never draw the two as one simultaneous event. A composite/montage must
not imply direct co-presence, shared sightline or knowledge. A dramatic camera choice cannot rewrite
what was witnessed. Symbolic inserts must not masquerade as literal event evidence.

Validate scene contracts locally against event IDs/time/state/reveal constraints; use the approved
bounded semantic check for relations needing interpretation. A false contract blocks image dispatch.
Image generation receives a compiled visual brief, not raw recap paragraphs or the whole cast list.
That brief includes sufficient positive action/relationship evidence, relevant look/prop references
and explicit unsupported co-presence restrictions. Removing narration is not permission to omit
necessary evidence. Keep actual compiled prompt hash/provenance; never rely on negative prompts alone.

### 5.3 Identity references do not define scene participation

Plan identity/look registry once across approved scope, but draw only referenced approved states.
Bind uniform/off-duty clothes/disguise/injury/age/prop possession to event intervals; future state
cannot leak into an earlier shot. Separate supported physical facts from original chosen designs;
actor likeness is not a requirement or default. An attached identity sheet does not authorize that
character's presence, outfit or held prop in every scene.

Reference capacity is adapter-qualified and separate from participant count. Do not remove a key
witness/actor or change story staging simply to fit 'three references'. Faithful coverage may use
separate shots, a qualified composite reference or another approved route inside the envelope;
otherwise block with a concrete composition/route choice. Optional unused looks cost zero media.

## 6. Image QA checks actual pixels against independent evidence

Pass the small approved canonical event/scene packet and actual image pixels to the qualified
combined image checker. It must extract observed entities/actions/objects/relations and compare
them against evidence constraints, not merely score whether the producer's expected prompt looks
matched. Both producer and checker use the same source truth; the checker does not accept the
producer's unsupported expansion as truth. Model self-confidence is not independent evidence.

Return observed claims, checked constraint IDs, contradictions, unknown visibility, hard-gate
outcomes and separate style notes. Inspect relevant foreground/background figures, uniforms,
reflections, silhouettes, sightlines, props and concealment state when they could change meaning.
A polished frame adding an observing policeman to an unsupported event fails. Low resolution or
occlusion cannot certify required absence/identity; use qualified crop/full-resolution inspection
inside the existing envelope, or block the uncertain gate for human evidence review.

Use deterministic image integrity/dimensions/layout checks first; extend the existing combined
semantic/visual QA, not a new model judge per constraint. Cache verdicts by artifact+canonical
constraints/rubric/configuration; evidence/scene changes invalidate their exact descendants.
A full redraw triggers runtime conservative content-gate closure, including identity and meaning.
After bounded exhaustion preserve the rejected/unverified image with the issue; no blind retries,
style-only acceptance of factual defects, or human Keep stale that waives required fidelity.

## 7. Speech, translation and captions must preserve critical meaning

Keep separate versioned narrative text, approved translation, spoken-form normalization and
caption display text, linked by stable segment/claim IDs and qualified word/group alignment.
Cross-script transliteration may be many-to-one/one-to-many; don't force equal token counts or
rewrite narrative words just to match a renderer's tokenization.

Retain ≥95% pronunciation and applicable speaker-similarity floors, and also require zero
unresolved critical meaning discrepancies: negation, actor/speaker, object, named entity, number,
modality and material temporal/causal relationship. 'The police did not see him' cannot become
'The police saw him' even if aggregate ASR accuracy passes. Critical bindings come from approved
claims and language-specific forms/aliases; arbitrary string equality and a model's confidence
are not proof of semantic equivalence. Uncertain ASR/shaping/translation needs qualified review,
not automatic acceptance or repeated paid speech generation.

Use already planned extraction/translation and qualified speech QA/local ASR where possible.
No mandatory paid back-translation or duplicate STT for every line. Calibrated speech-group timing,
continuity, room tone, prosody and emotion checks remain required; factual accuracy does not make
robotic/discontinuous audio acceptable. Changed text/group/settings uses the existing bounded
repair/reapproval and per-edition timing rules.

Captions derive locally from approved display text and qualified measured alignment. Protect
entities/negations/numbers/qualifiers during spelling/line-break/transliteration corrections;
dictionary substitutions cannot silently rewrite a name or action. Validate reading load, line
length, actual script shaping/font support and format safe zones. A display-only edit changes no
voice/images; a semantic correction is an explicit narrative/translation edit with its impact plan.

## 8. Ready is a local cross-artifact consistency gate

Create a versioned preparation manifest per edition: selected event/claim/source versions and
reveal policy; script/translation segments; shot/cast/look/image IDs; speech/critical-meaning/
alignment/caption versions; resolved timeline/cue IDs; every required gate's evidence and current
status; deliberate permitted omissions and unknowns. Join the artifact dependency records locally.
No routine final whole-film model call, new research or hidden regeneration occurs at this gate.

Ready requires complete required coverage and mutually compatible pinned versions, no unresolved
critical contradiction and all applicable gates/approvals current. Individually approved but
contradictory image/narration/caption events do not compose into ready. Unknown/failed checks stay
blocked with item, impact, source/action and retained independent work. Acknowledging stale work
or a human content approval cannot waive a required factual contradiction. Changing the source or
intended story is a separate versioned edit, not a Skip/Accept shortcut.

Ready means preparation complete, no video necessarily rendered. Export/automatic delivery QA/
exact-release final human viewing remain separate. Quality thresholds, usage ceilings and explicit
export authority stay intact; a release can be beautifully rendered yet still fail required meaning.

## 9. Drishyam regression and evidence limitations

Scope is the Hindi 2015 film, with the 2022 sequel as a separately identified later source—not
Malayalam/remake facts merged by title. Checked 2026-10-18 via secondary text sources, not direct
inspection of the original generated frame or licensed film. Supported distinction:

- Car-disposal/lake, body-concealment/police-station and road sighting/report are separate events.
- Gaitonde's account concerns Vijay driving the car elsewhere; it cannot justify adding a policeman
  observing a body dumped into the lake. An ambiguous synopsis joining clauses cannot override
  narrower dialogue evidence about the reported road encounter.
- Later-sequel witness David is distinct from a uniformed policeman and has a separate reveal scope.

Sources/limitations: [contemporaneous Times of India report](https://timesofindia.indiatimes.com/entertainment/hindi/bollywood/news/ajay-devgn-as-vijay-salgoankar-in-drishyam-framed-or-guilty/articleshow/48236456.cms)
corroborates the car sighting; the [mirrored dialogue transcript](https://www.springfieldspringfield.co.uk/movie_script.php?movie=drishyam)
locates the reported encounter and distinguishes the recovered car from the missing person;
[GQ's sequel recap](https://www.gqindia.com/entertainment/content/drishyam-2-ending-explained-what-exactly-happened-in-this-thriller)
describes body concealment and the later witness. These support the event distinction, not an
exact verified camera angle, sightline or exhaustive absence claim. Authorizing an actual film
inspection may strengthen the fixture evidence; do not fabricate timecodes or frame observations.

Regression cases use typed synthetic event/shot fixtures and schematic test images; no movie
clip/export is needed to test local constraints. Require: car→body substitution blocks; later
investigator cannot become earlier witness; road sighting cannot become disposal witnessing;
sequel/Malayalam contamination and premature reveal block; cast sheet doesn't add police presence;
correct storyboard plus extra observing figure fails actual-pixel QA; unknown/occluded witness
visibility cannot pass as absence; a missing spoken/caption negation blocks despite ≥95% ASR;
unchanged good/rejected readiness resumes with zero provider calls. Actual model vision/semantic
accuracy is a separately qualified test, not proven by mocked checker responses.

## 10. Owners, acceptance and source-retrieval references

| CLI / companion phase | Existing owner | Required proof |
|---|---|---|
| P2/P3; C1/C2 | research/dossier/store/guide schemas | Exact work pin, bounded API/HTML acquisition, dedup lineage, source cache, gap retrieval, byte/entailment failures |
| P7/P9; C2/C3/C5 | script/storyboard/editing registry | Clause/event/reveal checks, faithful scene contracts, used-look generation and scoped invalidation |
| P7/P9; C6/C8 | assets/voice/language/caption adapters | Actual-pixel false-witness rejection, reference limits without fact changes, critical-meaning audio/caption tests |
| P2/P7/P9/P10; C9 | app readiness/review/schema tests | Cached complete cross-artifact manifest, honest blocked/unknowns and zero-call readiness replay |

Plan view shows source/event coverage, factual uncertainty and additional required validation
beside resource estimates; detailed records stay in Inspect, with scoped evidence review via the
existing accessible bundle/version/hash workflow. New event/scene fields are schema-versioned and
legacy fixtures migrate without guessed witnesses/meaning: unverified semantics remain inspectable,
but affected new generation/ready claims block until adopted evidence and explicit approval.

Measure stage first-pass acceptance, critical factual/visual discrepancies, qualified human
quality ratings, evidence coverage and resource cost per accepted artifact, including repair and
research overhead. Zero critical contradiction in the chosen regression suite is a release gate,
not a claim of perfect real-world storytelling. No savings/max-quality claim from a plan review.

Technical references for retrieval rather than model browsing: [MediaWiki TextExtracts](https://www.mediawiki.org/wiki/Extension:TextExtracts)
provides plaintext/limited-HTML extraction; [HTTP caching RFC 9111](https://www.rfc-editor.org/rfc/rfc9111.html)
defines validation/freshness mechanisms. These are retrieval facilities, not truth guarantees.
No hardcoded publisher selectors/model prices/quotas are implied; adapter qualification and
access rules remain explicit. All collection/model work stays within finite resource policies.


Algorithm refinement: `algorithm-strategy.md` operationalizes local source grouping, lexical/entity
retrieval, optional qualified rank fusion/diversity, mandatory evidence packing and gap-directed
collection. It also defines dependency reuse and constraint-guided scene/caption planning. Retrieval
coverage remains distinct from entailment, and approximate similarity never licenses deletion of
contradictory or unverified changed passages. Every required semantic/actual-pixel/audio gate remains
inside the existing finite operation envelope; no additional automatic model experiments.
