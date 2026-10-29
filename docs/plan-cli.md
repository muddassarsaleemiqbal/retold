# Plan: Retold CLI rewrite

Cutover update (2026-10-21): the new CLI is now `retold`; `retold-next` remains an alias. The legacy binary was removed at the user's request, without legacy command compatibility. Older binary names and pending-phase statements below are historical; consult the [handoff](implementation-handoff.md) and [README](../README.md) for current status.

Status: finalized for implementation planning after independent revision 11 approval (see `reviews/cli-plan-review.md`) · Written 2026-10-18 · Revision 11 · Companion to `plan-series-and-quality.md`.

Implementation update (2026-10-18): the user explicitly deprioritized legacy compatibility.
Legacy command inventories, aliases, adapters and command-parity fixtures below are historical
planning requirements and are not implementation gates. Existing-data, accounting and permission
safety still apply whenever existing state is accessed. The new interface is built directly.
Current implementation evidence is tracked in [Phase 1](implementation-phase-1.md) and
[Phase 2A](implementation-phase-2a.md). Phase 2A delivers saved-video queries and read-only
inspection plans. [Phase 2B account core](implementation-phase-2b.md) adds shared transactional
accounting and fresh authority APIs. [Phase 2C](implementation-phase-2c.md) routes every per-use
engine dispatch through that account. [Phase 2D](implementation-phase-2d.md) adds hashed executable
plans and `video continue` up to `ready`. [Phase 2E](implementation-phase-2e.md) adds explicit,
edition- and format-scoped `video export`. [Phase 3A](implementation-phase-3a.md) adds
read-only configuration validation. [Phase 3B](implementation-phase-3b.md) adds
offline provider/tool discovery. [Phase 3C](implementation-phase-3c.md) adds typed provider
imports with validation and recovery backups. [Phase 3D](implementation-phase-3d.md) adds explicit
ordered provider route inspection and updates. [Phase 3E](implementation-phase-3e.md) adds
safe removal of unreferenced provider definitions. [Phase 3F](implementation-phase-3f.md) adds
explicit typed definition replacement with route compatibility checks.
[Phase 3G](implementation-phase-3g.md) adds a redacted configured-provider inventory;
[Phase 3H](implementation-phase-3h.md) adds durable cooldown admission and inspection;
[Phase 3I](implementation-phase-3i.md) adds health classification and configuration migration;
[Phase 3J](implementation-phase-3j.md) adds explicit, authorized `providers test`; remaining
Phase 3 work is pending. [Phase 6A](implementation-phase-6a.md) adds read-only
account attempt inspection; [Phase 6B](implementation-phase-6b.md) adds explicit billing
reconciliation without dispatch authority; [Phase 6C](implementation-phase-6c.md) adds read-only
`spend` reports; [Phase 6D](implementation-phase-6d.md) adds targeted `account limit` changes. [Phase 4A](implementation-phase-4a.md) opens the full-screen app with a read-only Home screen; [Phase 4B](implementation-phase-4b.md) adds the interactive Video screen; [Phase 4C](implementation-phase-4c.md) adds dedicated Providers and Spend screens, log pane toggles, and PTY tests; [Phase 4D](implementation-phase-4d.md) closes Phase 4 with the Command Palette, Theme Engine, and Mouse Interaction. Current loop stopping point: [handoff](implementation-handoff.md).

The user approved rewriting the CLI from scratch. The engine crates (`retold-engine`,
`-providers`, `-guide`, `-filmcraft`, …) stay and are changed by the series and quality plan.
The final replacement covers everything in `crates/retold` (about 8,300 lines: `cli.rs`, `wizard.rs`, `library.rs`,
`execute.rs`, `doctor.rs`, `readiness.rs`, `setup.rs`, …). The old binary stays available until the cutover gate in §17 passes.

Contents
1. Lessons from the current CLI
2. Goals and non-goals
3. Architecture
4. Information model
5. Command reference
6. Interactive app (TUI)
7. Providers: set up, test, route and monitor from the CLI
8. Spending and limits
9. Runs, jobs and the background studio
10. Review, approve and edit
11. Library, series and assets
12. Output, errors and help
13. Configuration and data safety
14. Quality of life features
15. Reliability and testing
16. Migration
17. Build phases
18. Acceptance tests
19. Execution, authorization and recovery contracts
20. Expanded product specification and required features
21. Resource efficiency and quality contract
22. Runtime, editing and integration invariants
23. Production-stage evidence and quality gates
24. Evidence-led algorithm strategy

---

## 1. Lessons from the current CLI

What went wrong, from the source and from the Drishyam run:

| Area | Problem |
|---|---|
| Structure | 21 commands at one level; everyday commands mixed with developer tools (`render-shot`, `bench`, `man`). |
| Series | `canon …` and `make --part canon` are two models of the same thing; a series shows as four unrelated videos. |
| Home menu | "Resume X" picks an arbitrary unfinished run (the first found on disk); "Videos in your library" prints and exits; setup items crowd the menu; no series entry. |
| Progress | Hundreds of `✓ … cached` lines; no per-stage progress; no time estimate. |
| Failures | Labelled with the wrong step (`plan stopped`, `direct-fix-0 stopped`); raw JSON; 0-based ids in logs vs 1-based in commands; reported only when the slowest parallel branch ends (hid the Hindi failure for hours). |
| Costs | A worst-case cost warning on every resume even when nothing is paid; spend outside a run (music library, $1.04) is not tracked anywhere. |
| Permissions | `--allow-paid`, `--allow-video`, `--yes`, `--budget` overlap; the wizard asks differently than the flags. |
| Footguns | `--upgrade-guide` silently re-runs the whole pipeline; no preview of what `resume` will do. |
| Providers | Only by editing `providers.toml` by hand. The file currently holds a stray `context_size = 8192` under `[providers.codex-personal.tiers]`: invalid config was written and never caught. Rate limits are invisible until a task fails. |
| Control | No stop, pause or status from another terminal; closing the terminal kills the run. |
| Editing | No way to edit a shot or line; cache files were edited and their hashes recomputed by hand. |
| Names | Very long slugs (`drishyam-hindi-film-series-ajay-devgn-drishyam-the-conclusion`). |

