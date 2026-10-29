# Plan: the full Retold TUI

Status: complete, 2026-10-23. The full gate passes on macOS and in the network-isolated Linux acceptance container (idle hosts, render budgets unchanged); human P9 readability accepted. Extends and supersedes `plan-cli.md` §6 ("Interactive app").

### Final acceptance pass (2026-10-23)

The current pass closes native Linux FilmCraft selection, editor shutdown preservation and
terminal background ownership. Linux exports now use `filmcraft` on PATH; macOS retains its
app executable. Doctor inspects the same native command. Both Linux production journeys
passed with the official checksum-verified ARM64 FilmCraft 0.9.0 under Xvfb/software graphics,
including the TUI series-part → review → ready → rendered-video flow.

A stalled editor can hold unsaved changes through SIGINT/SIGTERM/SIGHUP; Retold waits for
its exit, then restores terminal modes. It does not force-kill an editor or discard its draft.
A real xterm pass at all three sizes found that Dark/Light inherited terminal backgrounds;
colour themes now paint their canvas and modal backgrounds, while No colour preserves the
terminal defaults. Real Linux xclip copying and an ffplay process smoke check also passed.

Twelve live release captures (Home, Providers, a stepped form and confirmation at all three
sizes) were prepared for human review. They are ANSI-cell renderings, distinct from the real
xterm screenshots. The user accepted readability at all three sizes on 2026-10-23. This is a capture
readability review; it does not establish audible quality or production-provider quality. The reliability audit retains the original intermittent
studio observation rather than claiming that passing stress repetitions explain its cause.

### Implementation checkpoint

The foundation now includes an eleven-section responsive shell, command palette, generated
forms for runnable CLI commands, field help, four themes, Unicode input, mouse navigation,
background read/action lanes, cached refreshes, and a getting-started checklist. Providers
can be added and edited with typed CLI options as well as TOML files. Forms validate through
the CLI parser before confirmation and preserve their inputs when confirmation is cancelled.
Parity, reducer, worker, breakpoint snapshot and PTY tests cover this foundation.

Dedicated flows now include stepped provider setup, onboarding, file browsing and previews,
editor handoff, reviewed spend plans, saved image/audio review, and studio event streaming.
These extend the foundation; implementation presence does not establish complete acceptance
of every planned journey. Automated coverage includes command parity, confirmation cancellation,
worker isolation, Unicode input, responsive rendering snapshots, and PTY journeys including provider creation, route editing,
and read-only health/login inspection. End-to-end production journeys, platform-specific editor/audio/clipboard
behavior still require separate proof; automated host render budgets are covered below. The authorized local acceptance fixture now covers project export through the TUI and rendered
video export through the CLI; no production provider is exercised.

### Offline acceptance fixture recovery (2026-10-22)

The local OpenRouter stand-in now answers delivery thumbnail-ranking requests with all
three candidates, ordered by score. Its regression deserializes the answer using the engine's
actual review type. Character-review workflow coverage proves that current-sheet approval
allows continuation and a changed sheet hash invalidates approval. These checks do not close
the HTTP-fixture draft-to-export journey or establish real-terminal acceptance.

### Remaining implementation and acceptance work

The P0–P8 implementation and automated journeys are present. P9 capture readability was accepted by the user at all three sizes. Final automated
platform verification is recorded below. The phase-by-phase evidence and qualification
limits below distinguish implementation from acceptance:

- P1: done. A PTY journey starts from an empty provider file, opens guided setup (F2), adds a
  provider through the stepped form, lets setup advance to routing by itself and accepts
  suggested routes. `retold providers route --suggest [--dry-run]` fills only unrouted tiers
  from the configured providers' kinds (subscription CLIs first, only selectors the provider
  defines; paid models, voices and transcription stay a person's choice). Ctrl-S continues a
  stepped form or runs it from any field. Discovery drafts, the Routes editor, and the Health
  and Login tabs remain as described; none of them proves a provider can run production work.
- P2: implemented saved limits/attempts tabs, scoped limit editor, cursor pagination,
  read-only financial diff previews, guarded reconciliation and spend bars. Automated PTYs
  cover limit cancellation and application. Processes now has a read-only selectable owner
  list, identity/evidence details, filtering and independent cursor pagination. Backups explains
  account recovery scope and offers reviewed creation and offline folder verification. PTYs
  prove cancellation preserves accounting and confirmed creation verifies successfully.
  macOS and Linux fixture coverage are recorded in the final acceptance pass; backups are
  chosen by path, with no saved inventory.
