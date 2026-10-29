# Running and Recovering Retold

`ready` is the default stop endpoint: it generates scene illustrations, speech audio, aligned captions, platform posting text, an offline review page, pure timeline JSON, and a delivery manifest. It opens no editor and exports no video until explicitly authorized.

```sh
retold make explainer \
  --channel studio-prime \
  --topic "Rust Ownership" \
  --source-file facts.txt \
  --lang en:en \
  --lang es:es,en.srt \
  --budget 2.00 \
  --allow-paid \
  --yes

retold resume rust-ownership --until ready
retold review rust-ownership
retold redo rust-ownership images 4,14 --until ready
retold add-language rust-ownership hi:hi --until ready
retold status rust-ownership --json
retold costs rust-ownership
```

### Authorization Flags

- `--yes`: Approves review pauses non-interactively (e.g. character sheet sign-off).
- `--allow-paid`: Permits paid API requests within the cumulative run budget.
- `--allow-video`: Permits creating a FilmCraft editing project and exporting final video files.

Explicit permissions persist in `.retold/approvals.json`; subsequent resumptions retain granted authorizations. Passing `--allow-video` without `--until` runs through export. A later resume without the flag stops at `ready`, even if video permission was granted before.

---

## Library and File Locations

Always host the production library on durable storage that survives system reboots. macOS empties temporary directories on restart.

Configure your storage location in `settings.toml`:

```toml
library = "~/Retold"
```

Retold will warn during `doctor` and `make` if your library path is detected under a temporary directory.

---

## Voice Timing and Audio Calibration

TTS models vary in speech velocity:
- Inline direction tags are supported for speech synthesis engines that accept them.
- Separate delivery instructions can be configured per language on the channel:
  ```toml
  [voices.en]
  provider = "openrouter"
  model = "google/gemini-2.0-flash"
  voice = "Puck"
  instructions = "Narrate slowly and clearly, with an instructive and warm cadence."
  ```

Retold's audio fitting pipeline ensures narration duration plus ending holds matches the target duration window specified in the guide's `[format]` section. Minor tempo adjustments (within 0.94–1.06) are applied when necessary to align pacing cleanly without audible pitch distortion.

---

## Spend Management and Ledger Holds

Retold tracks all billable requests in a durable, append-only ledger (`.retold/ledger.json`):

1. **Pre-dispatch reservation:** Before calling a paid API endpoint, a conservative price ceiling quote is reserved against the run's budget.
2. **Atomic response capture:** The output media and API response metadata are persisted to disk.
3. **Settlement:** The ledger records the actual charge reported by the provider and releases any reserved surplus.
4. **Crash safety:** If a network error, timeout, or crash occurs, the reservation is retained. Resuming the run checks for existing committed assets before attempting any retry.

### Auditing and Manual Reconciliation

```sh
# View cost breakdown, active holds, and receipts:
retold costs <run-id>

# Reconcile an ambiguous hold after verifying billing status with provider:
retold costs <run-id> --request <request-id> --cost 0.045

# Or release an unbilled request:
retold costs <run-id> --request <request-id> --cost 0.000
```

---

## Selective Regeneration (`retold redo`)

If an individual illustration frame or stage requires revision:

```sh
retold redo <run-id> images 3,7 --until ready
```

Retold invalidates only the selected items and their downstream dependents (such as visual QA checks and timeline assembly), preserving all other completed artwork, narration, and transcription.

---

## FilmCraft Editor Ownership and Video Export

When `--allow-video` is supplied:
1. Retold connects to FilmCraft over a local socket protocol.
2. It constructs an isolated, non-interactive project session.
3. Scene clips, audio ducking envelopes, motion transitions, title cards, and styled subtitle layers are compiled into timeline tracks.
4. Loudness is normalized to the target LUFS standard (e.g. -14 LUFS for online video).
5. The finished video file and thumbnail candidates are rendered and written to the deliverables folder.

## Local models and shot intermediates

Model assets live in `~/Library/Application Support/retold/models/<name>/<version>/` on macOS.
`RETOLD_MODELS_DIR` overrides the root, including for isolated offline tests.

