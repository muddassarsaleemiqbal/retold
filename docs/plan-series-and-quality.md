# Plan: Series (canon) rebuild, zero-waste execution and film-grade output

Status: finalized for implementation planning after independent revision 11 approval (see `reviews/cli-plan-review.md`) · Written 2026-10-18 · Revision 11 after the Drishyam (Hindi, 3 parts) production run.

The CLI plan §19, `runtime-editing-contract.md` and `cost-quality-contract.md` own shared authorization, multi-resource accounting,
every-call recovery, operation budgets and quality-constrained optimization. CLI §19 owns ownership and artifact-state
contracts. This document does not grant spending or export authority. CLI plan §17 maps the
cross-plan dependencies and the final cutover gate. `cli-product-spec.md` adds the required
workspace, terminal, edition/asset review and structured delivery contracts; its features are
implemented through the mapped CLI phases and this document's existing engine phases.
Scope: the canon/series system, the execution and cache model, failure handling, image
generation, Hindi/Urdu language work, and the edit (music, sound, picture). Drishyam is not
being continued; it is the test case this plan learns from.

---

## 1. What went wrong in the Drishyam run

Every issue below happened in the run. Each has a cause and the section that fixes it.

### 1.1 Wasted usage (subscriptions and money)

| # | What happened | Root cause | Fix |
|---|---|---|---|
| W1 | Scripts for all three parts were written about three times. Storyboards and follow-on work were redone each time. | Each part's script prompt included the other parts' openings (`canon/openings.json`). Writing a sequel changed the prompt of an earlier part, so the earlier part regenerated, which changed the sequel's prompt again. | §3.2 the series plan fixes openings up front; §4.1 dependency declarations |
| W2 | Part 1's storyboard flip-flopped and 16 finished images were redrawn. | The cache key is a hash of the whole prompt text (`context.rs` `ask`). Any wording change, bug fix or extra context invalidates the step and everything after it. | §4.2 semantic keys |
| W3 | Adopting a guide with only a music-volume change started rewriting the script. | `--upgrade-guide` calls `bump_revision("research")`, which invalidates every stage. | §4.2 scoped guide fields |
| W4 | Steps finished earlier were replaced without asking. | Nothing is ever locked; the newest upstream output always wins. | §4.3 locks and stale marking |
| W5 | Codex profiles were retried after hitting their usage limit ("try again at 2:45 PM"). Each retry used a slot and time. | Rate limits are not remembered across tasks. | §5.2 provider health ledger |
| W6 | Image QA took up to 14 minutes per check, queued behind one or two codex text slots. | Text and vision jobs share a small pool with no priorities. | §5.3 scheduler |
| W8 | The voice seemed to wait for the images. It was made only after all 51 frames were done. | Voice and images already run in parallel (`workflow.rs`: `tokio::join!(images, editions)`); voice depends only on the script and the storyboard's delivery cues, not on images. But the Hindi check failed in the first minutes, and `join!` reported it only after the images branch finished hours later. Each resume repeated this, so the voice only ran once images were complete. | §5.1 report failures as they happen |
| W7 | A failed character-sheet check was cached. Resuming replayed the same failure; only a code change made it retry. | Failures are committed like successes. | §5.1 failure model |

### 1.2 Stops and blocks