- P3: implemented structured guide/channel document reading and typed existing-field editing,
  schema/template validation, stale-hash rejection and sidecar preservation. PTYs cover guide
  cancellation and saving. Explicit bundled story-recap selection now works without a source
  checkout; F3 opens fixed-choice pickers. Lists, including empty lists, can be replaced with
  schema-validated arrays. A fresh-HOME PTY creates an example guide and explicit channel,
  then validates both in the TUI with no provider invocations. Picker snapshots cover all three
  breakpoints. Schema-declared omitted fields with existing parents can be added through
  typed review; the list marks them not set and the editor labels its input as a suggestion.
  Missing object sections are offered with their required members filled, and every offered
  section of the bundled example guide and channel previews as valid. List items can be
  appended, removed and moved (`J`/`K`), each as a reviewed whole-list change. P3 is done.
- P4: implemented explicit bulk archive/trash selection and reviewed dispatch, plus line/shot
  edit shortcuts. Persisted per-guide/channel draft defaults, reviewed bulk tagging and saved delivery coverage
  are implemented. The draft review lists each tier the guide's roles use with its provider
  chain and flags unrouted tiers (backed by `routes` in the `new --dry-run` payload). A PTY
  creates a draft from the Videos tab through that review and checks the saved run on disk.
  A PTY opens export from the video workspace, sees the Spend Gate's dry-run plan and its
  "not saved at ready" blocker, and confirms Enter cannot apply it and no files change.
  The loopback HTTP fixture covers draft → character review → ready → rendered MP4 through
  the real TUI and studio, including a linked series part. ffprobe checks video and audio streams
  and positive duration. A separate real CLI journey exports both a project and an MP4.
  Providers use zero-price local stand-ins; media stages use real host tools. Cost
  estimates appear in the Spend Gate for continue/export plans; a draft has no plan to estimate.
- P5: implemented pending/stale/immutable artifact filters and saved hash/size comparison.
  Pinned saved-image comparisons and failed-status filtering are implemented. A real PTY
  records a note, approval and unlock while preserving image bytes. Language cycling now
  shows its selected edition, explains empty results and clears obsolete previews; reducer
  coverage checks selected action targets and late-worker rejection. Redo derives its stage and
  item from the saved `work/NN-<stage>/` path and shows a dry-run of what rebuilds before applying;
  `e` opens the shot edit form for the selected frame. macOS `afplay` passed a silent-WAV
  process smoke check. Linux ffplay also exited successfully using an SDL dummy device;
  audible quality is outside this automated evidence.
- P6: implemented pause/unpause/stop and neighbour reorder controls, with reducer coverage
  rejecting running neighbours. Real PTYs start and stop the studio through reviewed controls,
  execute a queued research job and attach to its saved events, and pause then unpause a job
  held by a slow local fake provider. P6 is done.
- P7: implemented saved series parts selection, part plan/run/add-parts controls, video
  navigation and safe Music/SFX row playback. The parts matrix colours each stage by its saved
  decision (cached, needs repair, not run) under a completion gauge. Music detail reports
  coverage of the director's score moods (`SCORE_MOODS`) from track tags. PTYs add a series
  part through the reviewed update and import a music track, then show its mood coverage.
  The production PTY runs a linked part from Series through character approval to ready,
  then exports its video. Completion is observed from verified saved timeline/delivery evidence;
  invalid delivery evidence clears completion. Queue submission alone never marks a part complete.
- P8: implemented dedicated health/storage reports and explicit repair/backup/verify controls.
  Creative result rendering and reviewed restore/activation previews are implemented;
  A PTY reviews a restore, leaves the restored workspace inactive and then activates it.
  System → Diagnostics browses every code and opens `explain CODE` read-only. P8 is done.
- P9: added money/document/series snapshots, loading/empty/failed list snapshots (which led to
  wrapping list diagnostics and hiding the "add the first one" hint until data loads), a
  1,000-video render budget regression, mouse-wheel ownership coverage, a sweep clicking every
  field of every generated form, palette row clicks, [terminal documentation](tui.md) and a
  README capture backed by a snapshot test. Confirmation buttons share the keyboard dispatch
  gate. PTYs cover editor handoff (F6 from the file browser, then the guide check), audio
  preview, file opening and clipboard copy through recording stand-ins on `PATH`. That work fixed
  F6 being swallowed by the global handler, filter letters triggering open/play, and a resume
  that failed when the terminal did not answer a cursor-position query. Linux clipboard falls
  back from `wl-copy` to `xclip` to `xsel`; real xclip copying passed in isolated X11.
  Linux xterm launch/exit and mode restoration passed at all three sizes under Xvfb.
  The user accepted capture readability at 80×24, 120×40 and 200×60. Terminal.app 2.15
  smoke checks cover all three
  breakpoints and Ctrl-C terminal-mode restoration; GNU Screen 4.00.03 covers launch, navigation
  and exit on this host. These are automated real-host smoke checks, not a human usability study.
  The host-program follow-up uses bounded readiness waits
  and atomic shim records; clipboard navigation retains a short delay because terminal redraws
  may omit unchanged text. All 30 PTYs passed at default concurrency on 2026-10-22, including
  editor/audio/clipboard/viewer coverage; this run did not reproduce the earlier timeouts.