```sh
retold models list --json
retold models install depth
retold models install voice
retold models install voice-lite
retold models install whisper
retold models install text-detect    # 2.4 MB; free check for printed text in generated images
retold models install person-detect  # 83 MB; free check that location/prop references show no people
retold models install upscale        # 8 MB; 2x super-resolution for panels and recut shots
retold models verify                 # installed bundles; no network
retold models verify depth
retold models worker-health          # installed Chatterbox environment; no synthesis
retold models remove voice-lite
```

`text-detect` (PaddleOCR PP-OCRv3 detector) and `person-detect` (RT-DETR r18) are optional:
when installed, every generated image is checked locally for printed text before any vision
model is asked, and location and prop references are checked for people. The text check only
counts confident, wide text-line shapes (no false hits on folds, buttons or hatching). The
person detector misses many stylised figures, so it is never used to count characters in
frames, only to catch people in references that must have none.

The embedded catalogue pins upstream revisions, byte sizes, SHA-256 hashes and licences.
Downloads retain `.download` files across interruption and resume with HTTP Range. Servers that
ignore Range cause a safe restart. A checksum mismatch discards the partial download; running
`install` again also repairs corrupt completed files. Activation is an atomic `active.json` commit,
after every asset and the voice environment are ready. Install/remove use an OS lock. A partially
installed directory is never reported ready. `verify` hashes all assets; listing may therefore take
time for the multi-gigabyte voice bundle. Inference performs no implicit model download.

The voice installer needs `uv` on PATH (`brew install uv`), or an explicit `RETOLD_UV` executable.
It manages Python 3.11 and the pinned, hash-locked Chatterbox dependencies; no manual Python setup is
needed. The worker communicates with versioned JSON lines, keeps model state between requests,
restarts once after a crash, closes after idle time and kills its entire process group on cancellation
or drop. Worker stderr is drained; protocol errors and timeouts are surfaced to the caller.

At the top level of a guide, `local_models = ["depth"]` declares local prerequisites. `retold check`
verifies these offline and prints the exact install command for missing or damaged assets.
`doctor` includes model licence/integrity rows and checks the actual ffmpeg version.

To exercise an ONNX model directly, pass an array of named tensors. `dtype` is `f32` or `i64`;
`shape` and `values` must match the installed model's input contract:

```json
[{"name":"pixel_values","shape":[1,3,518,518],"dtype":"f32","values":[]}]
```

The empty array above is schematic: supply all 804972 image values for that shape.
`retold models infer depth --input tensors.json --cpu` selects CPU; omitting `--cpu` tries CoreML
with CPU fallback. Sessions are cached by model hash and backend. Work runs off the async executor,
and cancellation terminates the active ONNX request.

### Register and render a shot

`retold render-shot <run> --spec shot.json --item 1` renders a real ProRes 4444 intermediate and
registers the specification for that one-based scene. A specification contains:

```json
{
  "image":"/absolute/path/frame.png",
  "depth":null,
  "frames":30,
  "fps":30,
  "motion_x":12.0,
  "motion_y":0.0,
  "sprites":[]
}
```

Optional depth is an image with the same dimensions, white near/black far. Each sprite contains
`image`, integer `x`/`y`, `first_frame` and exclusive `end_frame`. Motion is total pixel displacement.
The compositor preserves alpha and uses premultiplied bilinear sampling. RGBA frames stream through
a bounded two-frame queue to ffmpeg; clips and frame hashes activate only after successful encoding.
`RETOLD_FFMPEG` overrides PATH. Identical content reuses the clip; a missing/corrupt clip is repaired.

`retold resume <run> --until ready` rebuilds registered shots against the current scene images and
per-language narration timing, then places their media paths in the timeline. It retains FilmCraft
for grading, captions, music, loudness and final export. `redo <run> shots 1` bumps only the selected
shot's revision; unchanged scene and voice provider work is reused. Sprite intervals must fit the
new duration; adjust the specification if an edition becomes shorter.

## Depth-driven parallax

Install and verify the pinned small depth model with `retold models install depth` and
`retold models verify depth`. Install ffmpeg or point `RETOLD_FFMPEG` at its executable.
Enable parallax in your own guide:

```toml
[visuals.parallax]
enabled = true
max_share = 0.35
strength = 0.35
hero_fill = false
```

Both numeric limits are finite values between zero and one. Parallax is disabled by default.
The director can select `motion = "parallax"` and `parallax_move = "push_in"`, `"orbit"`,
`"pedestal"` or `"dolly_zoom"`, on at most `floor(max_share * shot_count)` shots. The default
move is push-in. Unselected shots retain the usual FilmCraft camera animation. Explicit
`render-shot` specifications take precedence. Zero strength skips local parallax rendering.
`retold check --guide <id> ...` checks model integrity and ffmpeg without inference, provider
calls, editor access or downloads. Runs retain their frozen guide; guide upgrades use the
existing explicit `resume --upgrade-guide` flow.

Depth uses the pinned Depth-Anything-V2-Small ONNX checkpoint, its ImageNet preprocessing,
aspect-preserving bicubic resize and normalized near/far map. CPU is the measured predictable
backend on this Mac. Depth cache keys include source bytes, model identity and preprocessing
version. Clip keys include depth, background, camera, strength, dimensions and edition timing.
Resume verifies cached hashes and repairs corrupt entries. `retold redo <run> depth 1` or
`retold redo <run> shots 1` invalidates the selected one-based item while reusing other work.
Audits live under the run's `work/parallax-shots/`; `retold bench <run> --json` reports depth
and shot timings. Render failures retain the run's ledger and log for normal resume.

Every output frame is audited before activation: revealed hole area must be at most 8%;
at most 1% of adjacent source edges may stretch beyond 1.8 times their source spacing;
no holes may remain after edge-aware filling. A rejected motion retries at half strength,
then becomes flat. Flat results leave the native FilmCraft camera move active. Successfully
baked clips receive no second camera animation or Lean-mode shot split. Per-key locks and
atomic activation preserve finished shots across interruption.

`hero_fill = true` requires enabled parallax, an image route, `--allow-paid` and sufficient
budget. Only director-selected hero shots request a masked background plate. The existing
media runtime quotes, reserves and settles the call; identical resumes reuse its receipt.
Only masked pixels enter the plate. Leave this setting false for local-only hole filling.

Parallax framing adds one constant margin across each move, capped at 20%, to keep
revealed source boundaries outside the picture. Local reconstruction fills only narrow
gaps: at most 0.4% of the shorter side, with a two-pixel allowance on small fixtures.
Wider reveals remain unresolved and trigger gentler/flat recovery. The audit records
the required framing scale, border holes before filling and maximum attempted fill radius;
isolated border splat gaps may still receive bounded reconstruction.

## Local voices and dialogue

Install the pinned local bundles with `retold models install voice`,
`retold models install voice-lite`, and `retold models install whisper`.
For a full multilingual ASR check, `retold models install whisper-large` supplies
pinned Whisper large-v3 weights; point the `local_whisper.model` configuration at
the returned model file. Local ASR cache identities include its path and file metadata,
so changing weights rechecks QA while preserving completed speech.
The dedicated Hindi `chatterbox-hi` route also requires `retold models install voice-hi`;
it uses pinned MIT Hindi T3 and V3 decoder weights with the same managed worker environment.
For Hindi and Urdu, the preferred route is `indicf5` (`retold models install voice-indic`):
pinned MIT IndicF5 weights in their own managed worker environment. It clones the voice
sample conditioned on its Whisper transcript (cached by content), so samples need
clear speech. Clone similarity is still measured with the `voice` bundle. When an
IndicF5 turn fails QA, the same sample is retried on `chatterbox-hi` if `voice-hi`
is installed, and then on the nearest Kokoro preset. Weights download from the ungated MIT mirror
`mshdev/IndicF5-retold` (unmodified ai4bharat/IndicF5, pinned by SHA-256), so no
Hugging Face login is needed.
Install `espeak-ng` and ffmpeg on PATH. Configure the local speech provider and
an existing `local_whisper` provider/Transcribe route in `providers.toml`:

```toml
[providers.local]
kind = "local_voice"
espeak = "espeak-ng"
max_parallel = 1
timeout_secs = 1800
```