---

## 2. Goals and non-goals

**Goals**
1. **Obvious.** A new user can make a video from the home screen without reading docs; an
   expert can do everything with one-line commands.
2. **Everything configurable in the app.** Providers, routing, voices, channels, music library,
   budgets. Editing TOML is never required (but still allowed).
3. **No surprises, no waste.** Every action that costs time or money shows a plan first.
4. **Calm output.** One line per stage; failures as sentences with a fix.
5. **Runs survive the terminal.** A background studio process; attach and detach.
6. **The same features three ways:** TUI, command line, and `--json` for scripts and agents.
7. **Reliable.** Typed state machines, validated config, atomic writes, snapshot-tested screens.
8. **Complete production workspace.** User-owned setup, inspectable assets/editions, shared series,
   batch queues, verified delivery folders, backup/recovery and redacted diagnostics.
9. **Designed terminal experience.** Responsive keyboard navigation, calm visual hierarchy,
   purposeful color, readable multilingual text and complete empty/error/disconnected states.

**Non-goals for this rewrite**
- A web app. The review page stays HTML, served locally.
- Windows support (macOS first; Linux keeps compiling).
- Uploading to YouTube (designed for in §14, built later).

---

## 3. Architecture

```
crates/
  retold-app      NEW  application layer: every use case as a typed function
                        (list_videos, plan_continue, start_run, add_provider, …).
                        No printing, no prompts. Returns data and events.
  retold-studio   NEW  background process: job queue, scheduler, provider health.
                        Uses the shared application accounting service. Local API (JSON-RPC over a Unix socket at
                        ~/Library/Application Support/retold/studio.sock).
  retold-cli      NEW  the `retold` binary: clap commands, TUI (ratatui), plain and JSON
                        renderers. Calls retold-app directly, or the studio when it runs.
  retold-engine   kept pipeline; gains a typed event API and step metadata (stage, item, kind).
```

Rules:
- **One use-case layer.** The TUI, the commands and the JSON output all call `retold-app`.
  No logic lives in presentation code, so the three can't drift apart (today the wizard and the
  flags ask for permissions differently).
- **Typed events.** The engine emits `StageStarted`, `ItemProgress{stage, done, total}`,
  `StepFailed{stage, item (1-based), kind, reason, fix}`, `Spend{provider, usd}`,
  `ProviderLimited{until}`, … Renderers decide how to show them. No string parsing of logs.
- **The studio is optional.** Without it, commands run in the foreground. With it (`retold studio
  start`, or started automatically), runs are jobs that keep going after the terminal closes.
- **Crate boundaries enforce layering:** `retold-app` cannot depend on `ratatui` or `clap`.

Libraries: `clap` (commands), `ratatui` + `crossterm` (TUI), `ratatui-image` (inline images in
Kitty, iTerm2 and WezTerm, falling back to "open image"), `miette` (diagnostics), `keyring`
(macOS Keychain), `mac-notification-sys` (notifications), `rusqlite` (library index), `insta`
(snapshot tests), `expectrl` (PTY tests).

---

## 4. Information model

The user works with five things. Every screen and command is about one of them.

| Thing | Example | Reference |
|---|---|---|
| **Video** | Drishyam · Part 1 (Hindi) | `drishyam/1`, `nosferatu`, `last`, or a unique prefix |
| **Series** | Drishyam (Hindi films) | `drishyam` |
| **Channel** | The Whole Plot | `the-whole-plot` |
| **Guide** | Story recap | `story-recap` |
| **Provider** | codex-personal, openrouter | its id |

A video has a preparation milestone (`draft → research → script → storyboard → cast →
images/voice → captions → ready`) plus independent job state, stage/edition outcomes and release
review status. Do not collapse mixed running/blocked work into one Failed label. The product
spec §4.3 defines aggregation, priority and next actions; §4.5 defines exported versus QA-passed
versus human-reviewed delivery. Existing successful outputs remain visible during partial failure.
`production-stage-contract.md` defines the proof-bearing artifacts and quality/usage gate at
every preparation stage; ready is a cached cross-modal compatibility result, not mere file presence.

Items are numbered **from 1** everywhere: logs, review page, commands and JSON.

---

## 5. Command reference

```
retold                                   open the app (TUI); non-TTY prints usage and exits without prompting
retold new [guide] [flags]               new video (asks for missing flags only in interactive mode)
retold series new|show|update|run|list   series work
retold video <action> <ref>              one video; nested edit grammar below
    list [--series s] [--state s] [--channel c] [--search text]
    show                                  stages, progress, blockers, spend, next step
    plan                                  what continue would do: steps, reasons, cost, time
    continue [--spend USD] [--until stage] [--yes]
    stop | pause | unpause
    review [--approve items] [--unlock items]
    edit shot <ref> <n> | edit line <ref> <n> [--lang l]
    edit cast <ref> <name> | edit budget <ref> <usd>
    redo <stage> [items]                  1-based items; shows its plan first
    undo                                  restore prior content as a new revision
    export <ref> --edition LANG [--preset NAME|--format 16x9|9x16|both] [--project-only]
    logs [--follow] [--level warn]
    open [--video|--folder|--review|--log]
    copy <new-ref> [--lang spec] [--channel c]   reuse everything that still applies
    archive | unarchive | delete          delete moves to the Trash and asks first
    costs | add-language <spec> | clean
retold providers                         list with health, limits and spend
    scan | add | edit <id> | remove <id> | test [id] | login <id> | route [task]
retold channels list|new|edit|voices|test-voice|fonts|look
retold library music|sfx  import|list|play|tag|licence|check
retold guides list|new|edit|check
retold spend [--today|--month] [--by provider|video|series]
retold studio start|stop|status
retold doctor [--fix]
retold config get|set|edit|validate|backup|restore
retold dev check|bench|render-shot|man|completions|events
retold help <topic> | retold explain <error-code>
```