| # | What happened | Root cause | Fix |
|---|---|---|---|
| B1 | One character sheet (Meera's sari look, used in one scene) stopped the whole part: images, voice and captions. | Every failure stops the run. | §5.1 blast radius |
| B2 | The sari sheet came out in police khaki. | The base sheet was attached as a reference, and the model copied its clothes as well as the face. | Fixed in code (reference is for the face only). Keep it and add a test. |
| B3 | A row of heads was read as "printed text" and failed character sheets. | The local text detector was a hard gate. | Fixed (now a hint to vision QA). Keep it and add a test. |
| B4 | Sensitive scenes (a minor, blackmail, violence) were refused or had the connection dropped on every profile. | The narration line in the image prompt carried words the safe shot description avoided. | Partly fixed (retry without narration). §6.2 adds a safety pass at storyboard time. |
| B5 | Frames with four characters (six or more reference images) kept timing out. | No limit on reference count or prompt weight per frame. | §6.1 reference budget |
| B6 | The Hindi ending check failed a correct ending, because a call to follow came after the question. | The Hindi question pattern required `?` as the very last character. | Fixed. Add a test. |
| B7 | Narration needed a 6.6% speed-up but the limit was 6%, after the paid voice was already made. | Length is checked after the paid step, not before it. | §7.2 length budget before paid audio; limit is now 8% |
| B8 | In economy mode, a single failed image QA stopped the run, even for minor defects. | No redraw allowance in economy mode, and minor and major defects are treated the same. | §6.3 graded QA |
| B9 | Editing the storyboard by hand needed hand-made cache hashes. | There is no command to edit a shot. | §8.2 `retold shot edit` |

### 1.3 Quality

| # | What happened | Root cause | Fix |
|---|---|---|---|
| Q1 | No music in the first render. | The channel's music and sfx folders were empty, and the edit skipped them without a warning. | §9.1 audio is required for render |
| Q2 | The music sounded happy and dance-like under a crime thriller, got suddenly loud in places and weak in others. | Tracks were generated from text prompts and nobody listened. Each mood change restarted a different full-mix track from its intro, with its own builds and drops. Loudness was matched to a whole-track average, not to the section actually used. The level was raised 6 dB by editing the library's loudness values. | §9 the music system |
| Q3 | Sound effects and ambience came from a music model with no check of what they sound like. | No audio QA. | §9.4 audio QA |
| Q4 | The picture is a moving storyboard: pans and zooms on still frames, with no depth, no designed titles and no rhythm to the cuts. | The edit uses six camera moves and four transitions from a catalogue of 130 effects and 112 transitions. | §10 picture |
| Q5 | I could not hear or watch the result before saying it was done. | There is no review step between render and "done" that checks audio and picture. | §11 delivery review |

### 1.4 Process (my mistakes)

- **P1.** I generated the music library without telling you it hadn't been listened to. I raised its level by editing library metadata, and called the video "world class" before anyone had checked the audio. Not good enough.
- **P2.** When you said "fair use": fair use does not cover background music in a video. Copyrighted film or label music will get a Content ID claim or strike. The right source is music that is **licensed for reuse**: YouTube Audio Library, Pixabay Music, Free Music Archive tracks with CC-BY or CC0, Incompetech (CC-BY) or a paid library (Epidemic Sound, Artlist). Section 9.2 makes licensed library music the default and generated music the fallback.

---

The production-stage contract `production-stage-contract.md` further defines captured multi-source
evidence and event/knowledge/reveal constraints at every step; no plausible false co-presence may
pass by matching a wrong storyboard. It includes a version-scoped Drishyam regression.

## 2. Goals and measures

1. **No duplicate spending.** A finished step never runs again unless its real inputs changed or you ask. Measure: re-running a completed series makes zero provider calls.
2. **No surprises.** Before anything runs, Retold shows what will run, why and what it costs.
3. **Failures stay local.** One bad asset blocks only the work that needs it.
4. **Film-grade output.** A score that follows the story, a mixed and mastered soundtrack, depth and motion in the picture, designed titles and checked delivery.
5. **A series is one job.** Shared work happens once; parts then run independently and in parallel.

---

## 3. Series system (replaces most of `canon.rs`)

### 3.1 One command, one object
`retold series new <work> --parts all` creates a **series** folder with:
```
series/<work>/
  series.json        # parts, order, spoiler policy, languages, budget per part
  bible/             # research per part, story-so-far per part (frozen once approved)
  plan.json          # the series plan (3.2)
  cast/ places/ props/ style/   # shared assets, each versioned
  parts/P01 P02 P03  # ordinary runs that reference series assets by version
```
Parts are normal runs. They read series assets by **version id**, never by "latest".

### 3.2 The series plan (fixed up front, approved once)
One plan step, made once, gives for every part:
- Its opening hook type and first line idea (no part ever reads another part's script).
- Its story so far (what earlier parts established), spoiler list and returning cast states.
- **Textual identity/look-state registry** across approved scope (base and per-part states),
  planned once. Generate only look references consumed by approved shots, each pinned to event
  intervals; unused optional states cost zero sheets. Cast references do not establish presence.
- Its musical identity: a main theme mood and per-section cues (§9.3), so all parts sound like
  one series.
- Its sensitive content map: scenes that need safe staging (§6.2).

After approval, parts run with no runtime dependency on each other.

### 3.3 Shared assets as a versioned registry
Characters, looks, places, props, style reference and audio cues are entries:
`{id, version, inputs_hash, status: draft|approved|stale, file, used_by: [part/unit]}`.
Each part pins the versions it used. Changing an asset makes a new immutable version. Registry publication, shared-generation
deduplication, snapshot pinning and lock ordering follow CLI plan §19.3.1 in both foreground
and studio modes. `used_by` is only a rebuildable reverse index of pinned run manifests.
Parts that used
the old one are marked **stale** and never regenerated automatically (§4.3).

### 3.4 Running parts
A single series scheduler runs all parts together over shared provider pools (§5.3). Part 2 can
draw images while part 1 records its voice. No hand-run loops.

---

## 4. Execution model: never pay twice

### 4.1 Declared dependencies
Every step declares typed inputs (artifact ids and versions, guide fields it reads, a template
version). The step key is a hash of those inputs only. Reading anything else (another part's
file, "latest" state) fails a test.

### 4.2 Semantic keys and template versions
- Prompt templates carry an explicit `template_version` and actual rendered hash/provenance.
  Editorial edits preserve pinned approved work; undeclared producer drift blocks fresh execution
  until explicitly versioned/adopted. A scoped plan shows affected steps; never relabel old provenance.
- Guide fields are tagged with the stages they affect (e.g. `edit.music.*` affects only
  assembly). Adopting a guide re-runs only the affected stages. `--upgrade-guide` no longer
  bumps research.
- Migration follows CLI plan §16: preserve verified legacy artifacts and request/charge IDs
  through versioned mappings; never guess missing provenance or regenerate during migration.

### 4.3 Locks, stale marking and explicit redo
- Approving a script, storyboard, sheet or frame atomically **locks** its hash and input versions.
  `lock` is an alias for approval; `unlock` revokes approval without regenerating. See CLI §19.4.
- When an upstream input changes, locked items become **stale** with a reason ("cast Meera v3
  → v4"). They are only redone after an explicit targeted redo plan. Keeping pinned versions records
  acknowledgement; accepting a stale list must explicitly choose keep or redo, never unlock silently.
- Unlocked items follow upstream changes as today.

### 4.4 `retold plan <run|series>`
Before any run, the CLI prints every step that would execute, the reason (new, changed input
X, stale, failed before), the provider it would use and the expected time and cost. `video continue`
prints the summary. Every invocation defaults to `--spend 0`; confirmation and `--yes` never
grant spending/export authority. All calls use shared reservations before dispatch (CLI §19).

---

## 5. Failures, providers and scheduling

### 5.1 Failure model
- **Separate execution failure from checker verdict.** Successfully obtained defect verdicts
  are immutable cached evidence tied to content/rubric/inputs; unchanged resume reuses their
  blocking decision without calling QA again. Transport failures never become passed outputs.
  Keep rejected generations for inspection without activating them (resource contract §6).
- **Bounded repair with feedback.** All retries/candidates/judges/fallbacks share one reviewed
  operation tree. Default one candidate and at most one targeted repair; never retry an ambiguous
  accepted subscription/paid call automatically. Resource contract §3–4 governs exhaustion/recovery.
- **Limited impact.** A failure blocks only the work that depends on it. Example: a failed look
  sheet blocks the frames that use that look; other frames, voice and captions continue. The
  run ends with a list of blocked items and the exact command to fix each.
- **Report failures as they happen.** Independent branches (images; language → voice →
  captions) run in parallel, and a failure in one is shown the moment it happens, with its
  fix command, while the others continue. The fixed branch can be resumed through a control request to the existing execution owner
  while the other runs; no second writer is admitted (CLI §19.3).
- **Voice early.** Voice starts as soon as the script, the storyboard's delivery cues and the
  translation checks are ready. Paid voice runs after the script is locked, so it is never
  thrown away by a later script change.
- **Kinds of failure:** confirmed not-dispatched transient → bounded backoff; ambiguous consumed
  outcome → recover/block; refusal → safe compliant restaging only if explicitly in the bounded
  repair tree; quota → park canonical account (§5.2); quality → bounded graded repair (§6.3).
  Account/profile switching never bypasses provider safeguards or exhausted usage authority.

### 5.2 Provider health ledger
Each canonical quota account's state is shared across profiles/workspaces: `ok | rate_limited(until) | failing(n)`. A profile
limited "until 2:45 PM" is skipped until then, with no retries. `retold doctor` shows it.

### 5.3 Scheduler
- Separate image/vision/text pools sit beneath aggregate canonical-account and host/device
  ceilings; all-or-none physical admission, orphan recovery and fair lazy scheduling follow
  runtime contract §3–§4. Foreground and studio join the same bounded ticket arbitration.
- Priorities: work that unblocks others (sheets, storyboards) first; QA before more drawing.
- Quality-qualified local image fallback (§6.4) is optional; select routes by repair-inclusive
  resource cost and task quality evidence, never by subscription/local/paid labels alone. Stable
  storyboard/visual pins gate full image generation; script/cues gate voice (resource §4).

---

## 6. Images

### 6.1 Reference budget
Reference counts/bytes/pixels are adapter-qualified limits, not limits on who actually participated.
Use only relevant approved look/prop states. If a truthful group exceeds adapter capacity, split
faithful coverage, use a qualified composite or explicitly choose another allowed route; otherwise
block. Never move/remove a plot-essential participant or add an observer just to fit references.
One event/time/place contract per literal shot; mentioned people are not automatically on-screen
(production stage contract §5). Preflight this before drawing.

### 6.2 Safety at storyboard time
The series plan's sensitive content map (§3.2) asks the director to stage those beats safely:
implied rather than shown, and no minors in harmful contexts. Before drawing, a cheap check of
each frame prompt flags wording likely to be refused. Raw narration is never appended to an
image prompt. Compile the approved event/shot facts, required actors/action/object/state and
co-presence/witness constraints into the visual brief; removing prose must retain needed evidence.

### 6.3 Graded QA
QA separates hard requirements from user/guide-defined stylistic defects (`minor | major`).
A minor style note may be retained under the chosen policy; factual/identity/safety/required-media
defects never become acceptable because a mode is cheaper. One targeted redraw and the full conservative checker-impact validation closure consume
the shared operation envelope (resource contract §6.1). New image bytes require all image-content
hard gates again, including identity; only provably unchanged checker inputs reuse prior verdicts. Exhaustion blocks dependent work and lists that
frame for review; other branches continue. Cache verdicts by exact artifact/rubric/event-constraint version. Production stage §6 requires
actual-pixel observed claims against independent canonical evidence, not prompt-match alone.

### 6.4 Local FLUX fallback (optional)
Local FLUX is an optional candidate route, not assumed equivalent or free. Qualify the actual
model/settings on character/layout/fidelity fixtures, include local compute and repair cost, and
use only within the user's finite policy. Unqualified output cannot automatically replace a proven
route. No new local provider is required for CLI cutover.

---

## 7. Language and voice

### 7.1 Transcreation checks
- Separate **facts** (hard-blocking) from **style** (user/guide rubric). Cache decisions and use
  at most the shared planned repair. A checker suggestion does not approve its own edited text;
  preserve explicit current-version review before downstream expensive media.
- Language gate patterns get tests per language (the Hindi question pattern).

### 7.2 Length before paid audio
Estimate a calibrated duration interval using relevant voice/language history; unknown history
stays uncertain. Revise approved text before synthesis or explicitly authorize a bounded uncertain
fit; no hidden calibration call or promise of never resynthesizing. Pin contextual sentence-group
identities, reuse qualified alignment and enforce existing pronunciation/speaker floors; all
synthesis/STT/fallback/repair requests share the same resource envelope (resource contract §7).

### 7.3 Captions
Derive display captions locally from approved text and qualified measured speech alignment, with
explicit many-to-one/one-to-many transliteration mappings. Protected names/negations/numbers/
qualifiers cannot change via dictionary correction. Keep ≥95% pronunciation and require no
unresolved critical meaning discrepancy even if aggregate accuracy passes (production stage §7).

---

## 8. Review and control

### 8.1 One review page per series
Shows script, storyboard, sheets, frames, score cues and the rendered video, with
approve/lock, redo and note controls per item.

### 8.2 Editing commands
- `retold video edit shot <run> <unit>`: change action, characters or staging. The storyboard step is
  marked edited (not regenerated); only that frame and true descendants (assembly) invalidate.
  Generation follows a reviewed plan and fresh authorization; edits never dispatch implicitly.
- `retold video edit line <run> <unit> --lang hi`: change one narration line; re-voices only that
  sentence group plus measured timing descendants for spoken-form changes. Narrative meaning
  changes also stale mapped translated/visual/cue descendants (runtime contract §5); images
  are reused only when their semantic dependencies are unchanged. Edit commits dispatch nothing.

---

## 9. Music and sound (rebuilt)

### 9.1 Audio is required
A render without a music bed, ambience and transition sounds is an error unless the guide says
`music = "none"`. `retold doctor` checks the channel library.

### 9.2 Licensed library first, generated second
- **Library:** curated folders of licensed tracks, each with a `license.toml` record (source, URL,
  licence, attribution text). Supported sources: YouTube Audio Library (download by hand), Pixabay
  Music and Free Music Archive (CC0/CC-BY, with attribution added to the post text),
  Incompetech (CC-BY), or a paid library folder.
- **Tagging:** each track is analysed on import: tempo, key, loudness curve, energy per
  section, and **mood tags from a local audio-text model** (CLAP) checked against wanted moods
  (tense, dark, mysterious, warm, sad, triumphant). Tracks tagged "happy" or "dance" are rejected
  for thriller cues.
- **Generated (fallback):** Lyria or ElevenLabs Music only for short cues the library lacks.
  Each one goes through the same analysis and is rejected if its mood or energy is wrong.

### 9.3 Scoring like a film, not a playlist
- **Cue sheet:** storyboard intent pins stable section/segment/word anchors, offsets and
  intensity curves; runtime contract §5.2 resolves final absolute times per edition from its
  measured qualified alignment. Pre-TTS timestamps are estimates, never final edit authority.
- **Fewer, longer cues:** one theme family per video (variations of a single track or
  stems), so moods change by arrangement, not by jumping between unrelated songs.
- **Musical edit points:** narration/word integrity comes first; optional beat snapping stays
  within permitted anchor ranges. Crossfades must fit actual cue handles/durations; no universal
  2–4-second rule. Invalid/orphan/short cues clamp only under explicit policy or fail for editing. Never start at a track's intro mid-video unless the cue starts there.
- **Choose the section, not the track:** take the part of a track whose energy matches the cue
  (calm passage under dialogue-heavy scenes, build into the climax).
- **Stingers and silences:** on reveals, the bed dips for one beat before the sting. Silences
  fade out over half a second rather than cutting.

### 9.4 Mixing and mastering
- Per-clip loudness matched to the **segment used** (short-term LUFS), not the track average.
- Sidechain-style ducking: smooth 300 ms attack and 800 ms release, −10 to −12 dB under voice.
- Bus processing with FilmCraft audio effects: dialogue (`speech_enhance`, `deesser`,
  `single_band_compressor`), music (`parametric_eq` notch at 1–4 kHz under voice), master
  (`multiband_compressor`, `hard_limiter`, `loudness_radar` at −14 LUFS, true peak −1 dBTP).
- **Audio QA after render:** measure short-term loudness of the music-only stem across the video.
  Fail if it jumps more than 4 LU between neighbouring 3-second windows without a planned cue
  change. Also check speech-to-music difference (at least 12 LU) and run mood checks on the
  final music stem.

### 9.5 Ambience and effects
Licensed sound-effect packs (CC0 sources such as Freesound CC0 or Sonniss GDC bundles) first,
with tags. Generated sounds only after the audio-text check confirms they are not music.

---

## 10. Picture

Using what FilmCraft already supports (`docs/filmcraft-effects.json`):

1. **Depth:** run the local depth model (`models/depth`, already installed) and the existing
   `retold-parallax` crate to give 2.5D parallax on wide and medium shots. Enable it in the
   guide only through the semantic keys of §4.2, so turning it on never re-runs the storyboard.
2. **Shot rhythm:** cut lengths follow narration phrases and the music's bars, with camera moves
   chosen by shot type (push-in on close-ups at reveals, slow drift on wides).
3. **Titles:** an animated title card (`text_animator`/`typewriter`, `light_sweep`), chapter
   cards between sections (part number, section name), and lower thirds for names on first
   appearance.
4. **Grade and texture:** one LUT-like `lumetri` grade per series with per-section variations
   (warm day, cold night, desaturated climax), `noise` grain, `vignette`, a soft
   `volumetric_rays`/`lens_flare` only on chosen hero shots.
5. **Transitions:** chosen by story beat, not section alone: `dip_to_black` before reveals,
   `film_dissolve` for time jumps, `whip`/`cross_zoom` in fast sequences, `light_leak` for
   memory or flashback.
6. **Formats:** 16:9 master and a 9:16 version from the same edit. Captions and safe zones are
   set per format.

---

## 11. Delivery review (before "done")

Live interactive HTML uses the runtime contract §7 loopback/session/CSRF/content/path boundary;
offline bundles stay inert. Browser review never grants money/quota or export authority.

After render, Retold makes a **review pack**: a contact sheet of every shot, a loudness graph of
voice, music and master over time, the cue sheet and a 60-second highlight. "Done" requires
the automatic audio and picture checks to pass and the person to confirm final viewing for
that exact release/hash. Product spec §4.5 defines separate export/QA/human-review states,
append-only review records and the explicit CLI/TUI confirmation. A review copy may be packaged
with an unreviewed/changes-requested label; it cannot be called Ready to deliver. New releases
never inherit a previous release's viewing confirmation.

---

## 12. Build order

| Phase | Work | Why first |
|---|---|---|
| 1 | Failure model (§5.1), provider health ledger (§5.2), `retold plan` (§4.4), tests for B2/B3/B6 fixes | Stops waste immediately; small |
| 2 | Semantic keys, template versions, scoped guide fields, re-keying migration (§4.2) | Ends cascades (W1–W3) |
| 3 | Locks and stale marking (§4.3), `shot edit` and `line edit` (§8.2) | Protects reviewed work |
| 4 | Music system: licensed library import, analysis, mood check, cue sheet, mixing, audio QA (§9) | Biggest quality gap you heard |
| 5 | Series system: series plan, registry, scheduler (§3, §5.3) | Removes canon's design problems |
| 6 | Images: reference budget, safety staging, graded QA (§6.1–6.3) | Fewer stops and refusals |
| 7 | Picture: depth, titles, grade, transitions by beat, 16:9 master (§10) | Film look |
| 8 | Language: fact/style split, length before TTS, caption spelling (§7) | Fewer paid retries |
| 9 | Review page per series and delivery review pack (§8.1, §11) | Close the loop |
| 10 | Optional local FLUX fallback (§6.4) | Resilience |

## 13. Acceptance tests (each one is a recorded failure from this run)

1. Scripting part 3 does not change any part 1 or part 2 step key.
2. Editorial prompt edits do not regenerate pinned approved artifacts; unversioned producer drift
   blocks fresh execution until explicit version adoption, without rewriting historical provenance.
3. Changing `edit.music.bed_lufs` reruns assembly only.
4. An unchanged defect verdict is reused from cache and remains blocking with zero QA calls;
   a transport failure is never replayed as a passed result.
5. A failed look sheet blocks only frames using that look; voice and captions finish.
6. A provider limited until a given time receives no requests before that time.
7. A four-person event under a low-reference adapter gets faithful coverage/composite/route
   review before media; no plot-essential participant/witness relationship changes to fit the limit.
8. A refused frame is retried once from its shot description alone; if that is refused too, it
   is listed for review and does not stop the run.
9. Hindi ending: "…सही था?\nफ़ॉलो करें।" passes the question gate.
10. The music-stem loudness check fails a mix with a jump over 4 LU, and the mood check fails
    a "happy/dance" track under a "tense" cue.
11. Re-running a finished series makes zero provider calls.
12. A render with an empty music library fails with a clear message instead of a silent edit.

## 14. Fixes already in the working tree (not committed)

- Character look sheets: the base sheet is a face reference only (`assets.rs`).
- Local text detector is a hint to vision QA, not a fail (`assets.rs`).
- Image fallback without narration on refusal or a dropped connection, for new frames and repairs (`assets.rs`).
- Canon openings read only earlier parts (`canon.rs`).
- Fact-check gets canon context; quoted evidence that skips parts of the source with "[...]" is checked piece by piece (`pipeline.rs`, `story.rs`).
- Disabled parallax moves are dropped before direction gates (`story.rs`, `pipeline.rs`).
- Hindi/Urdu question gate allows a short line after the question (`languages.rs`).
- Narration tempo limit widened to ±8% (`audio_fit.rs`).

These are stopgaps. Phases 1–6 replace most of them with the designs above.


## 15. Required resource/quality optimization

`cost-quality-contract.md` is normative for all subscription/paid/local work and extends these
engine phases through its §10 matrix. Default candidate counts are billing-independent; no judge
for one candidate, no nested unbounded repair and no blind ambiguous subscription retry. Stable
approved branch inputs gate costly generation; cross-format reuse keeps true format-specific QA.
The <1% preventable-waste objective is measured per unit with unknowns disclosed, not guaranteed
from opaque subscription metadata. Hard quality floors and human delivery review remain mandatory.


## 16. Evidence-to-image/audio/caption fidelity

`production-stage-contract.md` is normative: exact version/source pinning, deterministic bounded
web/API collection and source-lineage dedup, material event/knowledge/reveal records, script
clause entailment, coherent shot facts, used cast states and actual-pixel canonical-constraint QA.
Critical meaning checks complement pronunciation; ready requires compatible current cross-modal
evidence and cached gates, not another routine whole-film AI judge. Source absence means unknown,
not invented certainty that nobody witnessed an event. Factual illustration never invents a police
eyewitness by merging separate car-disposal, body-concealment and investigation events.


## 17. Algorithmic quality per resource

`algorithm-strategy.md` defines conservative existing-algorithm extensions: preserve contradictory
near-duplicates, retrieve scoped entity/event evidence, pack required support/counterevidence, stop
gap retrieval at finite bounds, reuse exact dependency slices and constrain scenes/timing locally
before inference. Optional rank fusion/embedding/diversity policies must earn per-task adoption
through held-out comparisons; no automatic model swarm/training/candidate race is introduced.
A clever retrieval score is never a factual/visual/voice quality pass. Existing floors/review gates
and current-version readiness still decide what is usable.