Set `mode = "dialogue"` in the guide's existing `[narration]` table. The writer
uses `NARRATOR [neutral]: words` or an exact planned character identifier; unknown
speakers and emotions fail before synthesis. Supported emotions are neutral,
furious, terrified, whispering, solemn, joyful, sarcastic and melancholy.

In the channel's existing voice configuration:

```toml
[voices]
en = { provider = "local", model = "chatterbox", voice = "narrator", sample = "voice-sample.wav" }
hi = { provider = "local", model = "indicf5", voice = "narrator", sample = "voice-sample.wav" }
ur = { provider = "local", model = "indicf5", voice = "narrator", sample = "voice-sample.wav" }

[character_voices]
Alice = { provider = "local", model = "kokoro", voice = "af_heart" }
Bob = { provider = "local", model = "kokoro", voice = "am_adam" }
```

Use `hf_alpha` / `hm_omega` for Hindi preset characters. Overrides are shared
across editions; use Chatterbox samples for the same character across languages.
Samples must be channel-relative PCM16/float WAV, 10–30 seconds long, audible and
not severely clipped. `sample_language = "en"` identifies the reference language
(default: the guide source language). Cross-language clones use zero CFG guidance
to reduce accent transfer, following Chatterbox's upstream guidance. Unspecified Chatterbox narrator samples use
`voice-sample.wav`. Automatic character casting records distinct immutable
presets under the run layout's shared cast folder in `voices/`; explicit overrides are required after four characters.
Optional top-level `room_tone = "room.wav"` supplies the 0.18-second gaps.
Without that asset, gaps use silence.

Urdu speech uses cached Devanagari transliteration with क़ ख़ ग़ ज़ फ़ retained;
caption words remain in Urdu. The transliteration must preserve word boundaries
for caption timing. `pronunciations.<lang>` can supply pronunciation fixes.
The final audio must fit the guide's duration range. Every turn must pass 95%
word edit accuracy, including inserted and missing words; clones also require
speaker cosine similarity ≥0.70. A failed turn gets one respelling retry, then
the nearest measured preset for a clone, followed by any explicit cloud
`fallbacks = [{provider="openrouter", model="...", voice="..."}]` on its voice.
Cloud fallback still requires `--allow-paid` and sufficient budget. Ambiguous
paid failures require reconciliation and never dispatch a fallback automatically.

`work/06-voice/<lang>/turns.json` records accepted turns, original text, spoken
text, quality metrics, A1/A2 tracks and alignment. `resume` reuses completed
speech, ASR and embeddings. `retold redo <run> voices <1-based-turn>` invalidates
the selected turn; an actual duration change also rebuilds later timing.

## Lip-sync

Enable `[visuals.lipsync] enabled = true` alongside dialogue narration. Install
`espeak-ng` (or set `RETOLD_ESPEAK`) and run `retold models install phonemes`.
The pinned Apache-2.0 acoustic model and hash-locked Python worker are managed by
Retold. Installation downloads model assets; execution makes no model downloads.
`retold check` verifies both prerequisites before timeline assembly.

Character mouth/eye sheets are cached under the cast's `mouths/` directory.
Face crops, fitted sprites, phoneme records, per-frame timing and composite QA
images are under the run's `work/mouths/`. Sheet generation uses the image route;
landmark and QA passes use `roles.image_qa`. For subscription-only operation, use
subscription CLI routes and never pass `--allow-paid`.

A sheet gets one repair pass. Up to four defective cells are regenerated in
isolation; a global defect triggers one full-sheet repair. Every repaired sheet
passes the same identity/shape checks. All eight mouth cells are also supplied as close-ups
so its ink outline is distinguishable from an open mouth cavity. Uniform skin
shade is matched during fitting and checked in the final composite.

The local acoustic recognizer measures IPA phone runs from actual audio on a
20 ms grid, rather than dividing word durations uniformly. Aligned words bound
the speech region. Low-confidence or invalid timing preserves the still. Frame
sampling uses frame centres and a two-frame minimum hold; shorter phone shapes
may merge into a neighbour. Model resolution is not a guarantee of accuracy
against independent human phonetic annotations.