The new flows preserve the existing in-process command contract. No production provider or
export readiness is implied, and the full P0–P9 acceptance criteria below remain authoritative.

## 1. Goal

Everything Retold can do, you can do from `retold` with no arguments, in a full-screen
ratatui app that feels designed, not generated. The CLI remains for scripts, agents and
`--json`; the TUI becomes the primary way a person uses the tool.

"Everything" is measurable: the CLI today has **124 leaf commands** in 22 groups. Every one gets
a TUI route, and a test fails the build if a new command is added without one (§4.4).

### Baseline before this plan

| | Today | Target |
|---|---|---|
| Screens | Home, Video (read-only), Providers (read-only), Spend, text Detail | ~30 screens in 11 sections + onboarding |
| Actions | 0 write actions; `c`/`t`/`l` print "run this from the command line" | All 124 commands, with guided flows for the important ones |
| First run | Empty Home, "No saved videos yet", config errors | Onboarding wizard from zero to first video |
| Event loop | Blocking `event::read`, synchronous queries on the UI thread | 60 fps tick loop, background workers, live studio events |
| Layout | Fixed header/body/footer | Responsive shell: sidebar, main, inspector, modals, toasts |
| Input | Lists and scrolling only | Forms, pickers, file browser, ordered lists, editor handoff |

## 2. Principles

1. **Complete parity.** Every CLI command is reachable, and the footer always shows the exact
   CLI command for what you are about to do. Running that command does the same thing.
2. **One use-case layer.** The TUI calls `retold-app` in-process, the same functions the CLI
   calls. No shelling out to `retold`, no second implementation of business rules.
3. **Never block.** Every query and action runs on a worker thread; the UI stays responsive and
   shows a spinner. Long work (continue, export, series run) runs in the studio and streams
   progress live.
4. **Safe by design, not by nagging.** Reads are instant. Writes show what will change. Anything
   that can spend money goes through one Spend Gate (§7). Destructive actions default to Trash.
5. **Guided, then fast.** First-time flows are wizards with examples and defaults; repeat use is
   keyboard-fast (palette, mnemonics, remembered choices).
6. **Every state is designed.** Empty, loading, error, partial and success states are drawn on
   purpose, each with the next useful action.
7. **Testable.** State stays a pure reducer (`App::press` → `Effect`), as today; every screen
   has snapshot tests at three sizes and every flow has a PTY test.

## 3. Information architecture

A persistent left sidebar with 11 sections. Numbers jump directly; `Tab` cycles focus.

```
 1 Home         dashboard, continue where you left off, alerts
 2 Create       new video, new series, from a template
 3 Videos       library table → video workspace (tabs)
 4 Series       series list → parts matrix
 5 Studio       background jobs, queue, live progress
 6 Library      assets, music, sound effects
 7 Providers    providers, routes, health, logins, tests
 8 Money        spend dashboard, account, limits, attempts, backups
 9 Configure    guides, channels, export presets, saved views, profiles
 0 System       workspaces, storage, doctor, config, diagnostics
 ⌘ Tools        creative audits and prompt compiler
```

### 3.1 Shell layout

Responsive breakpoints: **wide ≥ 140 cols** (sidebar + main + inspector), **normal 100–139**
(sidebar + main; inspector becomes a toggle), **compact < 100** (sidebar collapses to icons,
single column), **minimum 80×24** (still fully usable).