Global flags: `--workspace PATH`, `--offline`, `--json`, `--plain`, `--quiet`, `--verbose`, `--no-color`, `--yes`, and
`--dry-run` on every command that changes anything.

**Permissions** (replaces `--allow-paid`, `--allow-video` and `--budget` per command):
- `--spend USD`: the most this command may spend. The default is 0: free providers only, and
  the plan says which steps would need money.
- The per-video cap stays as a safety ceiling, set at creation (`video edit budget` changes it).
- `--usage-policy NAME` selects finite subscription/request/token/modality/local-resource bounds
  separately from money; a selected compatible user policy is required for generation. $0 never
  grants unlimited subscription requests. Details and linear/TUI paths are in §21.
- Project creation and rendering require `video export` (including `--project-only`).
  `new`, `continue`, series execution and setup stop at `ready`; their stage parser rejects
  `project` and `export`. See §19 for authorization and legacy flags.
- `--yes` skips ordinary confirmations within authority; it never approves/locks content,
  accepts stale versions, grants spending or authorizes export.

The old commands (`make`, `resume`, `runs`, `status`, `canon`, `wizard`, `redo`, `review`) stay
as hidden aliases for two releases and print the new form once.

---

## 6. Interactive app (TUI)

`retold` opens a full-screen app. The footer always shows the command-line equivalent of the
selected action, so users learn the CLI by using the app.

### 6.1 Home
```
┌ Retold ─────────────────────────── codex ✓  codex-personal ⏳ 2:45 PM  claude ✓ ┐
│ Today $1.28 · Month $3.10 of $20                 FilmCraft ✓   Studio ● 1 job    │
├──────────────────────────────────────────────────────────────────────────────────┤
│ CONTINUE                                                                         │
│ ▸ Drishyam · Part 2   images 3/48 ▓░░░░░  waiting: codex-personal 2:45 PM  yday  │
│   Drishyam · Part 1   exported · 6:00 · $1.28                          12:58    │
│ NEW         New video · New series · From a template                             │
│ LIBRARY     Videos (4) · Series (1) · Archive (0)                                │
│ SETUP       Providers · Channels · Music & sounds · Guides · Doctor              │
└ ↑↓ move  ⏎ open  n new  / search  : commands  ? help  q quit                     ┘
  retold video show drishyam/2
```

### 6.2 Video screen
- Stage list with progress bars and time estimates; parallel branches side by side.
- **Blockers panel:** each blocked item names impact and a specific safe action, following
  the product spec §4.4 matrix. No generic Skip/Accept that bypasses required media, QA or billing.
- **Plan panel:** what Continue would do, its cost and the providers it would use.
- Actions: Continue · Stop · Review · Export · Redo · Edit · Logs · Open · Copy · Archive.
- Updates live while running (studio events); `l` toggles a filtered log pane.

### 6.3 New video and new series
Step-by-step forms with remembered defaults per channel and guide: guide → work/title → part(s)
→ languages and captions (with plain examples such as "Hindi voice, Roman Hindi captions") →
quality → spend cap → **summary with plan and cost estimate** → Start (here or in the studio).
Fields validate as you type.

### 6.4 Review in the app
A grid of frames and character sheets shown inline (terminal image protocols), or a list with
"open". Keys: `a` approve and lock, `U` unlock, `r` redo with a note, `e` edit shot, `space` play the voice
line for that frame. The HTML review page stays for full-size viewing.

### 6.5 Providers screen
See §7. Add, test, log in, reorder routes (`J`/`K`), and see limits and spend.

### 6.6 Music and sounds screen
Tracks with mood, tempo, loudness, licence and source; `space` preview, `t` tag, `i` import a
folder, `x` reject. Shows coverage: which moods each guide needs and which are missing.

### 6.7 Everywhere
Command palette (`:` or `ctrl-k`) with fuzzy search over every action. `?` shows the keys for
the current screen. Themes: default, high contrast and no colour. Mouse support for clicking
and scrolling.

---

## 7. Providers: set up, test, route and monitor from the CLI

### 7.1 Discover
`retold providers scan` (also on first start):
- Finds the `claude`, `codex` and `gemini` binaries and their versions, plus **every profile
  folder** (`~/.claude*`, `~/.codex*`), and shows which are logged in and as whom.
- Finds local models (`llama-completion`, installed `models/`), FilmCraft and ffmpeg.
- Detects keys in the environment, `.env` and the Keychain (never printing them).
- Offers to add each item with a sensible default role (text, vision, image, voice, music,
  transcribe).

### 7.2 Add and edit
```
retold providers add
? Kind › Codex CLI profile │ Claude CLI profile │ Gemini CLI │ OpenRouter │ ElevenLabs │ Local model
? Profile folder › ~/.codex_third                (found · logged in)
? Name › codex-third
? Parallel jobs › 2
? Use for › [x] text  [x] vision  [x] images
✓ Profile added · no requests sent · test with: retold providers test codex-third
```
API keys are typed hidden and stored in the **macOS Keychain**. `.env` is still read, and
`doctor` offers to move keys.

### 7.3 Test
`providers test [id]` first shows the selected capabilities, quotes and required authority.
Only an explicit test action sends requests; scan/add never test automatically. Tests use the
same §19 authorization and reservations as runs. Default `--spend 0` tests only free routes;
paid tests require an explicit positive allowance, even after confirmation.