Sprites attach before camera/parallax transforms. Missing speakers, inconsistent
turn/word counts, hidden features, rejected sheets, rejected timing and rejected
composites retain still artwork. Cancellation, missing prerequisites, filesystem
errors and provider failures remain explicit run failures.

Native title fitting uses the configured script font, shaped width and font
ascent/descent so Nastaliq and Devanagari stay within the frame. `[title] vertical
= true` in the channel opts Japanese/Chinese titles into FilmCraft's native
vertical text. Other scripts reject that setting. The existing per-language
translation repair loop also checks idioms/cultural references using guide notes.

## Local delivery

Set `[delivery] aspects = ["9:16", "16:9", "1:1"]` in the guide. `ready` writes
reviewable aspect timelines and local metadata. Resume to `export` with the
separate `--allow-video` permission to create videos through FilmCraft's job queue.
FFmpeg is needed for aspect plates and measured -14 LUFS export calibration.

Each aspect has its own title/caption layout. Cached contain plates preserve the
whole source foreground over a soft background extension; explicit native Motion
applies the crop and zoom around the image QA focus. FilmCraft Auto Reframe
adds lateral tracking even to centered plates, so it is not stacked onto this
transform. Position follows zoom to keep the focus centered throughout. Mouths/camera motion in
existing clips stay attached. FilmCraft chapter markers follow aligned storyboard
section boundaries. Square delivery contains the whole two-person composition
without focus zoom or offset. Captions retain their original source filenames,
requested scripts and SRT/VTT formats.

Packages contain the video, three thumbnail candidates, `thumbnail-ranking.json`,
per-platform `title.*.txt`, `description.*.txt`, `tags.*.json`, chapter JSON/text,
caption sidecars, `metadata.json`, `post.md` and a hash inventory `package.json`.
Scores use legibility, subject visibility, composition and narration fidelity;
`selected` remains null until you choose a candidate for manual posting. `post.md`
is your editable checklist and is preserved on resume. There is no upload, OAuth
or automatic posting path.

Metadata is rejected rather than silently shortened: portable limits are 100
UTF-16 units for titles, 2200 units for descriptions including hashtags, 5000
UTF-8 bytes, and five distinct hashtags. These are conservative packaging bounds,
not a promise that every platform feature is available. Check chapter eligibility,
rights/disclosures and current platform requirements before manual posting.
YouTube's API separately documents [title/description byte limits](https://developers.google.com/youtube/v3/docs/videos).

## Local guide assessment

Assess explicitly selected saved run directories using their own guide/channel snapshots:

```sh
retold guides assess /path/to/run-a /path/to/run-b
retold guides assess /path/to/run-a /path/to/run-b --json > guide-assessment.json
```

This command reads local evidence only. It does not call providers, generate media,
write a report into a run, change a guide, reconcile state or recover a failed run.
The shell redirect in the second example saves the report where you choose.

Runs must be idle with no recorded active task or error. A shared read lock prevents
inspection during a write. Missing locks, invalid metadata/snapshots or malformed evidence
fail with the source path; missing optional evidence is reported as unknown. A run with
only some stage evidence remains a partial assessment, not proof of a complete video.

The report includes run identities, guide version/hash, channel snapshot hash, quality,
shot/edition identifiers and evidence paths. Current shot audits must match the state
record's output hash, include bounded-fill metrics and pass QA to support a motion proposal.
`verified` describes that audit provenance/QA; it does not reverify or decode video bytes.
Stale/legacy audits remain visible and cannot justify a proposal.

A candidate motion-strength diff requires at least three distinct run inputs with the same
exact guide snapshot, channel snapshot, quality and edition. Each contributing run must
have at least two audited shots, at least half using recovery, and a successful gentler
move. Affected inputs must account for at least half of all compared inputs in that group.
Repeated runs of the same inputs count once; copied run identities are rejected. These
are conservative screening rules, not proof that a lower strength improves every scene.
The full candidate guide is validated before displaying the diff. Inline-table manifests
get observations without an automatically guessed text patch.