```
╭─ Retold ─────────────── ws: ~/Retold ─── claude ✓  codex ✓  openrouter ✗ ── $0.42 today ─╮
│ ▌Home        │ Videos · 12                                    / filter   ⇅ updated │ Drishyam 2 │
│  Create      │ ─────────────────────────────────────────────────────────────────── │ ────────── │
│ ▌Videos    ◀ │ ▸ Drishyam 2           hi,en  storyboard ▓▓▓▓▓░░░  $0.31   2m ago   │ Stage  6/11│
│  Series      │   Drishyam             hi     ready      ▓▓▓▓▓▓▓▓  $1.28   yday     │ Spent $0.31│
│  Studio   ●1 │   The Conclusion       hi     blocked ⚠  ▓▓▓░░░░░  $0.12   Mon      │ Ceil  $1.00│
│  Library     │                                                                     │ Next:      │
│  Providers   │                                                                     │ continue → │
│  Money       │                                                                     │ ready      │
│  Configure   │                                                                     │            │
│  System      │                                                                     │ [⏎ Open]   │
│  Tools       │                                                                     │            │
├──────────────┴─────────────────────────────────────────────────────────────────────┴────────────┤
│ ⏎ open  n new  c continue  e export  a archive  t tag  / filter  : palette  ? help              │
│ $ retold video plan drishyam-2                                                                  │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
```

- **Header:** workspace, provider health badges (live), today's spend, studio job count.
- **Footer line 1:** the keys available *now*, context-sensitive and never more than one line.
- **Footer line 2:** the exact CLI equivalent of the selected action, copyable with `y`.
- **Toasts:** bottom-right, auto-dismiss after 4 s, errors stay until dismissed.
- **Modals:** confirm, Spend Gate, pickers and forms render centered over a dimmed background.

### 3.2 Global keys

| Key | Action |
|---|---|
| `1`–`9`, `0` | Jump to section |
| `:` / `ctrl-k` | Command palette: fuzzy search over all 124 actions plus navigation |
| `/` | Filter the current list |
| `?` | Keys for this screen (generated from the same keymap the footer uses) |
| `Esc` | Close modal / back |
| `Tab` / `shift-Tab` | Cycle focus between panes |
| `y` | Copy the CLI equivalent to the clipboard (OSC 52) |
| `ctrl-r` | Refresh |
| `T` | Cycle theme |
| `ctrl-c` | Quit (asks only when a foreground action is running) |

Mouse: click to select or focus, double-click to open, wheel to scroll, click tabs and buttons.

## 4. Architecture

### 4.1 Modules (in `crates/retold-cli/src/tui/`)

```
tui/
  mod.rs            run(), terminal setup/restore, panic hook
  shell/            router (screen stack), sidebar, header, footer, toasts, modal layer
  runtime/          event loop (crossterm poll + 16 ms tick), worker pool, effect executor,
                    studio event subscription, clipboard, $EDITOR handoff, file opener
  actions/          action registry: one entry per CLI leaf (§4.3), parity test
  components/       reusable widgets (§4.2)
  screens/          one module per screen (§6)
  onboarding/       first-run wizard (§5)
  theme/            design tokens, themes, glyph sets
```

`retold-app` gains nothing UI-specific; any missing typed API is added there so the CLI
benefits too (§8).

### 4.2 Component kit

Built once, used everywhere, each with snapshot tests:

- **Table:** sortable columns, filter, multi-select (`space`), sticky header, column priority so
  narrow terminals drop the least important columns first.
- **Form:** fields with label, help, inline validation, and a live CLI preview. Field types:
  text, number, money (USD with cents), path (with completion and a file browser), select,
  multi-select, toggle, key=value list, ordered list (`J`/`K` reorder), language/edition picker,
  secret (hidden; stored via Keychain when supported).
- **Wizard:** a stepper over forms, with a progress rail, back/next, skip, and resume.
- **Pickers:** fuzzy picker for videos, guides, channels, providers, assets, and jobs.
- **File browser:** a two-pane directory browser with filter and a preview pane.
- **Progress:** stage pipeline (parallel branches side by side), bars, spinners, ETA.
- **Charts:** sparkline and bar chart (spend by day/provider), gauges (limit used vs ceiling).
- **Log stream:** a virtualized live log with level filter, follow mode, and search.
- **Viewers:** TOML/JSON with syntax highlight, diff view (before/after for edits), Markdown.
- **Image preview:** `ratatui-image` (Kitty, iTerm2, Sixel), falling back to half-blocks; `o`
  opens the full file in the system viewer.
- **Audio preview:** `space` plays a voice line, music track or SFX via `afplay`/`ffplay`.
- **Status chips:** consistent badges for ok/warn/error/locked/stale/running/paid.

### 4.3 Action registry

Every CLI leaf becomes one typed `Action`:

```rust
struct Action {
    path: &'static [&'static str],     // ["video", "continue"]
    title: &'static str,               // "Continue video"
    section: Section,                  // Videos
    risk: Risk,                        // Read | Write | Spend | Destructive
    form: fn(&Context) -> FormSpec,    // fields, defaults, validation
    run: fn(&Context, &Values) -> Payload,  // in-process retold-app call
    render: fn(&Payload) -> ResultView,     // rich result, not raw text
}
```

