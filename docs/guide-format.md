# Guide Format Specification

Schema version 1 uses strict TOML along with companion Markdown files. The authoritative JSON schema is generated directly from the Rust types:
- [`guide.schema.json`](guide.schema.json) (regenerate anytime via `retold guides schema`).

Every production run saves an immutable snapshot of its guide files under `.retold/guide.toml`. Updating a guide in your configuration library never alters existing runs unless explicitly requested via `retold resume --upgrade-guide`.

---

## 1. Top-Level Guide Manifest

A guide manifest (`guide.toml`) contains the following required sections:

```toml
#:schema ../../schemas/guide.schema.json
schema = 1
id = "guide-slug"
version = "1.0.0"
name = "Human-Readable Guide Name"
summary = "Brief description of the produced video format."
```

### Manifest Fields

- `schema` *(integer, required)*: Manifest format version (currently `1`).
- `id` *(string, required)*: Alphanumeric slug identifier.
- `version` *(string, required)*: Semver version string (e.g. `"1.0.0"`).
- `name` *(string, required)*: Display title.
- `summary` *(string, required)*: Short description of the guide's output.

---

## 2. Dynamic Inputs (`[inputs]`)

Guides specify input parameters needed to produce each video. Each input defined in `[inputs.<name>]` automatically becomes a CLI flag on `retold make` and `retold check`.

```toml
[inputs.topic]
type = "text"
required = true
prompt = "Concept or topic name"
help = "The primary subject to explain"
example = "Rust Ownership"

[inputs.difficulty]
type = "choice"
choices = ["beginner", "intermediate", "advanced"]
default = "beginner"
help = "Target audience complexity level"

[inputs.episodes]
type = "int"
default = 1
min = 1
max = 10
help = "Number of parts in this breakdown"
```

### Supported Input Types
- `text`: Single-line text string.
- `multiline`: Multi-line text string.
- `choice`: Enumerated choice from a predefined list of string literals.
- `int`: Signed integer with optional `min`, `max`, and `default`.
- `bool`: Boolean flag.

---

## 3. Format and Length (`[format]`, `[length]`)

Defines video dimensions, frame rates, and duration constraints:

```toml
[format]
aspect = "9:16"            # Aspect ratio string: "9:16", "16:9", "1:1"
resolution = [1080, 1920]  # Width and height in pixels
fps = 30                   # Frame rate: 24, 25, 30, 60
target_seconds = [55, 65]  # Minimum and maximum duration in seconds
source_language = "en"     # Master script language tag

[length.en]
seconds = [55, 65]         # Permitted speech duration range for English
# Or word count bounds:
# words = [130, 160]
```

---

## 4. Research Policies (`[research]`)

Controls what source facts the AI writing pipeline may use:

```toml
[research]
policy = "source_only"     # Strict factual adherence: uses only provided sources
max_chars = 14000          # Maximum characters ingested from source documents

[[research.sources]]
kind = "file"              # Source file passed via --source-file CLI flag
optional = false
```

### Research dossier (multi-source research)

Add `[research.dossier]` to research a story from many sources before writing. Every
online source is read, long ones (whole books) are read in chunks, and a planning model
reconciles everything into a cited dossier (`work/01-research/dossier.json` and `dossier.md`):
the exact work and version, at least `min_beats` chronological beats marked
must/should/could, at least `min_iconic` iconic elements and hidden details (famous lines,
symbols, foreshadowing, Easter eggs, book-versus-adaptation differences), each with the
beat that sets it up and the beat that pays it off, plus a character bible (want, flaw,
arc, signature line, original look, needed expressions). A `--source-file` is read
alongside the online sources rather than replacing them.

```toml
[research]
policy = "source_only"
max_chars = 30000          # Default budget per source

[research.dossier]
min_sources = 3            # Distinct sources cited (pages and web URLs; not the user file)
min_beats = 25
min_iconic = 4

[[research.sources]]
kind = "wikipedia"
query = "{{ inputs.title }} {{ inputs.author }} novel"
sections = ["plot", "synopsis"]   # Read first
full = true                # Also every other section, minus reference lists
related = 2                # Plus up to 2 related articles (adaptations, sequels)
language = "en"            # Language edition; templates allowed

[[research.sources]]
kind = "wikiquote"         # Famous lines, verbatim
optional = true

[[research.sources]]
kind = "fandom"            # The work's or author's Fandom wiki
optional = true

[[research.sources]]
kind = "gutenberg"         # Full text of a public-domain book (title and author must match)
max_chars = 600000
optional = true

[[research.sources]]
kind = "web"               # Live web research by a subscription CLI with search tools
brief = "{{ inputs.title }}: hidden details, Easter eggs, famous lines, adaptation differences"
optional = true
```

With a dossier, the planner decides on every dossier beat (keep it in a section, or drop
it with a reason). Every `must` beat has to be kept, and the review page lists what was
left out. `quality.coverage = true` then checks every draft and the final script: each
kept beat and chosen iconic element must really appear (the checker has to quote the
script line, and set-ups must come before pay-offs), every named character must be
introduced, and nothing may come from another version of the work.

---

## 5. Narrative Structure (`[structure]`)

The structure block controls storytelling rules and section allocations:

```toml
[structure]
open_loops = 0             # Number of narrative open loops required (0 for technical/educational)
emotional_peak = false     # Whether an emotional climax is required
max_named_characters = 0   # Maximum distinct characters (0 for non-character content)

[[structure.sections]]
id = "concept"
share = 0.4                # Target share of script (shares across all sections must sum to 1.0)
purpose = "Introduce the fundamental mechanism and problem statement."

[[structure.sections]]
id = "deep_dive"
share = 0.6
purpose = "Explain practical memory safety guarantees in detail."
```