Writing checks are historical repair observations and never weaken a gate. Narration timing
uses saved editions, the language's target (or format fallback), and the ending hold; this
is post-fit timing and cannot diagnose original speaking pace by itself. No candidate diff
is generated for either category. Voice casting, pronunciations and renderer defects need
changes in their own responsible layer. Review any candidate on representative scenes and
explicitly create a new guide version before adopting it. Existing run snapshots stay fixed.
There is no automatic apply mode, YouTube sign-in or audience analytics in this command.

## Recovery soak

The offline recovery soak runs separately from ordinary unit/integration checks and has its own
CI step. It uses the production research→script→images→dialogue→captions→depth→mouths→shots→
Ready workflow with deterministic test providers, recorded depth and fixture phoneme windows.
Every one of ten runs contains English, Hindi and Urdu, real ffmpeg intermediates, a seeded
process kill, restart from disk and a second warm resume. Completed cache bytes, finished
clip hashes/timestamps, final state, zero-spend ledgers and warm provider counts are checked.

```sh
RETOLD_S8_OUTPUT=/absolute/path/to/fresh-offline-output \
  cargo test -p retold-engine --features acceptance --test s8_soak -- --nocapture
```

The seed is `0x5838`. Cuts are shuffled named stage completion events, including the window
between immutable cache commit and state-index flush. This is reproducible process-crash
coverage, not a claim of exhaustive arbitrary-timing fault injection. Existing tests separately
exercise in-flight encoder kills, cancellation, corrupt-cache repair and ambiguous paid holds.
The state index may intentionally invalidate downstream entries when unfinished upstream work
resumes; the preservation invariant is immutable cache/artifact integrity, not every old index key.

## Offline posting metadata and character cards

Posting titles, descriptions and hashtags (`package` role) and character cards
(`design` role: appearance, clothes, palette and a distinguishing detail) can run
through a local GGUF text model. Chapters remain derived from the production timeline.
This does not make research, writing, illustrations, thumbnail ranking or visual QA
offline.

Model size matters for character cards. Qwen2.5-1.5B-Instruct writes valid posting
metadata but fails the card checks (it repeats names as appearances and gives every
character the same palette). Qwen2.5-7B-Instruct Q4_K_M (Apache-2.0, about 4.7 GB in two
split files; point `models` at the `-00001-of-00002.gguf` part) passes in about 30 s on
an M4 with 16 GB. Avoid Qwen2.5-3B: its licence is research-only. Cards are rejected,
with one repair attempt, when an appearance just repeats the name or every character
shares one palette.

Install a recent llama.cpp with `llama-completion` on PATH (on this Mac, `brew install
llama.cpp`), and download an instruction-tuned GGUF once. Keep the model at an absolute
path. Retold neither downloads models at inference time nor starts a model server.
The adapter passes `--offline`, constrains output with the requested JSON schema and
uses the model's chat template. It was verified with llama.cpp 0.6.0 build 11429 and
Qwen2.5-1.5B-Instruct Q4_K_M; larger/better models can use the same interface.

Merge this provider and routing entry into your existing `providers.toml`; preserve
your other providers and routes:

```toml
[providers.offline]
kind = "local_text"
# Optional: binary = "/absolute/path/to/llama-completion"
models = { metadata = "/absolute/path/to/writer.gguf" }
max_parallel = 1
timeout_secs = 600
max_tokens = 2048
context_size = 8192

[routing]
"text.local" = ["offline:metadata"]
```

In your guide's existing `[roles]` table change only the roles you want local:

```toml
package = "text.local"
design = "text.local"
```

New runs snapshot that guide. Existing runs retain their original role choices.
`text.local` accepts only locally billed providers: subscription and paid fallbacks
are rejected even with `--allow-paid`. Other roles keep their configured routing.
The adapter rejects inherited `LLAMA_ARG_HF_REPO`, `LLAMA_ARG_HF_FILE`,
`LLAMA_ARG_MODEL_URL` and `LLAMA_ARG_RPC`; unset these for offline inference.

`retold check --guide <guide> --channel <channel>` checks the binary, model file,
selector and token/context settings without invoking inference. `retold doctor`
shows configured local providers; `doctor --ping` runs a small request through every
configured text provider, including cloud providers, so it is not an offline-only probe.