The registry drives the palette, the "All actions" browser, contextual action menus (`.` opens
the actions available on the selected item), the footer CLI line, and the help overlay.

### 4.4 Parity guarantee

A test walks the clap command tree and asserts that every leaf has a registry entry, and that
the CLI string the TUI renders for given form values parses back to the same arguments. Adding
a CLI command without a TUI route fails CI. Until a command has a dedicated screen, the registry
renders a generated form from its clap arguments, so coverage is 100% from Phase 0 onward and
dedicated screens replace generated forms one by one.

### 4.5 Runtime

- The event loop polls crossterm with a 16 ms timeout and drains a result channel each tick; the
  UI thread never touches disk or the network.
- Worker pool (2 threads) runs queries and actions; each result carries the screen and request ID
  it belongs to, so stale results are dropped.
- Studio integration: the TUI subscribes to studio events for live job progress, in the same way
  `queue attach` streams. Starting long work uses `--background` semantics by default.
- `$EDITOR` handoff suspends the TUI, restores the terminal, opens the file, then validates it.
- Startup: the shell draws within 150 ms; data loads asynchronously with skeleton rows.

## 5. First run: onboarding wizard

Shown automatically when the installation is fresh (no workspace, no providers, no account),
and available any time from System → Setup. Each step is skippable, resumable, and shows its
CLI equivalent. Config folder and an empty `providers.toml` are created automatically (already
implemented, §10 P0).

```
╭─ Welcome to Retold ──────────────────────────────────────────────────────────────╮
│  ● Workspace   ○ Providers   ○ Routes   ○ Account   ○ Guide   ○ Channel   ○ Done   │
│                                                                                   │
│  Where should your videos live?                                                   │
│  ┌──────────────────────────────────────────────┐                                  │
│  │ ~/Retold                                    │  ⇥ browse                        │
│  └──────────────────────────────────────────────┘                                  │
│  Name   The Whole Plot                                                            │
│                                                                                   │
│  Creates: films/, series/, audio/music, audio/sfx, guides/, channels/, presets/   │
│                                                                                   │
│                                         [ Skip ]   [ Back ]   [ Continue ⏎ ]       │
╰─ $ retold workspace init ~/Retold --name "The Whole Plot" ──────────────────────╯
```

1. **Workspace:** location and name → `workspace init`, `setup`, `workspace switch`.
2. **Providers:** an offline scan lists what was found (claude, codex, gemini, llama, ffmpeg,
   FilmCraft, every `~/.claude*` / `~/.codex*` profile, API keys in env/.env, never printed).
   Tick the ones to add; each has prefilled, editable fields → `providers add`.
3. **Routes:** a suggested route per task (text fast/smart/best, vision, image, voice,
   transcribe) built from what you added, reorderable with `J`/`K` → `providers route`.
4. **Check:** login status per provider, and an optional free test (`--spend 0`) → `providers
   login --status`, `providers test`.
5. **Account:** `account init`, then limits from presets (Cautious $1/day · Normal $5/day ·
   Custom) with requests caps → `account limit`.
6. **Guide:** start from an example (story-recap, book-recap from `docs/examples`) or a blank
   scaffold; language, purpose, style, delivery, length → `guides create`, then `guides check`.
7. **Channel:** name, handle, language, accent colour (live swatch), voice provider/model/voice,
   fonts → `channels create`, `channels check`.
8. **Done:** a summary of what was set up, plus a "Create your first video" button.

## 6. Screens

Each screen lists its layout, its actions (key → CLI), and its states. Section numbers match
the sidebar.

### 6.1 Home
Dashboard cards: **Continue** (in-progress videos with stage bars and blockers), **Studio**
(running/queued jobs), **Attention** (items waiting for review, providers limited or logged out,
limits near ceiling, failed attempts to reconcile), **Spend** (today/month sparkline vs limit),
**Quick actions** (New video, New series, Import music, Run doctor).
- Commands: `home`. Fresh install → onboarding (§5). Empty library → big "Create your first video".

### 6.2 Create
**New video wizard** (`new` / `video new`): guide (cards with descriptions) → channel → work
title (+ year, kind, scope, named guide inputs, source file) → editions (Hindi voice + Roman Hindi
captions, Urdu voice + English captions, … with plain-language examples) → quality (lean /
standard / premium, with what each changes) → budget ceiling → **summary with `video plan`
preview, cost estimate and providers used** → *Create draft* or *Create and continue* (opens the
Spend Gate). Defaults remembered per guide and channel. `--dry-run` = "Preview".