### 7.4 Routing
`providers route` shows each task (text fast/smart/best, vision, image, voice per language,
music, sound effects, transcription) with its ordered providers. Change the order in the TUI or
with `providers route image --order codex,codex-personal,codex-third,local-flux`. Validation
rejects unknown providers and capabilities a provider doesn't have.

### 7.5 Health and limits
The application keeps a durable **provider health ledger**, shared with the studio: `ok`, `limited until <time>` (parsed from
messages like "try again at 2:45 PM"), `failing (n in a row)` and `logged out`. It's shown in the
header, the providers list and `doctor`. Limited providers are skipped until their reset time,
with no wasted retries, and you can ask for a notification when one is available again.

### 7.6 Usage
Per canonical quota account: requests, failures, latency, money and native resource vectors.
Distinguish observed/estimated/unknown quota and usage; aliases share allowances. See §21 for
resource reservation and waste/quality reports; an opaque limit never becomes an invented percent.

---

## 8. Spending and limits

- **One resource/accounting authority for every provider call** (not just USD-billed calls): runs, provider tests, library generation and anything
  else. (The Lyria music library was paid for outside any ledger; that can't happen again.)
- Caps: per video (existing), **per day and per month** (new, in `config`), and per command
  (`--spend`).
- Every plan shows an estimate; every paid step records the actual cost. `retold spend` breaks
  it down by provider, video, series and day.
- Reconciling ambiguous charges stays (`video costs --reconcile`).

---

## 9. Runs, jobs and the background studio

- The **studio** runs jobs from a queue, with per-provider pools shared fairly across videos.
- With the studio on, `video continue` queues a job and shows live progress. `ctrl-c` detaches
  and the job continues. `video stop` stops at the next safe point; `pause` holds new steps.
- **Scheduling:** `video continue --after 2:45pm`, or `--when-available codex-personal` to wait for
  a limit to reset.
- **Notifications:** a macOS notification and terminal bell when a video finishes, fails, needs
  review or reaches its budget.
- **Crash safety:** if the studio dies, its jobs return to `stopped` at their last safe point, and
  `doctor` explains what happened.
- Without the studio, everything runs in the foreground with the same events and output.

---

## 10. Review, approve and edit

- **Approve (which locks)** any item: script, storyboard, sheet, frame or voice line. Approval records the content hash and pinned inputs. Locked items
  never regenerate automatically; explicit redo shows and confirms the affected set. Changes upstream mark them stale (series plan §4.3).
- **Edit shot** (`video edit shot <ref> 8`): action, characters, staging and shot type, in a form or
  `$EDITOR`. Only that frame is redrawn; the storyboard is marked edited, not regenerated.
- **Edit line**: classify spoken-form versus narrative-content changes. Spoken-form-only changes
  affect its context group/timing; meaning changes also stale mapped translations/visuals/cues
  before reuse/export. Runtime contract §5 defines semantic dependencies and edition retiming.
- **Edit cast**: change a character's look. Every frame that uses it is listed before anything
  is redrawn.
- **Undo:** every edit keeps the previous version; `video undo` restores its content as a new
  revision and marks affected descendants stale. It never rewinds spend or deletes history.
- **Notes:** attach a note to any item ("make Meera older"); `redo` uses notes as feedback.

---

## 11. Library, series and assets

- **Index:** a SQLite index of all videos (state, series, channel, languages, dates, spend,
  tags) for instant lists and search; incrementally maintained/rebuilt from authoritative commits
  under runtime contract §2, with visible freshness and atomic staged publication.
- **Series view:** parts with progress, shared cast/places/props with versions, stale items and
  total spend.
- **Templates:** save a video's settings as a template ("Hindi film recap, Roman captions, $1
  cap") and start new videos from it.
- **Tags, search and archive:** `video list --search drishyam --state blocked`.
- **Music and sound library** (`library music|sfx`): import folders with licence records
  (source, URL, licence, attribution), automatic analysis (mood, tempo, key, loudness), preview,
  a coverage report per guide, and a check that every track in a video has a licence record.
  Attribution text is added to the post description automatically.
- **Disk:** `video clean` keeps deliverables; `doctor` reports disk use per video.

---

## 12. Output, errors and help

### 12.1 Progress
- Terminal: one line per stage, updated in place; parallel stages side by side; warnings
  collapsed per item.
- `--plain`: stage changes, failures, spend and the result only. Cached steps are counted
  ("412 reused"), never listed.
- `--json`: newline-delimited typed events with a stable, versioned schema.

### 12.2 Errors
Every error has a **code**, one sentence, the item it is about and a fix:
```
✗ RG-IMG-QA  images · frame 8 · "Sam at the camp gate"
  The phone screen faces the viewer; the shot needs it facing away.
  Fix: retold video redo drishyam/1 images 8   ·   retold video edit shot drishyam/1 8
  More: retold explain RG-IMG-QA
```
- Reported **the moment it happens**, not when the slowest parallel branch finishes.
- Raw provider and checker output goes to the log only.
- Connection drops that recover on retry are never shown as failures.
- `retold explain <code>` lists causes and fixes.

### 12.3 Help
- `retold help <topic>` for languages, series, budgets, providers, captions and more.
- Examples on every command; suggestions for typos in commands, refs and values.
- Dynamic shell completion for video refs, series, providers, channels and stages.

---

## 13. Configuration and data safety

- **One typed, versioned config model** (`settings.toml`, `providers.toml`, channels, guides)
  with a JSON schema and **validation on every write**, so no invalid file is ever written
  (the stray `context_size` in today's file could not happen).
- Atomic writes with a timestamped backup; `config backup|restore`.
- Automatic, logged **migrations** between versions.
- Secrets live in the Keychain; config files never contain keys.
- `config get|set` for scripts; `config edit` opens `$EDITOR` and validates before saving.
- One writer per video (file lock; the studio is the only writer while it runs).
- Destructive actions (delete, clean, reset) confirm, support `--dry-run`, and move files to the
  macOS Trash.

---

## 14. Quality of life features

1. **`last` and short refs everywhere**; mutating commands require an explicit ref (including
   explicit `last`). Ambiguous prefixes return candidates without choosing one.
2. **Plan before run** for every expensive command; `--dry-run` everywhere.
3. **Time estimates** from past runs (per stage and provider).
4. **Pick up where you left off**: Home highlights the last object and preserves its position;
   entering it remains deliberate. Navigation/launch rules are in product spec §4.1.
5. **Copy to clipboard** (`y`): a command, path or caption text.
6. **Open anything**: video, folder, review page or log.
7. **Preview**: play a voice line, a music track or the rendered video (Quick Look).
8. **Copy video**: same story with a new language or style, reusing everything that applies.
9. **Notifications**: finished, failed, review needed, provider available again.
10. **Publishing kit**: post text, tags, chapters and thumbnails in one folder; a YouTube upload
    export target later.
11. **`doctor --fix`**: reinstall missing models, repair config, log providers back in, rebuild
    the index.
12. **First-run setup**: scan, add providers, create a channel/guide, prepare a sample through
    review checkpoints and a route/allowance plan. No free route is assumed. Explicit export
    is required for a project/video; content approval and spend authority stay separate.
13. **Accessibility**: no-colour/high-contrast themes, ASCII and linear `--plain` output, plus
    version-pinned external review. Test supported assistive environments (product spec §4.6).
14. **What's new** after an upgrade (`retold help changes`).

---

## 15. Reliability and testing

Design rules:
- **No logic in renderers.** Use cases are functions over a store, tested without a terminal.
- **State machines** for the video lifecycle and jobs; illegal transitions are rejected.
- **No `unwrap`/`expect` outside tests**; typed errors with codes.
- **One writer per video**; atomic writes; tested crash recovery.

Tests:
1. **Snapshot tests** (`insta`) for every TUI screen at three terminal sizes, and every plain and
   JSON output.
2. **PTY end-to-end tests** (`expectrl`) with mock providers: new video, continue, stop, review,
   providers add/test/route.
3. **Scenario tests** for each Drishyam failure (§18 here and §13 of the series plan).
4. **Property tests** for config round-trips and migrations; **fuzzing** for ref and language-spec
   parsing.
5. **Studio tests**: crash in the middle of a job, two clients at once, a provider limit and
   reset, the budget reached.
6. **Golden JSON schemas** for `--json` output and events; changing them requires bumping the
   schema version.
7. Lints: `clippy` pedantic, `cargo deny`, and the existing clean-code baseline.
8. Performance budgets: `retold` starts in under 150 ms; `video list` over 1,000 videos in under
   100 ms.

---

## 16. Migration

- Existing folders are the source of truth; the library index is a disposable projection.
  Read-only inspection does not migrate runs or call providers.
- Before writing, validate the source schema and write an immutable backup plus a migration
  journal containing source hashes, target schema version and each completed operation.
  Stage new manifests alongside old ones, validate, then atomically switch the active manifest.
  A crash resumes the journal idempotently or restores the backup before any execution.
- Legacy cache entries are reused only when artifact bytes/schema, original request identity,
  recorded inputs and charge identity can be verified. Preserve original keys and append a
  versioned mapping to semantic keys; never overwrite artifacts or manufacture provenance.
  Unknown mappings are `legacy_unverified`: inspect/export verified existing outputs, but block
  affected regeneration pending an explicit plan and authorization. Migration makes zero calls.
- Preserve exact costs and unresolved holds once, with original request IDs. Import into shared
  accounting transactionally before any paid execution. Historical costs remain historical;
  unresolved holds reduce available current allowances until reconciled, without double counting.
- Old and new binaries must never write the same run concurrently. Side-by-side use means
  separate fixture/copy roots until cutover. Legacy binaries cannot write upgraded manifests;
  the launcher refuses them, and downgrade requires an offline restore into a separate root.
  Never restore over newer results or discard charges accumulated after the backup.
- Secret migration is opt-in, verifies Keychain access before removing a source secret and
  leaves source configuration untouched on failure. Linux uses environment/file references
  with restrictive permissions; a missing Keychain never prevents read-only use.

---

## 17. Build phases and cutover

`C` denotes the companion series/quality plan's phase. Both plans share the contracts in §19;
engine capabilities land before a UI offers actions using them. Unavailable new actions report
`RG-NOT-AVAILABLE`, without side effects. These phase gates are required, not optional snapshots.

| Phase | Deliverable and dependencies | Required exit evidence |
|---|---|---|
| 1 | `retold-app`, extend existing engine events, errors, plain/JSON contracts; C1 failure events | Event schemas, immediate independent-branch failures, noninteractive tests |
| 2 | Commands, refs, safe plan/continue/export, fresh job authority, transactional accounting core; C1 planning and C2 input/change model | Concurrent cap tests, default-zero and export-boundary tests, cache-only plan accuracy |
| 3 | Config validation/backups, migration and provider setup/health; C1 health and C2 migration | Interrupted/repeated migration and rollback fixtures; scan/add make no requests |
| 4 | TUI home/video/new/provider flows using available use cases | PTY/snapshot parity and accessibility checks; series creation stays unavailable until phase 9 |
| 5 | Studio queue, control, provider pools, attach/detach; depends on phase 2 accounting | Ownership, idempotency, crash, stop/pause, event replay and two-client tests |
| 6 | Spend reports and cap configuration UI over phase 2 enforcement | Cross-mode accounting and reconciliation; day/month boundary tests |
| 7 | Review/edit/undo and previews; depends on C3 locks/edit semantics | Approve/stale/redo and exact descendant invalidation tests across interfaces |
| 8 | Library index/search/templates/copy and audio manager; depends on C4 audio contracts | Rebuild index without calls; copy preserves provenance; licence/import checks |
| 9 | Series commands/screens; depends on C5 registry/scheduler and phases 5/7/8 | Pinned assets, no cross-part key cascades, shared budget/provider pools |
| 10 | Doctor/setup/help/accessibility; safe repair plans | No implicit paid/export actions; all legacy workflow fixtures and cutover suite |

The binary remains `retold-next` throughout development. It becomes `retold` only after all
10 phases and their required companion dependencies pass, with both plans' acceptance suites.
No shell-only cutover after phase 4. Optional C10 FLUX is not a cutover prerequisite.

Before phase 1 implementation, check in an inventory generated from current clap help/tests:
all commands/subcommands, dynamic guide input flags, aliases, flags, defaults, output formats,
exit behavior and files read/written. Each row maps to a replacement and a recorded fixture. The expanded specification
`cli-product-spec.md` §3 maps every additional required feature F01–F24 into these same phases.
All five complete journeys J1–J5 and the structured-delivery/terminal-design gates are required
for cutover; a feature may not be dropped merely because it was added after revision 3.
Include `make`, `resume`, `runs`, `status`, `canon`, `wizard`, `redo`, `review`, `costs`, `models`,
`check`, `render-shot`, `guides`, `channels`, `doctor`, `bench`, `man` and completions, plus any
additional current commands. Inventory completeness is checked against clap, not this list.

Keep old names/arguments for two releases using adapters to the new use cases. Notices go to
stderr (typed diagnostic events in JSON). `--budget` remains a video ceiling, never spend
permission; `--allow-paid` requires explicit `--spend` rather than granting an unlimited amount.
`--allow-video` on legacy make/resume returns a migration error showing a separate explicit
`video export` command; it performs no partial run. No adapter calls legacy permission-merging
code. Intentional permission changes are documented and tested, not silently emulated.

---

## 18. Acceptance tests

1. Home lists unfinished videos newest first; none is picked arbitrarily.
2. Opening a video from the library shows its screen with actions; nothing prints and exits.
3. A continue where everything is cached prints at most 10 lines with `--plain`, and `video plan`
   reports "0 steps · $0".
4. An image-QA failure shows its code, stage, 1-based frame and reason; never `plan` or
   `direct-fix-0`; no raw JSON.
5. A translation failure is shown within seconds while images keep drawing.
6. A provider limited "until 2:45 PM" is shown once with its reset time and gets no requests
   before then.
7. `providers add` finds a Codex profile and adds selected routes without requests; a separate
   authorized test verifies its capabilities. The written file validates.
8. No command can write invalid config; `config validate` reports the stray field in today's file.
9. `--spend 0` never calls a paid provider; the plan names the steps that would need money.
10. Closing the terminal during `video continue` (studio on) doesn't stop the job; `video show`
    shows the running job; `video logs --follow` attaches to events.
11. `video edit shot <ref> 8` plans frame 8 and its true descendants, with nothing upstream.
12. Every paid call, including library tools, appears in `retold spend`.
13. Every TUI action shows its CLI command, and running that command does the same thing.
14. Old names (`make`, `resume`, `runs`, `status`, `canon`) still work and print the new form once.


## 19. Execution, authorization and recovery contracts

### 19.1 Fresh authority and explicit export

Dry-run is read-only planning before admission and persists nothing (runtime contract §6).
Synchronous config/content edits create audited versioned commits; they do not invent generation
jobs or execution authority. Each actual execution/job admission persists an authorization envelope: unique job/request
ID, resolved video/series IDs, reviewed plan hash and input versions, maximum spend, allowed
provider capabilities/routes, creation time, optional explicit deadline and export intent.
The default allowance is zero. Saved run approvals are historical review evidence only;
`allow_paid`, `allow_video` and old `yes` values never grant new execution authority.
`--yes` skips confirmations only; it cannot raise allowance, unlock stale items or permit export.
A displayed plan and its authority are revalidated under ownership before dispatch; changed
inputs, routes, affected set or higher quotes require a new plan/authorization.

Only `video export` creates a FilmCraft project or exports video, including project-only mode.
Authorization is scoped to the resolved run and formats. New/continue/series/setup cannot cross
that boundary, even with `--until export`, `--yes` or historical permissions. Retries and fallback
routes remain within the same envelope and reservations; disallowed/over-budget routes block.
Provider testing and library generation use the identical boundary. Scan/add are offline.

Detachment preserves the same job allowance, never refreshes it. A studio crash stops the job;
explicit restart creates a new zero-default authorization and links prior charges/holds.
Queued jobs may start only while their persisted envelope is valid and plan still matches;
an expired deadline blocks them. New invocations cannot borrow another job's unused allowance.

### 19.2 One accounting authority in both modes

A canonical installation-account SQLite accounting database is owned by `retold-app`, not the optional
studio and not the rebuildable library index. It lives outside workspace-controlled roots and
binds every registered workspace; see product spec §8.1. Foreground commands, studio jobs, tests and
library generation use the same transactional reservation API. No provider dispatch occurs
outside it; §21 extends this boundary to subscription and local resource reservations as well. Reuse current conservative reservation semantics while replacing per-file in-memory
coordination with cross-process database transactions and durable request identities.

Before dispatch, atomically reserve a conservative maximum against command/job, cumulative
video, optional series, day and month ceilings. Unsupported/unbounded quotes block paid dispatch.
Use integer currency micros. Persist request identity before network I/O; settle only after output
and response metadata commit. A failed/ambiguous network response retains its hold; no automatic retry/fallback occurs until
saved-result recovery or explicit reconciliation resolves that attempt. The explicit duplicate-risk
exception in §21 requires fresh bounded authority and keeps the original hold; --yes is insufficient.
Settlement/release is idempotent. Explicit billed failed attempts remain charged.

Periods use the configured IANA timezone (default UTC), recorded per reservation. Attribute cost
to reservation start; an unresolved hold also reduces current-period availability after rollover.
Reconciliation clears that carried hold exactly once. Timezone/cap changes never erase incurred
costs or holds, and lowering caps blocks new reservations when already consumed. Configuration
changes serialize with reservations. The database is backed up and migrated with a journal;
missing/corrupt accounting fails closed for paid work and never rebuilds by guessing from outputs.

### 19.3 Execution ownership and control

All mutations first join the account maintenance/admission barrier (product spec §8.2),
which precedes asset/series/run locks and accounting transactions. Read-only inspection does not.
A canonical run path/ID is protected by one OS-backed exclusive lock held for the full mutation
lifetime. No timeout can steal a live lock. A studio singleton lock protects its socket and queue;
socket directory is user-only, peers must have the same UID, and control messages are schema
validated. The existing foreground owner finishes/stops before the studio may acquire that run.
Foreground clients seeing a studio route mutations to it; if unavailable/busy they fail, never
silently run a second writer. Retry admission uses an idempotency key persisted before enqueue;
identical keys return the same job, and a different continue for a busy run returns that job.

Job transitions: `queued → running → completed|blocked|failed|stopped`;
`running → pausing → paused → queued`; `queued|paused → stopped`.
Pause stops admitting new steps and drains in-flight work; stop cancels cancellable workers,
persists completed results and retains ambiguous paid holds before releasing ownership.
After studio death, incomplete jobs are recovered as `stopped` once OS locks are available;
no automatic paid restart. Late results cannot commit without the current owner/job generation.
Resuming a repaired branch while other work runs is a control request to the existing owner;
no parallel invocation acquires another run writer. Editing waits for the affected work to drain,
revalidates the plan and cannot mutate in-flight inputs. Detached clients do not cancel jobs;
foreground Ctrl-C requests stop and cleanup. Event records have durable job ID and sequence;
reattach replays after the last seen sequence before live streaming, without executing work again.

### 19.3.1 Shared series ownership

Run locks do not protect shared series state. Both foreground and studio use the same canonical
series ID and OS-backed registry lock for metadata transactions. Shared asset files and version
manifests are immutable after publication. A registry transaction writes and fsyncs new files,
then atomically publishes a versioned registry snapshot; readers see either complete snapshot.
Recovery ignores/quarantines unpublished staging files and never removes a referenced version.

Generation is deduplicated by `(series ID, asset ID, semantic inputs hash)`, protected by an
OS-backed asset-generation lock and durable attempt/result record. One process owns generation
through dispatch and publication; a contender waits or reuses its verified committed result.
After a crash, unresolved paid holds block dispatch; committed staged output is recovered and
published before considering another attempt. No blind retry or second shared paid generation.

Lock order is asset-generation lock → registry lock. Registry-only operations take only the
registry lock; they never wait for asset locks or run locks. Neither shared lock may be acquired
while holding a run lock. Registry transactions release their lock before network work, and
accounting transactions never wait for filesystem locks or network I/O. Commands needing shared
preparation complete it before acquiring a run lock. A running owner needing new shared assets
first drains affected work and checkpoints/releases run ownership; it then prepares shared work
and reacquires its run lock, revalidating job generation and inputs before resuming.

Part admission captures one complete registry snapshot under the registry lock and retains its
immutable manifest/version IDs. After releasing that lock, it acquires the run lock and commits
those exact pins; no read of `latest` occurs during execution. Concurrent series updates publish
new snapshots and never mutate existing pins. Existing parts retain their inputs and receive
stale notices; adopting new versions requires a separate plan under normal approval rules.
`used_by` is a rebuildable reverse index from authoritative pinned run manifests, never an input
to generation, deletion or authorization. Updates cannot garbage-collect shared versions while
admitted/running parts may reference them; version deletion is outside this rewrite's scope.

Required races: two foreground parts, foreground plus studio and concurrent `series update`;
assert one generation per shared input key, no lost registry publication, unchanged admitted
pins and safe recovery when killed between artifact commit and snapshot publication.

### 19.4 Artifact review and regeneration

Approval and locking are one atomic action in CLI, TUI and HTML. `approve` (alias `lock`)
records reviewed content hash and pinned input versions. `unlock` explicitly removes protection
and revokes approval; it does not itself regenerate. Upstream changes mark protected descendants
`stale_locked`; continue retains them and blocks dependent new work until the user chooses:
keep the exact pinned versions (recorded acknowledgement), or preview and authorize targeted redo.
Redo invalidates approval on the affected set, creates new versions and requires fresh review.
Neither `--yes` nor ordinary resume unlocks anything. Unlocked items follow dependency changes.
Paid voice requires an approved current script and cues; stale/edited scripts block new voice.
Editing one shot only invalidates that frame plus true descendants such as assembly; editing a
line invalidates its sentence group and alignment/timing descendants; narrative meaning changes
also invalidate mapped translations/visuals/cues. Only demonstrably unaffected images are reused
(runtime contract §5). Editing commits no generation and every edit checks the target base version.

### 19.5 Noninteractive and machine contracts

Non-TTY stdin/stdout, `--plain` and `--json` never prompt, open an editor or launch a TUI.
Bare noninteractive invocation prints usage (a typed result for JSON) and exits successfully.
Missing required inputs or required confirmation return `RG-INPUT-REQUIRED` with exit 2;
use explicit flags/`--yes` for confirmations, within existing authority. Conflicting output flags
return usage errors. Default mutating refs are never inferred in scripts or interactive commands.

Every JSON stdout line is a versioned NDJSON envelope with `schema_version`, `type`,
`request_id`, optional `job_id`/`sequence`, and typed payload. No ANSI, notices, provider output
or prompts appear outside envelopes. All invocations emit exactly one terminal result/error,
including parser errors: completed, queued, detached, blocked or failed, while stdout is writable.
A broken pipe cannot guarantee terminal delivery; query durable job state without resubmitting. Queued/detached are
client outcomes, not job completion. Exit 0 means the requested action succeeded or was accepted;
exit 2 is input/usage, 3 is blocked authority/review/budget, 4 is execution failure.
Transport loss exits 4 with the known job ID; querying it does not resubmit the request.
`video show` is a bounded snapshot; `video logs --follow` streams until detach/job termination.

### 19.6 Required regression matrix

Before cutover, record passing evidence for: concurrent foreground/studio/test/library caps;
rollover with unresolved holds; kill after reservation/dispatch/output commit; idempotent billing
reconciliation; old approved runs resumed at $0; `--yes` and legacy flag boundaries; export-stage
rejection; setup with zero exports; duplicate enqueue and ownership races; stale workers; repair
inside a live owner; event replay; repeated/interrupted migration with verified/unknown cache
entries, rollback and preserved billing; every inventory legacy command; identical artifact
transitions across CLI/TUI/HTML; and closed-stdin/JSON prompt-refusal and schema-valid terminal
errors. Tests use fake providers, temporary roots and fake clocks. Real paid calls and video
exports require separate current user authorization; review approval authorizes implementation
planning only and is not evidence that these tests have passed.


## 20. Expanded product specification and required features

[CLI product specification](cli-product-spec.md) is normative for the expanded rebuild. It adds
24 required feature groups, five full production journeys, a terminal design system, canonical
command grammar, versioned workspace/run/delivery folders, output schemas, safe backup/restore,
resource controls and feature-level completion evidence. Every feature maps to §17 phases;
§19 remains authoritative for authorization, reservations, locks and migration.

Structured output means both predictable NDJSON for programs and navigable, verified folders
for people. New v2 layouts do not silently relocate legacy runs or override guide folder rules.
UI polish includes states, keyboard behavior, terminal restoration and multilingual text;
approval requires actual visual/PTY evidence when implemented. No promised feature may ship as
a placeholder or a success response over an unimplemented use case.

Revision 3 review approval predates this expansion. Obtain new architecture and product reviews
of this document, its product specification and the companion before marking the expanded revision approved.


Senior UI/UX refinement (revision 6): product spec §4.1–4.8 defines root/object navigation,
text-entry key scope, modal focus and recoverable drafts, composite state/actions, safe blocker
choices, release-scoped human review, an equivalent linear/external review path and participant
task-quality gates. These refine F01–F24 and are mandatory at their assigned phases; historical
revision 5 approvals do not certify these additions or any rendered implementation.


## 21. Resource efficiency and quality contract

[Resource efficiency and quality contract](cost-quality-contract.md) is normative for subscription
usage, quota-account identity, every-call recovery, bounded generation/judging/repair trees,
minimal payloads, checker/provenance reuse, quality-constrained routing, audio units and waste
measurement. It refines §19 at the existing accounting/store/router seams, not a new service.

Target <1% preventable waste per measurable resource dimension, not a guarantee over opaque
provider billing. Deterministic duplicate/local-validation bypass cases must reach zero dispatches.
Do not relax factual/identity/voice/QA floors to meet a usage target. Real efficacy/coverage remains
unverified until separately authorized provider evaluation; fake tests cannot establish savings.

Its §10 maps required work into the same ten phases and all F01–F24 gates. Revision 6 senior UX
contracts remain intact; quota/unknown-outcome actions follow the same focus, human-language,
accessible-path and separate approval/allowance/export rules. Obtain a fresh cost-quality verdict
before finalizing this expanded revision.


## 22. Runtime, editing and integration invariants

[Runtime and editing contract](runtime-editing-contract.md) is normative for bounded event delivery,
incremental index freshness, aggregate physical admission, supervisor/orphan recovery, fair lazy
queueing, semantic edit dependencies, anchored per-edition timing, expected-target commits,
read-only dry-run and browser review trust boundaries. Its §8 assigns phase owners and acceptance.
It refines these invariants without removing prior resource, quality, UX or explicit-authority gates.
Five-field review findings and same-revision approvals: `reviews/multidiscipline-plan-review.md`.


## 23. Production-stage evidence and quality gates

[Production stage contract](production-stage-contract.md) is normative for exact-work draft scope,
retrieval-first multi-source research, captured event/witness/knowledge/reveal records, clause
entailment, coherent shot participants, used look-state generation, actual-pixel semantic QA,
critical speech/caption meaning and cached cross-artifact readiness. It specifically prevents
merging a car-disposal event, body-concealment event and later police sighting into one false scene.

API/HTTP scraping, deduplication, passage indexing and cache validation do collection locally;
bounded AI handles necessary semantic extraction/reconciliation/writing/QA, not routine page reading.
A URL/source count is not evidence of entailment. Work/version/source ambiguity blocks the affected
stage before costly generation; all prior resource, quality, edit and explicit-export gates hold.
Its §10 maps existing owners/phases and regressions. Historical revision 9 panel approval predates
this new flow refinement; no original generated image/film frame was inspected or corrected here.


## 24. Evidence-led algorithm strategy

[Algorithm strategy](algorithm-strategy.md) specifies conservative MinHash grouping, language-aware
BM25/exact-entity retrieval, qualified RRF/diversity/coverage packing, gap-directed source collection,
exact dependency-slice reuse, bounded constraint-guided planning/layout and quality-qualified check/
route policies. These extend existing `retold-algo`/engine/app seams; no new autonomous research
service or unrestricted self-improvement loop.

Mandatory quality/critical-evidence safeguards and finite evaluation are required; candidate
algorithms are adopted only after measured per-task benefit with no hard-quality regression.
An inconclusive candidate leaves the qualified baseline in place. Its §6 maps the existing phases,
and its §5 requires held-out ablation/usage/quality evidence before claiming smarter/cheaper output.
All earlier stage, factual, quota, recovery, UX, secure-review and explicit-export gates remain.