The existing platform/title/hashtag checks still reject invalid metadata. The local
provider uses the shared process-group cancellation, timeout, output cap and concurrency
limits. Successful results and zero-cost receipts use the normal durable cache; warm
resume works without text providers. A changed model does not automatically replace
already accepted cached metadata. Use a new run when evaluating a changed model or guide.
Token counts are not reported by this CLI adapter; zero token fields mean unmeasured.

Reproduce the real local acceptance check without cloud calls or video export:

```sh
RETOLD_LOCAL_TEXT_MODEL=/absolute/path/to/writer.gguf \
  cargo test -p retold-engine --features acceptance --test local_packaging \
  installed_model_generates_valid_posting_metadata -- --nocapture
```

This test uses fixtures for the other production stages, real local inference for all
four platforms' metadata, the normal quality gate, and a warm pass with text providers
disabled. It does not establish multilingual metadata quality for every GGUF model.
Protocol reference: [llama.cpp completion documentation](https://github.com/ggml-org/llama.cpp/tree/master/tools/completion).

## Music and stings library

`library.music` in `channel.toml` points at a folder with a `music.toml`:

```toml
[[tracks]]
path = "beds/slow-strings.mp3"
moods = ["dark", "sad"]
lufs = -18.0

[[tracks]]
path = "beds/pulse.mp3"
moods = ["tense", "mysterious"]
lufs = -16.5

# Optional short accents for reveals.
[[stings]]
path = "stings/low-hit.wav"
moods = ["dark", "tense"]
lufs = -14.0
```

When the storyboard scores its units (mood per unit, stings on reveals, silences), the bed
follows the story: stretches of one mood of at least eight seconds each get the best matching
track, crossfading over a second; silences drop the music out; stings play slightly above the
bed. Storyboards without scores keep one bed chosen by the plan's tone.

## Scene sound library

`library.sfx` may hold an `sfx.toml` describing tagged sounds:

```toml
[[sounds]]
path = "ambience/rain.wav"
tags = ["rain", "storm"]
kind = "ambience"   # loops under every consecutive unit whose ambience matches
lufs = -24.0

[[sounds]]
path = "effects/door-creak.wav"
tags = ["door", "creak"]
kind = "effect"     # one shot at the start of the unit
lufs = -18.0
```

Direction gives each unit an ambience and an optional effect in plain words; the sound
with the most shared tag words is used, and nothing plays when no tag matches. Ambience
sits under the music bed with half-second fades; effects play a little above it. The
transition whoosh still uses the folder's first audio file when `sfx_on_transitions` is on.

## Franchises and series: research once with a canon

A guide with `[research.canon]` (the bundled `story-recap` guide) can research a whole film
franchise, TV series or book series once and reuse it for every part's video.

```sh
retold canon build story-recap --work "Fast & Furious" --kind film            # no video
retold canon build story-recap --work "You" --kind series --spinoffs          # also spin-offs
retold canon show films/fast-furious/canon      # parts, story order, researched or listed
retold canon update films/fast-furious/canon    # new film or season: only new parts researched
retold canon make films/fast-furious/canon --parts 1-3 --until storyboard
retold make story-recap --kind film --work "Fast & Furious" --part "Fast Five"   # one part
```

A canon lists every part (release order, story order, main/spin-off/special, year, season),
researches each included part with the guide's normal research, and merges the characters
into one cast with each person's status, role and look per part. It is stored beside the
work's shared cast (`<kind>s/<work>/cast/canon/`), so every part video in that folder uses
it automatically:

- **Spoiler-safe:** a part video sees only earlier parts (by release order, or story order
  with `spoilers = "story"`), each returning character's state going in, and never names a
  character or title that belongs to a later part (a repairable check).
- **Pinned:** the slice a video started with is saved in its research folder, so updating
  the canon never changes a video in progress.
- **Continuity:** later parts may open with one line on where the earlier parts left
  things, are told how other parts opened so they open differently, and their titles carry
  the work and "Part k".
- **Characters across parts:** one base design per person; when a part changes their look,
  a look sheet is drawn with the base sheet as identity reference.

Without a canon, a `story-recap` video researches its own part as before.
