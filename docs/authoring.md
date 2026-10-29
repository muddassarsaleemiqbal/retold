> CLI cutover (2026-10-21): guide/channel formats below remain reference material. Legacy execution examples (`make`, `resume`, `wizard`, `check`) are historical; use the [current CLI workflow](../README.md).

# Retold Authoring Guide

Retold is an independent, guide-driven production engine. It ships with no preinstalled guides or channels: all video structures, styles, voices, and branding are authored and owned by you in your local configuration directory (`~/Retold/config/`).

This guide walks through creating, validating, preflighting, and running your own production pipelines.

---

## 1. Clean Installation & Configuration Layout

When you first install Retold, the configuration library contains no guides or channels:

```sh
retold guides list
# Output: No guides yet. Create one with `retold guides create <id>`; see docs/authoring.md.

retold channels list
# Output: No channels yet. Create one with `retold channels create <id>`; see docs/authoring.md.
```

Retold stores your configurations and production library in `~/Retold/` (or the directory specified in `settings.toml`):

```
~/Retold/
├── config/
│   ├── guides/          # Authored guide packages
│   │   └── <guide-id>/
│   │       ├── guide.toml
│   │       └── voice.md
│   └── channels/        # Authored channel brand packages
│       └── <channel-id>/
│           ├── channel.toml
│           └── assets/
│               └── style.png
└── library/             # Generated production runs and ledgers
```

Authoritative JSON schemas are provided for editor autocompletion and strict schema validation:
- [`docs/guide.schema.json`](guide.schema.json)
- [`docs/channel.schema.json`](channel.schema.json)

---

## 2. Creating a Guide

A **Guide** defines *how* a video is produced: input schema, narrative structure, section shares, writing gates, visual pacing, audio ducking, packaging metadata, and AI role routing.

### Interactive Creation

Run `retold` in a terminal, open Configure → Guides, and press `n`. The stepped form
collects an identifier, source language and creative choices; `Ctrl-S` advances each step.
Review before creating. The CLI equivalent is always displayed in the footer.

Leave Example and From empty to create a neutral scaffold. To copy the bundled story-recap
guide, choose `story-recap` in Example (left/right or `F3`). This is an explicit copy into your
configuration library; it does not install anything automatically or invoke providers.

### Non-Interactive Scaffold

You can also scaffold a guide non-interactively using CLI flags:

```sh
retold guides create tech-explainer \
  --language en \
  --purpose "Explain technical concepts clearly with diagrams" \
  --style "Minimalist vector diagrams on dark background" \
  --delivery "Measured, clear, instructive" \
  --writing "Third person technical clarity" \
  --packaging "Short technical title and description" \
  --seconds 60
```

### Copying a Bundled Example

```sh
retold guides create my-recap --example story-recap --name "My recap"
retold guides check my-recap
```

The example is embedded in the binary and includes its English, Hindi and Urdu writing files
and narrative examples. It works without a source checkout. `--dry-run` validates without
creating files. Choose one source: `--example`, `--from`, or scaffold options. Example/import
creation accepts an identifier and display name, while its other settings are copied intact.
Creation never overwrites an existing resource. A successful configuration check establishes
schema/configuration validity; use doctor and video plan for runtime readiness.

### Importing an Existing Guide

If you already have a guide manifest on disk, import it directly:

```sh
retold guides create tech-explainer --from path/to/guide.toml
```

*(Note: when `--from` is passed, scaffold flags like `--language` or `--seconds` cannot be combined with it.)*

### Guide Anatomy

Inside `~/Retold/config/guides/<guide-id>/`, the primary file is `guide.toml`:

```toml
#:schema ../../schemas/guide.schema.json
schema = 1
id = "tech-explainer"
version = "1.0.0"
name = "Tech Explainer"
summary = "Explains technical mechanisms without narrative recap tropes."

[inputs.topic]
type = "text"
required = true
prompt = "Concept or topic name"
help = "The primary subject to explain"
example = "Rust Ownership"

[run]
folder = ["{{ inputs.topic }}"]
cast_depth = 1

[format]
aspect = "16:9"
resolution = [1920, 1080]
fps = 30
target_seconds = [50.0, 70.0]
source_language = "en"

[length.en]
seconds = [50.0, 70.0]

[research]
policy = "source_only"
max_chars = 12000
[[research.sources]]
kind = "file"
optional = false

[structure]
open_loops = 0
emotional_peak = false
max_named_characters = 0

[[structure.sections]]
id = "concept"
share = 0.4
purpose = "Introduce the fundamental mechanism and problem statement."

[[structure.sections]]
id = "deep_dive"
share = 0.6
purpose = "Walk through practical guarantees and consequences."

[writing]
voice = "voice.md"
examples = []
tense = "present"
person = "third"
hook_candidates = 2
drafts = { subscription = 1, paid = 1 }
gates = []

[quality]
max_fix_passes = 2
fact_check = "off"
image_qa = false

[visuals]
seconds_per_unit = [4.0, 8.0]
lean_seconds_per_unit = [6.0, 12.0]
style = "Clean diagrams in {{ channel.colors.accent }}. No text, no photorealism."
safe_zones = { top = 0.12, bottom = 0.20 }
shots = ["wide", "medium"]
max_same_shot_run = 2

[visuals.characters]
reference_sheet = false
originality = "strict"
review = false

[narration]
mode = "continuous"
direction = { en = "Measured, instructive, articulate." }

[edit]
motions = ["zoom_in", "pan_right"]
opening = { kind = "cover_title", seconds = 1.0 }
ending = { kind = "hold_fade", seconds = 1.0 }
music = { mood_from = "plan.tone", bed_lufs = -24.0, duck_under_voice_db = -6.0 }
sfx_on_transitions = false

[packaging]
platforms = ["youtube"]
rules = "Accurate technical title under 50 characters."
title_max_chars = 50
hashtag_count = 2

[packaging.notices.en]
prefix = "Briefing: {{ inputs.topic }}."
suffix = "Produced by {{ channel.name }}."

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

---

## 3. Creating a Channel

A **Channel** defines *branding and identity*: channel name, handle, color palettes, fonts, voice assignments per language, and reference artwork.

### Interactive Creation

```sh
retold channels create studio-alpha
```

The CLI interactively prompts for:
- Channel handle (e.g. `@studioalpha`)
- Primary accent color (hex format `#RRGGBB`, e.g. `#2A75D3`)
- Primary voice provider and model
- Caption font family and style

### Non-Interactive Scaffold

```sh
retold channels create studio-alpha \
  --handle "@studioalpha" \
  --accent "#2A75D3" \
  --language en \
  --voice-provider openrouter \
  --voice-model "google/gemini-2.0-flash" \
  --voice "Puck" \
  --font-family "Inter" \
  --font-style "Bold"
```

### Channel Assets

Channels contain an `assets/` subfolder:
- `assets/style.png`: A 2K reference image establishing the graphic palette, textures, and visual feel.
- Fonts and audio assets.

Retold enforces path sandboxing on channel assets: symlinks and relative paths containing `..` or escaping the channel root are strictly rejected.

---

## 4. Structure Validity vs. Production Readiness

There is an important distinction between a **syntactically valid** scaffold and a **production-ready** guide:

1. **Syntactically Valid (`retold guides check`)**:
   Newly scaffolded guides and channels pass strict schema checks and TOML deserialization. However, scaffolding deliberately places `CONFIGURE: ...` placeholder notices in fields requiring human editorial decisions.

2. **Production Ready (`retold check`)**:
   When preparing for a real run, Retold audits for lingering `CONFIGURE:` placeholders. Any placeholder in prompts, rules, styles, or voices blocks production runs with an informative diagnostic directing you to finalize the text.

---

## 5. Preflight Verification (`retold check`)

Before running a production or spending any API budget, run `retold check` to perform a comprehensive offline preflight check:

```sh
retold check \
  --guide tech-explainer \
  --channel studio-alpha \
  --source-file input.txt \
  --until ready
```

### Preflight Contract (Zero API Calls)

`retold check` guarantees:
- **Zero API model calls**: No text, image, or speech endpoints are contacted.
- **Zero budget spend**: No ledger reservations or holds are placed.
- **No filesystem modifications**: No runs or directories are created.
- **Complete route auditing**: Verifies that every provider required for the requested pipeline depth is installed, authenticated, and has a valid route.
- **Cross-template rendering**: Evaluates prompt templates against channel variables to detect broken placeholders or missing variables before launch.

---

## 6. Running Production

### Interactive Wizard

For interactive launch with guided inputs:

```sh
retold wizard
```

### Scripted Production Run

To launch a headless, automated production run:

```sh
retold make tech-explainer \
  --channel studio-alpha \
  --topic "Rust Ownership" \
  --source-file notes.txt \
  --budget 2.00 \
  --allow-paid \
  --until ready
```

### Review and Inspection

Once a run reaches `ready`, you can inspect the deliverables locally:

```sh
retold review rust-ownership
```

This launches a local, offline HTML review page displaying storyboard shots, narration scripts, word-aligned captions, and generated image artwork.

If you wish to export the final edited video to FilmCraft:

```sh
retold resume rust-ownership --allow-video
```