**New series wizard** (`series new`): guide, work, kind, channel, parts, budget → summary.
**From template:** copy an existing video (`video copy` with title/lang).

### 6.3 Videos
**Library table** (`video list`): title, editions, stage, progress, spent/ceiling, updated, tags,
status chip. Filter, sort, saved views (`views list/show/save/remove`), archive toggle, tags
filter. Multi-select for archive/tag/delete.

**Video workspace** (opened from the table) with tabs:

| Tab | Content | Actions → CLI |
|---|---|---|
| Overview | Identity, editions, pipeline graph, blockers with specific fixes, next step | `c` continue, `e` export, `.` all actions — `video show` |
| Plan | What continue would do: stages, ceilings, routes, plan hash, until-stage selector | `video plan --until … --spend … --max-requests …` |
| Review | → §6.3.1 | `video review/approve/unlock/note/redo` |
| Script & Shots | Line and shot lists with inline edit forms and diff preview | `video edit line/shot/cast/budget`, `video undo` |
| Costs | Per-stage/provider spend, unresolved attempts, reconcile form | `video costs [--reconcile --request --cost]` |
| History | Timeline of plan/edit/review/export events, filter by kind | `video history --kind` |
| Files | Work, delivery and state paths, sizes, verify, clean, open in Finder | `video paths/verify/clean` |
| Deliver | Editions × formats × aspects matrix, export, package | `video export`, `video package` |

Row actions: `video copy/archive/unarchive/delete/tag`. Delete defaults to Trash; `--force`
requires typing the video name.

**Continue flow:** choose stop point (`--until`) → plan preview → Spend Gate (§7) → runs in
the studio with a live pipeline view (stage bars, current provider, cost ticking, log pane `l`,
pause/stop). Foreground mode is available but background is the default.

**Export flow:** edition, format, preset, aspect, project-only → dry-run preview → Spend Gate
when the export needs paid work → progress → "Open file" / "Reveal in Finder" / "Package…".

#### 6.3.1 Review
A grid of frames (inline images when the terminal supports them, otherwise a list with `o` to
open) and character sheets; filter pending / stale / failed, language, take.
Keys: `a` approve and lock, `U` unlock, `n` note, `r` redo with note (shows which descendants
rebuild, `--dry-run` first), `e` edit shot, `space` play the voice line, `←→` compare takes.

### 6.4 Series
Series list (`series list`) → series view: **parts matrix** (parts × stages, colour-coded),
budget gauge (`series show`). Actions: plan selected parts (`series plan --parts`), run with the
Spend Gate (`series run`), add parts (`series update --add-parts`), open a part's video workspace.

### 6.5 Studio
Daemon status card with start/stop (`studio status/start/stop`). Queue table (`queue list`):
job, video, stage, state, progress, started, ETA. Select a job for live detail (`queue show`,
`queue attach` streaming). Actions: pause/unpause/stop (`queue pause/unpause/stop`), reorder with
`J`/`K` or drag (`queue move --before`).

### 6.6 Library
- **Assets** (`assets list`, scoped by video/series): table + preview pane with provenance and
  takes (`assets show`), verify hashes (`assets verify`), "where used" tree (`assets uses`).
- **Music / Sound effects** (`library music|sfx list`): mood, tempo, loudness, licence, tags,
  commercial flag; `space` preview; `i` import via file browser with title/licence/attribution/
  tags form (`… import`); `t` tag (`… tag`); licence report (`… licence`); integrity check
  (`… check`). Coverage panel: which moods your guides need and which are missing.

### 6.7 Providers
Two-pane: provider cards (kind, health, login, parallelism, routes it serves) and detail.
- `a` **Add provider** form: kind picker, then only the fields that kind needs (tiers as a
  key=value list, profile folder picker with "found · logged in" hints, key env var), validated
  against the typed schema; no hand-written TOML → `providers add`.
- `e` edit (`providers edit`), `d` remove (blocked with an explanation if routed;
  `providers remove`), `l` login status / configure (`providers login`), `t` test with the
  Spend Gate (`providers test`), `s` rescan (`providers scan`).
- **Routes** tab: each task with its ordered chain; `J`/`K` reorder, `+`/`-` add/remove entries,
  validation rejects capabilities a provider lacks (`providers route --order`).
- **Health** tab: cooldowns, "limited until 2:45 PM", failure streaks (`providers health`).

### 6.8 Money
- **Spend** (`spend`): window (today/month/lifetime) × group-by (provider/video/series/day),
  bar chart + table, holds.
