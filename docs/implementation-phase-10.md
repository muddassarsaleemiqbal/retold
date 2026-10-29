# Phase 10: Doctor, Setup, Top-Level Commands, Creative Inspection, Export Presets, Profiles & Cutover Hardening

Date: 2026-10-20. Status: implemented and verified across full test suite and quality gates.

Closing out Phase 10 capabilities (`docs/plan-cli.md` §17 & §18, `plan-series-and-quality.md` §5 & §9, `cli-product-spec.md` §7, F16, F23), Phase 10 delivers:
1. Workspace health inspection and safe repair (`doctor [--fix]`).
2. Offline first-run onboarding (`setup`) with zero external network requests.
3. Creative resource inspection: production guides (`guides list|show`) and brand channel profiles (`channels list|show`).
4. Reusable export presets management (`export-presets new|list|show|remove`).
5. Export preset resolution and aspect filtering in video export (`video export <ref> --edition LANG --preset NAME [--aspect ASPECT]`).
6. Reproducible preferences and configuration profiles (`config profile list|new|show|use`).
7. Shell autocompletions (`completions`) and roff manual page (`man`).
8. Top-level ergonomics: `new`, `usage`, `explain`, `schema`, `home`.
9. Full human and NDJSON machine projection parity across all commands.

---

## Capabilities & Architecture

### 1. Doctor (`doctor [--fix]`)
- Inspects configured external editor (`FilmCraft.app` / `$EDITOR`).
- Detects host media toolchains on `PATH` (`ffmpeg`, `ffprobe`).
- Validates essential workspace directories (`channels`, `guides`, `series`, `presets`, `library/music`, `library/sfx`).
- Verifies catalog integrity: loads and parses installed production guides and channel brand profiles, reporting broken manifests or syntax errors.
- Computes aggregated video disk usage across all managed projects.
- Detects orphaned temporary files (`*.tmp`, `.tmp-*`).
- In `--fix` repair mode, recreates missing directory structures and purges stale temporary artifacts safely without touching user media or metadata.
- Outputs clean check-by-check health indicators (`[ok]` / `[!!]`) and itemized repairs in human projection mode.

### 2. Setup (`setup`)
- Offline workspace onboarding for new installations.
- Initializes standard directory tree (`channels`, `guides`, `series`, `presets`, `library/music`, `library/sfx`).
- Scans host tools via local PATH discovery (`ffmpeg`, `ffprobe`, etc.) with zero network calls or provider requests.
- Recommends explicit next steps for provider configuration and video planning.

### 3. Creative Resource Inspection (`guides`, `channels`)
- `guides list`: enumerates all installed production guides with ID, version, title, and synopsis.
- `guides show <guide>`: inspects a specific guide manifest, displaying required/optional inputs, parameter descriptions, and generation properties.
- `channels list`: enumerates installed channel profiles with ID, brand title, and social handle (`@handle`).
- `channels show <channel>`: inspects channel specifications, including configured narration voices per language, color palettes, and typographic settings.
- Fully supports `--json` structured envelope output and `--plain` readable summaries.

### 4. Export Presets Management (`export-presets`)
- `export-presets new <name> --file PATH`: registers and validates a new export specification (aspect ratios, container format, video/audio codecs, caption mode) with cryptographic content hashing.
- `export-presets list`: enumerates all registered export presets.
- `export-presets show <name>`: displays full target specs, container/codec parameters, and content hash.
- `export-presets remove <name> --yes`: safely deletes an installed preset, requiring explicit `--yes` confirmation.
- Validation rejects unsupported aspects (e.g. non 9:16, 16:9, 1:1), invalid containers, or unknown codecs offline.

### 5. Export Preset & Aspect Resolution in Video Export (`video export`)
- `video export <ref> --edition LANG --preset NAME [--aspect ASPECT]`: resolves the stored preset snapshot from `presets/<name>.json`.
- Dynamically translates aspect ratios (`9:16` -> `9x16`, `16:9` -> `16x9`, `1:1` -> `1x1`) into delivery formats.
- Supports filtering to an explicit aspect subset (`--aspect`) verified against preset capabilities.
- Pins the resolved preset name and formats in the hashed authority envelope, requiring explicit `--plan-hash` or `--yes` confirmation to execute.

### 6. Reproducible Preferences & Configuration Profiles (`config profile`)
- `config profile list`: enumerates installed preference profiles in `profiles/*.toml`, highlighting the currently active profile.
- `config profile show <name>`: inspects settings, default channel, default preset, and active status.
- `config profile new <name> [--file PATH]`: creates a new named configuration profile from a file or default template, enforcing that no credentials or secrets (`api_key`, `token`, etc.) are ever stored.
- `config profile use <name>`: switches the active profile in `settings.toml` (`selected_profile`), ensuring clean multi-environment management.

### 7. Shell Autocompletions & Manual Pages (`completions`, `man`)
- `completions <shell>`: generates completion scripts to stdout for `bash`, `elvish`, `fish`, `powershell`, and `zsh` via `clap_complete`.
- `man`: dynamically renders the Unix roff manual page for `retold-next` via `clap_mangen`.
- Both commands honor `--plain` (direct raw text) and `--json` (versioned machine-readable envelopes).

### 8. Top-Level Ergonomics & Cutover Readiness
- `new`: direct shortcut to series and project creation.
- `usage`: typed CLI command overview and discovery.
- `explain <code>`: detailed explanation and remediation advice for diagnostic error codes.
- Complete protocol compliance with versioned NDJSON output envelopes and human terminal formatters.

### 9. Local Activity Timeline & Video Audit History (`video history`, F24)
- `video history <ref> [--kind plan|edit|review|export|all]`: inspects recorded chronological audit events for a video.
- Aggregates edit history (component edits with revisions, diffs, and invalidated downstream stages), reviews (approvals, locks, and feedback notes), execution plan steps, and export deliveries.
- Provides numbered sequences, ISO timestamps, human summaries, and structured event details.
- Validates filter kinds strictly offline with zero provider calls.

### 10. Saved Library Views & Organizational Tags (`views`, `video tag`, F03)
- `views save <name> --filter-file PATH`: registers a named search, tag, archive, and sort view specification from a JSON file.
- `views list`: enumerates all saved views with version and content hash.
- `views show <name>`: displays saved view filter specifications.
- `views remove <name> --yes`: safely deletes a saved view preference, requiring explicit `--yes` confirmation.
- `video list --view <name>`: applies resolved view filters (channel, search query, tag, sort mode, limit, archive) to library queries.
- `video tag <ref> --add <tag>` / `video tag <ref> --remove <tag>`: atomically tags videos for flexible organizational categorization and series groupings.
- `video list --tag <tag>`: filters library runs by organizational tag.
- `video list --sort title|channel|created|updated`: sorts library results by chosen metadata property.

---

## Quality Gate Verification
- Passed `cargo fmt --all --check`
- Passed `cargo clippy --workspace --all-targets -- -D warnings`
- Passed `cargo test -p retold --test clean_code` with zero baseline growth under `preset = "strictest"`
- Passed integration tests in `crates/retold-cli/tests/doctor_and_setup.rs`, `crates/retold-cli/tests/creative.rs`, `crates/retold-cli/tests/presets.rs`, `crates/retold-cli/tests/profile.rs`, `crates/retold-cli/tests/continuation.rs`, `crates/retold-cli/tests/review.rs`, `crates/retold-cli/tests/tags.rs`, `crates/retold-cli/tests/views.rs`, and `crates/retold-cli/tests/protocol.rs`
- Passed full test suite `./scripts/check.sh`