---

## 6. Writing Rules and Gates (`[writing]`)

Configures narration voice, drafting parameters, and deterministic quality gates:

```toml
[writing]
voice = "voice.md"         # Path to Markdown file defining narration voice and prose tone
examples = []              # Optional companion example scripts
tense = "present"          # "present" or "past"
person = "third"           # "first", "second", or "third"
hook_candidates = 2        # Number of hook opening options to draft and judge
drafts = { subscription = 1, paid = 1 }
per_section = false        # Draft one section at a time (each sees the ones before), then stitch
judge_criteria = []        # What the draft judge weighs, one entry each; empty = built-in rubric

[[writing.gates]]
id = "variety"
kind = "sentence_length_variety"
max_same_length_run = 3    # Prevents monotone sentence runs

[[writing.gates]]
id = "starts"
kind = "no_repeated_sentence_start"
```

---

## 7. Visuals and Artwork (`[visuals]`)

Defines image generation parameters, camera shots, safe zones, and style rendering:

```toml
[visuals]
seconds_per_unit = [3.0, 5.0]       # Duration range per illustrated unit
lean_seconds_per_unit = [5.0, 8.0]  # Pacing used when running with --quality lean
style = "Clean diagrams in {{ channel.colors.accent }}. No text, no photorealism."
safe_zones = { top = 0.12, bottom = 0.20 }
shots = ["wide", "medium", "close-up"]
max_same_shot_run = 2
reuse_share = 0.35         # Up to this share of shots recut an earlier frame instead of a new image (0 disables)
frame_resolution = "2K"    # "1K" makes scene frames cheaper where the provider prices it lower; references stay 2K
panels = 1                 # 2 or 4: supporting shots of one scene drawn per image and split along white gutters; hero shots stay single

[visuals.characters]
reference_sheet = false    # Generates dedicated character turnarounds when true
originality = "strict"     # Strict prevents imitation of real actors or existing IP
review = false             # Pauses pipeline for human approval of cast sheets
```

---

## 8. Narration and Audio (`[narration]`, `[edit]`)

```toml
[narration]
mode = "continuous"        # Records single continuous audio take per section
direction = { en = "Articulate, measured, instructive." }

[edit]
motions = ["zoom_in", "pan_right", "pan_left"]
opening = { kind = "cover_title", seconds = 1.0 }
ending = { kind = "hold_fade", seconds = 1.0 }
music = { mood_from = "plan.tone", bed_lufs = -24.0, duck_under_voice_db = -6.0 }
sfx_on_transitions = false
```

---

## 9. Packaging Metadata (`[packaging]`)

Controls social and platform deliverable text formatting:

```toml
[packaging]
platforms = ["youtube"]
rules = "Accurate technical summary under 50 characters."
title_max_chars = 50
hashtag_count = 2

[packaging.notices.en]
prefix = "Briefing: {{ inputs.topic }}."
suffix = "Produced by {{ channel.name }}."
```

Templates support Jinja-style expansion using inputs, channel metadata, and language tags.
Prefixes and suffixes are prepended and appended to post descriptions.

---

## 10. Provider Roles (`[roles]`)

Maps abstract pipeline tasks to provider capability tiers:

```toml
[roles]
plan = "text.smart"
write = "text.smart"
transcreate = "text.smart"
judge = "text.smart"
check = "text.fast"
direct = "text.fast"
design = "text.smart"
package = "text.fast"
image_qa = "vision.fast"
image = "image"
voice = "tts"
transcribe = "transcribe"
```


## Optional parallax settings

`[visuals.parallax]` accepts `enabled` (default false), `max_share` (default 0.35),
`strength` (default 0.35) and `hero_fill` (default false). Share and strength must be finite
values in `[0, 1]`; hero filling requires enabled parallax. Existing guides remain valid.
The director selects the shots within the share cap. Hero filling additionally requires
paid-call authorization and budget. See [operations](operations.md#s2-depth-driven-parallax)
for model setup, motion choices, QA thresholds and recovery.

## Optional lip-sync settings

```toml
[visuals.lipsync]
enabled = true
```

Disabled by default. Applies only with `narration.mode = "dialogue"`. Speaking
characters need cast sheets and visible, roughly frontal faces. Mouth and blink
sprites are composited before camera motion. Rejected sheets or composites keep
the original still artwork. Requires `espeak-ng` (override with `RETOLD_ESPEAK`) and the managed local
`phonemes` model (`retold models install phonemes`).
See [operations](operations.md#s4-lip-sync) for limitations and acceptance status.

For native titles, channel `[title] vertical = true` selects traditional vertical
Japanese/Chinese text. It defaults to false. Urdu and Hindi retain their native
horizontal shaping and configured script font; titles fit both width and font
ascent/descent. See `docs/channel.schema.json` for channel settings.

## Local aspect delivery

```toml
[delivery]
aspects = ["9:16", "16:9", "1:1"]
```

Choose distinct supported aspects. If omitted or empty, the original guide format
remains the only edition. Each requested aspect gets its own FilmCraft sequence,
native title/caption layout and `deliverables/<lang>/<aspect>/` package (`9x16`,
`16x9`, `1x1`). Alternate burned-subtitle languages get a variant subfolder.
Timelines and metadata are prepared at `ready`; exporting still requires
`--allow-video`. The export preflight requires a vision route for thumbnail review.