- **Account** (`account show`, `account init`): balances and every limit as a gauge.
- **Limits editor** (`account limit`, `account set-limits`): scope picker (account, workspace,
  video, series, provider) × window, USD + requests + extra resources, dry-run diff, remove.
- **Attempts** (`account attempts`, `account processes`): unresolved attempts with a guided
  reconcile form (`account reconcile --attempt --cost`).
- **Backups** (`account backup`, `account verify-backup`).

### 6.9 Configure
- **Guides** (`guides list/show/create/check/schema`): cards → structured guide view; create from
  example or scaffold; edit in a schema-driven form *or* `$EDITOR` with validation on return and
  a list of unfinished fields from `guides check`.
- **Channels** (`channels list/show/create/check/schema`): brand profile view with colour swatch,
  voice, fonts, music/sfx folders; same create/edit/check flow.
- **Export presets** (`export-presets list/show/new/remove`): a form instead of a JSON file.
- **Saved views** (`views list/show/save/remove`): saved from the Videos filter bar directly.
- **Profiles** (`config profile list/show/new/use`).

### 6.10 System
- **Workspaces** (`workspace list/show/check/init/switch`): list with active marker; switch
  reloads the app.
- **Storage** (`storage usage/backup/verify/restore/activate`): usage treemap/bars by video and
  kind with reclaimable space; backup/restore wizards; activation review with plan hash.
- **Doctor** (`doctor`, `doctor bundle`): checks with pass/warn/fail and "Fix" buttons; support
  bundle export.
- **Config** (`config validate/migrate`), **Setup** (re-run onboarding, `setup`),
  **About** (`schema`, `explain CODE` browser for every diagnostic code, `completions`, `man`).

### 6.11 Tools
Creative audits (`creative audit/beats/benchmark/dossier/bible/coverage/storyboard/audio/
optimize`): pick a video or paste/select text, see findings grouped by severity with jumps to the
offending line or shot. Prompt compiler (`creative prompt`): description + target/lighting/
optics/grade selectors with a live result.

## 7. Safety UX

| Risk | Examples | UX |
|---|---|---|
| Read | list, show, plan, scan, health | Instant, no confirmation |
| Write | add provider, tag, edit line, create guide | Inline confirm that shows exactly what changes (diff for edits) |
| Spend | continue, export with paid stages, series run, provider test | **Spend Gate** |
| Destructive | delete video, remove preset, restore/activate | Trash by default; permanent needs typing the name |

**Spend Gate** (one modal for all paid work):

```
╭─ Continue "Drishyam 2" to ready ─────────────────────────────────────────╮
│ Plan  7f3a…c21   stages: storyboard → images → voice → assemble          │
│ Estimate   $0.38 – $0.55     ceiling $1.00     spent so far $0.31        │
│ Providers  codex (images) · claude (text) · openrouter (voice, paid)     │
│ Limits     account today $0.42 / $5.00 ▓░░░░░░  video $0.31 / $1.00     │
│                                                                          │
│ Allow up to  [ $0.60 ]   max requests [ 200 ]                            │
│                                                                          │
│                    [ Cancel ]   [ Run in studio ⏎ ]   [ Run here ]        │
╰─ $ retold video continue drishyam-2 --until ready --spend 0.60 … ───────╯
```

- The allowance field never defaults above $0; free-only work proceeds with $0.
- The plan hash shown is the one submitted (`--plan-hash`), so what you saw is what runs.
- Idempotency keys are generated automatically, so a retry never double-spends.

## 8. Backend work the TUI needs

Things the CLI does through files or prints only as text, which need typed APIs in
`retold-app` (the CLI gains them too):

1. **Providers:** add/edit from a typed `ProviderConfig` (today only `--file`); scan results as
   structured "suggested definitions".
2. **Guides and channels:** read/write the structured document (not just scaffold and check);
   list unfinished fields with their paths.
3. **Export presets, saved views, account limits:** typed create APIs (today JSON files).
4. **Studio events:** a subscription API (shared with `queue attach`) for live progress.
5. **Review:** frame and character-sheet image paths, voice-line audio paths, take metadata.
6. **Structured results** for commands that only return text today, so screens render tables
   instead of paragraphs.
7. **Help text** for every argument (many are blank: `--until`, `--spend`, `--plan-hash`,
   `--yes`, `storage`, `config validate/migrate`, …): the forms and `--help` share it.
8. `publish` stays "not available yet" in both interfaces until it is built.

## 9. Visual design

- **Design tokens**, not ad-hoc colours: `surface`, `surface-raised`, `border`, `border-focus`,
  `text`, `text-muted`, `accent`, `success`, `warning`, `danger`, `info`, `paid`, `locked`.
- **Themes:** Dark (default), Light, High contrast, No colour (also via `NO_COLOR`). Truecolor
  with a 256-colour fallback.
- **Shape:** rounded borders; focused pane gets the accent border and a bold title; selection is
  a full-row highlight with a `▌` gutter mark; consistent 1-cell padding.
- **Glyphs:** a Unicode set (✓ ⚠ ✗ ● ◐ ▓ ⏎) with an ASCII fallback set for limited terminals.
- **Motion:** spinners and progress only; no decorative animation.
- **Text:** correct width for Devanagari and Nastaliq via `unicode-width`; Urdu shown right-aligned
  in line/caption views; truncation with `…`, never mid-grapheme.
- **Empty states:** an illustration-free one-liner plus the primary action ("No providers yet —
  press a to add one, or s to scan").
- **Errors:** RG code, plain message, the fix, and buttons for the fix and `explain`.

## 10. Phases

Each phase ends with tests green, snapshots reviewed, and a short demo recording.

| Phase | Scope | Done when |
|---|---|---|
| **P0 Foundation** | Config auto-creation; new shell (sidebar, header, footer, toasts, modals), 16 ms tick loop, worker pool, design tokens + 4 themes, component kit v1 (table, form, picker, wizard, file browser), action registry with generated forms for all 124 commands, parity test, palette over all actions | Every CLI command can be run from the TUI; parity test enforces it |
| **P1 Onboarding & Providers** | §5 wizard; §6.7 complete (add form, edit, remove, routes reorder, login, test via Spend Gate, health) | Fresh HOME → configured providers and routes without touching a file |
| **P2 Money** | §6.8: spend charts, account, limits editor, attempts/reconcile, backups; Spend Gate component | Limits set and spend inspected entirely in TUI |
| **P3 Configure** | §6.9 guides and channels (example/scaffold, schema form, `$EDITOR`, check), presets, views, profiles | A guide and channel created and validated in TUI |
| **P4 Create & Videos** | §6.2 wizards; §6.3 table and workspace tabs (Overview, Plan, Script & Shots, Costs, History, Files, Deliver); continue and export flows with live progress | Draft → ready → exported file from the TUI |
| **P5 Review** | §6.3.1 grid with inline images, approve/lock/unlock/note/redo/edit, audio playback | A full review pass without leaving the TUI |
| **P6 Studio** | §6.5 daemon control, live queue, attach stream, pause/stop/reorder | Background jobs monitored and controlled live |
| **P7 Series & Library** | §6.4 parts matrix and run; §6.6 assets, music, SFX with import, preview, tags, licence | Series run and music library managed in TUI |
| **P8 System & Tools** | §6.10 workspaces, storage, doctor with fixes, config, explain browser; §6.11 creative tools | No generated form left for a common task |
| **P9 Polish** | Responsive breakpoints, Light theme, mouse everywhere, accessibility (key-only, no-colour, screen-reader-friendly plain mode), performance budgets, docs and README screenshots | Usability pass on 80×24, 120×40, 200×60 |

Generated forms remain as the fallback for any command without a dedicated screen, so parity
holds at every phase.

## 11. Testing

- **Reducer tests:** every key on every screen, every wizard step (pure `App`, no terminal).
- **Snapshots:** each screen and state (empty, loading, error, populated) at 80×24, 120×40,
  200×60, in Dark and No-colour themes, via ratatui's `TestBackend`.
- **PTY flows:** onboarding on a temporary HOME; provider add → route → test (fake provider);
  new video → continue (fake engine) → export; delete → trash; Ctrl-C and panic restore the
  terminal.
- **Parity test** (§4.4) and round-trip test: the rendered CLI line parses back to identical
  arguments.
- **Performance:** first frame ≤ 150 ms; key-to-frame ≤ 16 ms on a 1,000-video library; no disk
  or network access on the UI thread (enforced by an architecture check).
- Existing `clean-code` gate, clippy, and `scripts/check.sh` continue to apply.

## 12. Decisions to confirm

1. **In-process calls (recommended)** vs. shelling out to the `retold` binary for each action.
   In-process gives live progress and typed results; shelling out is faster to build but
   text-only.
2. **API keys in the macOS Keychain** (recommended) or only via env/.env as today.
3. **Inline images** in Review via terminal image protocols (recommended; falls back cleanly).
4. **Background by default** for continue/export/series run (recommended), with "Run here" as
   the alternative.
