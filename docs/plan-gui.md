# Plan: Retold desktop GUI with GPUI Kit

Status: native workspace/draft, reviewed generation, script/character review, queue and export
slices implemented; full G0–G9 acceptance remains open.
Prepared: 2026-10-25. Repository inspected at `01f7471`, with existing local changes.
Companion: [interactive design concept](gui-concept.html).
Implementation checkpoint: [G0 native evidence and blockers](desktop/g0-checkpoint.md).
Continuation checkpoint: [shared contracts and native shell](desktop/g1-checkpoint.md).
Current navigation/presentation: [catalog, commands and preferences](desktop/g2-checkpoint.md).
Provider setup: [reviewed settings and offline observations](desktop/g8-providers-checkpoint.md).
Budget reporting: [installation spend and headroom](desktop/g8-budget-checkpoint.md).
Delivery review: [separate native export confirmation](desktop/delivery-checkpoint.md).
Character review: [saved-sheet approval and native execution](desktop/character-review.md).
Source and media: [native source-backed drafts and saved artifact playback](desktop/source-and-media.md).
Review notes: [versioned comments and retained native drafts](desktop/review-notes.md).
Full completion tracking: [G0–G9 implementation and acceptance ledger](desktop/completion-ledger.md).

Editing restart recovery: [retained script and note drafts](desktop/editing-recovery.md).

## 1. Outcome and delivery sequence

Build an additional native desktop application, provisionally called **Retold Studio**, for
creating, reviewing, continuing, and exporting narrated, illustrated videos. It uses Longbridge's
`gpui-kit`, backed by Zed's GPUI, and shares Retold's application contracts, saved work, accounting,
and daemon. The existing CLI and TUI remain supported entry points.

The interface should make a first video approachable and a large series manageable. Its defining
surface is a visual review workspace: the current frame, its narration, and the next useful action
together. Setup, execution, recovery, and delivery must receive the same design care as that screen.

Implement in these phases, each with its own acceptance gate:

| Phase | Deliverable | Depends on |
| --- | --- | --- |
| G0 | Prove toolkit, media, multilingual input, accessibility, and runtime feasibility | This plan |
| G1 | Typed desktop boundary, command coverage, daemon lifecycle, and stopping contracts | G0 |
| G2 | Design system, native shell, interaction foundations, and component gallery | G0; G1 contracts |
| G3 | First launch, workspace selection, setup, and saved production browsing | G1, G2 |
| G4 | Offline video/series creation and reviewed execution | G3 |
| G5 | Visual review, semantic editing, history, and targeted redo | G4; G0 media proof |
| G6 | Series continuity, shared assets, provenance, and version impact | G4, G5 |
| G7 | Activity, stopping/recovery, and bounded live observation | G1; integrate throughout G4–G6 |
| G8 | Provider, budget, authoring, storage, and advanced workflow coverage | G3; integrate progressively |
| G9 | Delivery, native packaging, platform qualification, and release usability | G5–G8 |

The first useful alpha is G0–G5 plus the essential G7 controls, with one complete draft → reviewed
run → review → ready → explicit export journey. That alpha is not full GUI completion. Full release
requires G0–G9, the capability matrix, and the acceptance journeys below.

## 2. Product brief and design checkpoint

| Question | Decision |
| --- | --- |
| Project stage | Existing Rust production engine and CLI/TUI; new desktop presentation |
| Users | Solo video creators; editors reviewing output; experienced operators managing series |
| Main objects | Workspace, video, edition, series, character, asset/take, job, guide, channel, delivery |
| Primary task | Turn source material into reviewed, exportable videos without losing work or budget control |
| Entry | Open an existing workspace or create one; return to the last meaningful selection |
| First decision | Continue an existing production or create a video/series |
| Information priority | Current work, blockers, next action, visible media; operational detail follows |
| Success | Understand the next step, review the actual artifact, and obtain a verified delivery |
| Recovery | Preserve drafts and saved output; explain the blocker; offer one relevant recovery action |
| Compact-window task | Review the selected shot and act; move navigation/inspector into accessible drawers |
| Temperament | Calm creative production tool with precise operational feedback |
| Foundation | GPUI Kit controls and behavior, with Retold tokens and custom media/storyboard elements |
| References | Zed for responsive native interaction; media-review tools for frame/context/action proximity; platform conventions for menus/dialogs |
| Primary vocabulary | Video, series, script, scenes, characters, activity, budget, export |
| Secondary diagnostics | Plan hashes, execution intents, artifact IDs, account attempts, physical claims, JSON |
| Main risks | Backend terminology dominating; too many panels; hidden spend; stale previews; attractive but incomplete screens |
| Verification | GPUI interaction tests, real pixel captures, packaged native journeys, assistive technology, task-based user trials |

Desktop is the release target. A narrow desktop layout is required; mobile and browser delivery
are separate future decisions, even though toolkit documentation describes those targets.

### 2.1 What "easy and powerful" means

- A new user can create a saved draft without configuring every provider or understanding stages.
- The main page always answers: **What is this? What state is it in? What can I do next?**
- A returning user resumes the same video, edition, selection, and useful view without hunting.
- Keyboard and pointer reach the same actions. Context menus and shortcuts accelerate visible flows.
- Expert controls live near their subject in expandable inspectors; the default workspace stays calm.
- A person can see the effect of a proposed edit or run before approving work that costs money.
- Errors retain context and inputs. Recovery does not require searching logs unless diagnosis needs it.
- All supported product capabilities remain reachable; common workflows have purpose-built screens.

### 2.2 Explicit boundaries

This is an additional GUI, not a rewrite of the engine or an automatic retirement of the TUI.
The first release is a production and review editor, not a new unrestricted nonlinear video editor.
Timeline changes are limited to semantics supported by shared application contracts. Multitrack
compositing, arbitrary keyframes, cloud collaboration, a plugin marketplace, and hosted rendering
are outside this plan. Exact renderer/editor handoff remains available for supported advanced work.

Planning and prototype work do not authorize live provider calls, restarting the stopped recap
jobs, rendering production videos, exporting, or publishing. The historical Drishyam request is
not an acceptance fixture. Use fictional content and pre-existing synthetic fixtures for design.

## 3. Verified repository foundation

These are current code observations, not claims that every future GUI contract already exists.

| Existing source | Reuse and implication |
| --- | --- |
| `crates/retold-app/src/lib.rs` | Presentation-free application modules; suitable boundary for another frontend |
| `workspace/context.rs`, `workspace.rs`, `library.rs` | Resolve managed and older layouts consistently; avoid independent GUI path discovery |
| `creation.rs` | Draft creation is offline; guide/channel/language/source inputs already exist |
| `plan.rs`, `continuation.rs`, `execution.rs` | Compile/revalidate plans, reviewed hashes, scoped authority, account admission, separate export |
| `studio/submission.rs`, `studio/types.rs` | Durable submission and idempotency; typed control requests and sequenced progress |
| `queue/types.rs`, `queue/store.rs` | Real queued/running/pausing/paused/stopping/stopped/completed/failed/blocked states |
| `review.rs` | Notes, approvals, edit history and redo concepts; several older edit handlers only record history, so actual semantic editing still needs implementation |
| `series.rs`, `series/execution.rs` | Linked parts, immutable version snapshots, shared characters, planned submission |
| `assets.rs`, `audio.rs` | Asset provenance/takes/usages and licensed audio inspection/import |
| `authoring.rs`, `configuration.rs`, `providers.rs` | Validated resource/configuration writing and provider setup |
| `account/`, `storage.rs` | Financial preview/revalidation, maintenance, backup/restore, activation boundaries |
| `crates/retold-cli/src/tui/` | Useful journey, vocabulary, state, and test references; frontend code remains terminal-specific |
| `scripts/check-architecture.py` | Currently only recognizes CLI reachability and existing crate layers |
| `scripts/check.sh`, `deny.toml`, `.github/workflows/ci.yml` | Existing gates and dependency-source rules must include GUI explicitly |

Important implementation gaps to resolve:

1. Application entry points mix typed inputs with generic `Payload`/JSON outputs. Define typed
   desktop-facing projections where needed; do not deserialize arbitrary JSON throughout views.
2. CLI command metadata and its dispatcher are private to `retold-cli`. The GUI cannot simply
   depend on the TUI catalog; define shared operation coverage without adding Clap to `retold-app`.
3. Daemon spawning lives in the CLI. A packaged GUI needs a reliable bundled worker launch path.
4. Studio client/server code imports Unix sockets and Unix filesystem APIs. Windows support is
   a backend portability project and cannot be inferred from GPUI's Windows support.
5. Current studio control exposes individual job operations and shutdown. An atomic, scoped
   "Stop all" contract and its receipt need shared application work, not a loop hidden in a button.
6. Inspect the actual implementation of indexes, resource scheduling, and edit-impact calculation
   against [runtime/editing invariants](runtime-editing-contract.md). That document includes planned
   requirements; its existence does not prove current implementation. Track gaps explicitly.

Preserve the pre-existing edits in `retold-app/src/discovery.rs`, `retold-engine/src/assets.rs`,
and `retold-engine/src/canon.rs`. This planning change does not own or validate those edits.

## 4. Toolkit decision and early feasibility gates

Research checked on 2026-10-25, using upstream sources. Recheck versions before implementation.

### 4.1 Dependency policy

Use **Longbridge `gpui-kit`**, whose current documentation is version 0.8.0. Its facade brings in
GPUI, Base, styled Component, and assets. The v0.8.0 manifest pins its upstream GPUI snapshot as
`gpui-pre = =0.3.9`. Use GPUI types through the facade to keep one compatible type/runtime graph.
Do not add an unrelated `gpui` release, track Zed `main`, or enable an alternate rendering backend
without a separate measured need. This meets the requested GPUI + GPUI Kit architecture.
[Kit architecture](https://gpui-kit.com/docs/),
[release manifest](https://raw.githubusercontent.com/longbridge/gpui-kit/v0.8.0/Cargo.toml).

Provisional dependency, to be confirmed in G0:

```toml
[dependencies]
gpui-kit = "=0.8.0"
retold-app.workspace = true

[dev-dependencies]
gpui-kit = { version = "=0.8.0", features = ["test-support"] }
```

Commit the resolved lockfile. Confirm released test-support APIs actually match the selected
version. Avoid shell/JavaScript extension runtimes, embedded WebViews, and optional speech/editor
features unless required by a concrete Retold workflow. Plain narration uses multiline input;
source-code editing belongs only in advanced resource authoring.
[Kit feature manifest](https://raw.githubusercontent.com/longbridge/gpui-kit/v0.8.0/crates/kit/Cargo.toml),
[Editor guidance](https://gpui-kit.com/component/editor/).

GPUI is still pre-1.0 with breaking changes. Isolate toolkit details behind local components and
pin upgrades to a dedicated compatibility pass.
[Zed GPUI README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md).

### 4.2 Toolchain and platform floor

Kit's current installation guide states macOS 15+, Windows 10+ with MSVC, and Ubuntu 24.04
dependencies with a working graphics session/driver. It recommends Rust 1.92+ for the locked
graph. Retold declares Rust 1.90; this host and current CI use 1.99.0. G0 must resolve and compile
the actual graph before choosing the desktop MSRV; update metadata/docs coherently, or give
`retold-desktop` its proven higher MSRV while separately testing the headless crates' minimum.
[Installation requirements](https://gpui-kit.com/docs/installation/).

Release order: macOS Apple Silicon first, Linux next, Windows only after shared backend portability
and native acceptance. Intel macOS is a separate qualified build, not a property of the ARM build.
Record minimum OS, architecture, graphics backend, driver, font set, and media dependencies per target.

### 4.3 G0 proof checklist

| Spike | Required evidence | If it fails |
| --- | --- | --- |
| Toolchain/dependencies | Locked sample builds; licenses/advisories/sources reviewed; no duplicated GPUI type graph | Resolve before application shell; no silent source-policy weakening |
| Window/input | Native window, resize, focus restoration, menus, clipboard, file dialogs, drag/drop, high DPI | Document missing platform adapter and implement it before dependent flows |
| Multilingual text | English/Hindi/Urdu mixed with numbers; shaping, selection, caret, grapheme deletion, wrapping, bidi, IME composition, undo | Fix adapter/input integration; do not ship broken target-language editing |
| Media | Existing synthetic audio/video opens inline, scrubs, seeks, pauses, closes; A/V sync and cancellation measured | Select another compatible media adapter within GPUI; external-open is a diagnostic fallback |
| Accessibility | Named controls, useful reading order, keyboard-only flow, actual VoiceOver operation | Repair semantics/focus; screen-reader access is a release gate |
| Execution bridge | A simulated slow query cannot block typing, navigation, or Stop; no extra daemon on launch | Separate control/work queues before feature work |
| Visual quality | Real GPUI captures at three sizes, light/dark, long text, busy/error/review states | Revise shell/tokens before multiplying screens |

The reviewed component catalog covers useful controls, tables, virtual lists, resizable/dock
layout, forms, diffs, and command UI. It does not establish a production media player for Retold.
Media integration remains an explicit engineering decision. Avoid conflating similarly named
third-party “GPUI kits” with the Longbridge dependency.
[Component catalog](https://gpui-kit.com/component/).

Fonts can be registered with the toolkit; choose and test suitable script fallbacks rather than
assuming the system font covers every edition. Custom media surfaces must provide independent
accessible controls and descriptions.
[Font integration](https://gpui-kit.com/docs/fonts/),
[Accessibility model](https://gpui-kit.com/docs/accessibility/).

## 5. Information architecture

Keep five primary destinations. Put global configuration below them, visually secondary.

| Destination | Main content | Primary action |
| --- | --- | --- |
| Home | Continue work, needs-review items, recent videos, recoverable interruptions | Continue the selected work / New video |
| Videos | Searchable saved productions, editions, state, recent activity; list and visual browse | Open video / New video |
| Series | Ordered parts, continuity, linked videos, shared material, delivery coverage | Open part / Add part |
| Library | Images, characters, audio, reusable references; provenance and usage | Inspect / Import supported asset |
| Activity | Jobs grouped by work, blockers, control state, useful progress, recovery | Review blocker / Pause / Stop |

Footer: **Providers**, **Budget**, **Settings**, **Help**. Guides and channels live under Settings
with contextual links from creation; do not expose an eleven-item infrastructure menu on first launch.

Global shell:

- Workspace switcher with clear active/read-only/inactive status; recent workspaces are explicit.
- Search/command entry, native app menus, compact activity summary, accessible Help.
- One main content area. Contextual inspector appears when needed; hide/reset layout is easy.
- Back/forward navigation and stable internal destinations including workspace, video, edition,
  shot, review item, series part, and job. These are internal routes initially, not public URL handlers.
- Restore selection, scroll, pane sizes, filters, and density; never restore a pending confirmation.
- A small shared job tray can show progress while browsing. Full logs open deliberately.

Default layouts are fixed and understandable. Add resizable splits first; offer optional docking
for experienced users only after default journeys pass. Persist a versioned layout and provide
**Reset layout**. Kit Dock is infrastructure, not a reason to make users assemble their workspace.
[Dock documentation](https://gpui-kit.com/component/dock/).

### 5.1 Video workspace

Use task-oriented tabs: **Overview · Script · Scenes · Characters · Audio · Export**.
History, notes, and technical detail are contextual panels rather than additional primary tabs.

- **Overview:** what exists, what needs review, current stage, editions, budget, and one next action.
- **Script:** readable sections with narration, source references, notes, approval, and impact preview.
- **Scenes:** selected-frame preview with narration and a horizontal sequence; grid overview optional.
- **Characters:** references, appearance versions, sheet review, and where each character is used.
- **Audio:** edition-specific narration lines, waveform, timing, pronunciation, music/SFX and licence state.
- **Export:** explicit edition/format/destination, readiness, previous verified deliveries, and export review.

Scene workspace geometry at approximately 1440 × 900:

```text
Workspace         Search commands                         Activity
Navigation | Video title / edition / saved state                  |
           | Overview Script Scenes Characters Audio Export      |
           |                                    |                |
           |       Selected frame               | Review/details |
           |       Playback controls            | Notes          |
           |                                    | Next action    |
           | Narration and source context                        |
           | Scene sequence: thumbnails, duration, status         |
```

The frame is the visual anchor. A selected item has one strong outline and visible title; the
inspector never competes with it. No screenshot-only interface: every depicted control maps to
a contract and a loading/error/disabled/recovery state.

## 6. Visual language and interaction craft

Direction: **a quiet screening room with readable editing surfaces**. The video supplies the
color and character; chrome is restrained slate, clear typography, and modest blue selection.
The memorable element is the frame-and-narration workspace, not decorative app chrome.

### 6.1 Initial token proposal

| Role | Dark | Light |
| --- | --- | --- |
| Canvas | `#1E2935` | `#F4F7FA` |
| Raised/editor surface | `#263442` | `#FFFFFF` |
| Primary text | `#EDF3F7` | `#202F3B` |
| Secondary text | `#AAB8C6` | `#516273` |
| Selection/focus accent | `#83B8E8` | `#245C91` |
| Attention/review | `#E6C482` | `#795400` |

Add semantic success, error, border, hover, disabled, and selection tokens during G2. Measure
contrast for actual pairs and control states; these values are proposals, not certification.
Light, dark, system, and high-contrast modes must all have reviewed captures.

- Platform system sans for interface labels; 14–15 px default, 12–13 px secondary metadata.
- Script/narration at 17–20 px with comfortable line height and a 60–75 character reading measure.
- Script-specific fallback fonts for Hindi and Urdu; explicit text direction per paragraph where needed.
- Monospace only for timecode, codes, and diagnostics; tabular figures for duration/money.
- Spacing: 4 px base, usually 8/12/16/24/32; align content edges and adjacent control baselines.
- Comfortable rows around 40 px; optional compact rows around 32 px; larger targets for primary actions.
- Modest 6–10 px corners according to component hierarchy. Dialogs are distinct from thumbnails.
- Minimal shadows; separators group actual regions. Avoid a grid of identical dashboard cards.
- Status uses text and icon as well as color. Progress stays indeterminate when work is uncounted.
- Motion explains selection, opening, or completion; 120–180 ms as a starting point. Respect reduced motion.
- No mandatory animated onboarding, decorative graphs, or empty full-screen hero treatments.

### 6.2 Window and input behavior

Target 1440 × 900; verify 1280 × 800 and 1024 × 700. At narrow widths, collapse navigation to a
drawer and move the inspector behind a labeled control. Keep selected media and the next action
visible. Support 200% text scaling without clipped actions, with a single-column fallback if needed.

Use platform modifier conventions. Initial shortcuts: Cmd/Ctrl+K search commands, Cmd/Ctrl+N
new draft, Cmd/Ctrl+F local search, Cmd/Ctrl+S save/publish the current edit as appropriate,
Cmd/Ctrl+Z editing undo, Space playback outside text fields, Escape close the current transient
surface. Bindings must be tested in focus contexts; Space and Delete never act on background items
while typing. No destructive global shortcut without scope and confirmation.

Local text undo and committed production history are distinct. Explain that undoing a production
edit can invalidate descendants and never reverses a charge or silently reruns generation.

## 7. Core journeys

### 7.1 First launch and setup

1. Offer **Open workspace** or **Create workspace**, plus recent work. Explain the folder choice briefly.
2. Resolve via the same workspace context as CLI/TUI; show older layouts honestly and avoid migration by surprise.
3. Show a short readiness checklist: creative template, channel, provider routes, tools, and budget configuration.
4. Allow browsing and offline drafting before execution is ready. Missing providers block a run, not the whole app.
5. Offer bundled guide examples with explicit copying into user-owned resources. Preview before adding.
6. Run offline discovery only; let the user review executable/profile findings and model choices before saving.
7. Offer explicit provider testing as a separate reviewed action; configuration is not proof of authentication.
8. Finish on Home with a draft action. An existing workspace opens useful work immediately; setup stays available.

Do not read credentials, invoke provider programs, start production, or admit spending during ordinary
first launch. A doctor check shows the observation time and does not silently install/repair tools.

### 7.2 Create a video or series without a configuration maze

One guided draft flow with **Single video / Series** choice, four short steps, and a persistent summary:

1. **Story:** title/work, template, source text/file, required guide-specific inputs.
2. **Audience and style:** channel, language editions, approximate duration/quality where supported.
3. **Continuity:** for series, ordered parts and supported shared material; start with sensible inherited choices.
4. **Review draft:** saved inputs, missing prerequisites, budget target, and **Create draft**.

Defaults come from existing user settings and validated guide schemas. Schema-driven fields honor
types, choices, help, optionality, arrays, and cross-field validation. Do not invent unsupported duration
controls. Show an estimated duration only when it is an estimate. Preserve all inputs on Back or error.

Creating a draft never starts generation. A saved production budget and a run allowance are different
concepts; use distinct labels and explain them at the run decision.

### 7.3 Review and run

From a draft, **Review run** opens a clear plan:

- Target video/edition or selected series parts, stopping point, and existing work to reuse.
- New work, work changed by edits, checks, and blockers; per-stage provider and bounded resource needs.
- Known quoted ceiling, request limit, current account headroom, and remaining unknowns.
- Existing approvals that remain valid and items that need review.
- An explicit allowance for this run, defaulting to zero; no preselected paid authorization.

Run submission binds the exact reviewed hash, scope, export status, and idempotency key. If anything
material changed, refresh the plan and show the difference. Disabling the button prevents accidental
double clicks; backend idempotency remains the real duplicate-dispatch protection.

Offer **Start background studio** when required, as a scoped setup action; merely opening the app
does not resume saved jobs. Explain that starting an existing daemon can enable queued authorized
work. G1 must provide inspect-before-start behavior and an inhibited launch when needed.

Once submitted, return to the production with useful progress. Do not require watching a log page.
Completion at `ready` enables a separate export action; it is never treated as export permission.

### 7.4 Review, edit, and approve

- Open **Needs review** at the first relevant item, with a clear remaining count.
- Read source/context, see the current frame/sheet, hear existing audio, and inspect checker findings.
- Approve the current content/version, add a note, request an edit, or prepare a targeted redo.
- Move to the next item without losing edition/filter context. Preserve a clear way back to all scenes.
- Show old/new versions side by side where useful, with synchronized zoom for images.
- Before semantic save, show affected shots, audio, timings, approvals, and deliveries.
- Text-only local draft edits autosave to a recoverable GUI draft; publishing to production uses the
  application edit contract and expected version/hash. No file overwrite from a text-widget callback.
- Redo shows the reason and a new reviewed run plan. A note or changed prompt is not permission to run.
- Approvals are content-hash based. A changed sheet/frame becomes reviewable again; the previous
  approval remains historical evidence, not a current green check.
- Per-language review can be empty. Clear the preview immediately; late image/audio results must
  not display media from another shot, language, version, or workspace.

The initial timeline is an accurate selection/navigation and timing-inspection surface. Add only
supported timing/reorder edits with backend impact previews; never let drag-and-drop imply an
unsupported composition engine or silently mutate neighboring edition timings.

### 7.5 Series and shared work

The Series page has **Parts · Shared material · Characters · Deliveries**:

- Parts show ordered titles, linked-video status, current stage, blockers, review counts, and delivery coverage.
- Shared material distinguishes research/source dossiers, style/guide settings, characters, voices, and assets.
- Each shared item shows version, which parts use it, and whether it is available, verified reusable,
  stale, missing, or not yet generated. Display reuse only when engine provenance supports it.
- Version changes open an impact view: affected parts/artifacts, valid pinned versions, explicit keep/update choice.
- A new part can inherit validated series choices and link/create a genuine video. Unlinked parts block execution.
- Batch planning shows the aggregate and per-part allowance allocation, duplicate links, and partial-submission receipts.
- Support selecting a subset of parts without accidentally changing the entire series.
- Completion derives from linked saved timelines and nonempty ready deliveries; exported state requires a
  validated delivery artifact. A registry flag or submitted job is insufficient.

“Research once” and “consistent characters” are visible lineage/reuse outcomes, not decorative badges
or promises that every generation across films is automatically reusable.

### 7.6 Activity, stop, and recovery

Always show meaningful state: **Queued, Running, Waiting for review, Waiting for provider, Pausing,
Paused, Stopping, Stopped, Failed, Ready**. Map these from recorded state plus explicit blocker data;
retain original technical codes in details. A job and a video can have different states.

- Jobs grouped by work; stage, latest useful event, elapsed time, and last observation time.
- Pause, resume/unpause, and stop only where the backend allows them. A fresh continuation after
  changed inputs goes through a new plan, not an unconditional Resume label.
- Reorder pending work with pointer and keyboard controls; backend validates eligible rows.
- Reconnect from the last durable event sequence. Stale connection status is visible; never imply live progress.
- Reopening the GUI observes existing work. It does not re-admit it or renew expired authority.
- Close window and Quit distinguish leaving background jobs running from requesting their stop.
- Crashed/uncertain work opens recovery with saved attempts and artifacts before retry planning.
- A blocked provider displays reason and recorded retry time; no automatic paid fallback or repeated retry loop.

**Stop all is a first-class shared operation:**

1. Clearly scope it to this workspace's production queue. Other workspaces remain distinct.
2. On explicit activation, inhibit new dequeue/admission in that scope, snapshot eligible jobs, persist
   stop requests, and return a structured receipt. Retrying the same control request is idempotent.
3. Stop queued/paused jobs and request cancellation for running work; keep the GUI responsive.
4. Show stopped, finished during stopping, still stopping, and unconfirmed counts. Verify managed local
   worker exit where supported; remote already-dispatched requests can remain uncertain.
5. Never equate socket loss, sent signals, daemon disappearance, or zero visible rows with complete stop.
6. Keep the admission inhibition durable across daemon restart. Explicitly clearing it does not restart
   stopped work; resuming requires normal job/authority checks.
7. A daemon-unreachable path first records the scoped inhibition through an authorized shared handler,
   then reports pending/unconfirmed controls; it does not claim remote work stopped or launch a new daemon.

Stop preserves saved output and needs no extra confirmation when the user explicitly presses the
clearly scoped control. Force termination, if offered later, is a separate reviewed action bound to
verified process identity. Process inspection alone grants no termination or accounting-clear authority.

### 7.7 Explicit delivery

Export screen:

1. Select edition, supported aspect/preset/format, destination, and overwrite policy.
2. Inspect readiness and approved content. Incomplete/stale delivery prerequisites link to their cause.
3. Review the export plan, required resources, and exact output. Click **Export video** or a separately
   labeled **Open project in editor** action; neither is a side effect of Continue.
4. Track render state and cancellation without losing the review workspace.
5. Verify output identity, existence, nonzero size, and expected streams/duration through the shared
   export validation path. Show **Exported** only when validation succeeds.
6. Offer inline playback, **Show in folder**, and repeat export from saved settings with a fresh review.

Previous deliveries retain edition, preset, source version/hash, size, duration, time, and validation
status. An invalidated or missing file is visible and never remains an unqualified success.

## 8. Complete capability coverage

This matrix defines release coverage, not just navigation. Final G1 inventory enumerates actual
current operations rather than freezing a historical command count.

| Area | Dedicated experience | Shared boundary | Phase |
| --- | --- | --- | --- |
| Workspace/setup | Choose/create/open; checklist; profiles; health | `workspace`, `setup`, `profile`, `doctor` | G3 |
| Video collection | Search, sort, editions, saved views, multi-select | `library`, `video`, `views` | G3, G8 |
| Draft creation | Schema-guided video/series flow, source preview | `creation`, `authoring`, `series` | G4 |
| Planning/execution | Reuse/impact/provider/allowance review | `plan`, `continuation`, `execution`, `studio::submit` | G4 |
| Script/scenes | Review, source context, notes, hash approval, semantic edit | `review`, `video` | G5 |
| Characters/continuity | Sheets, versions, pins, affected uses | `series`, `assets`, review APIs | G5, G6 |
| Audio/localization | Listen, waveform/timing, per-edition lines, supported edit/redo | `audio`, `review`, plan APIs | G5 |
| Shared series | Link/create parts, subset plans, continuity/reuse and deliveries | `series::execution`, `assets` | G6 |
| Assets/takes | Search/filter, inspect provenance/usage, compare/select supported takes, verify | `assets`, `library` | G6 |
| Music/SFX | Preview/import/tag, licence evidence/checks | `audio` | G6, G8 |
| Activity | Observe/replay, queue ordering, pause/unpause/stop, recovery | `queue`, `studio`, supervision contracts | G7 |
| Providers | Offline discovery, reviewed add/edit, routes/fallback order, health/login, explicit test | `providers`, `provider_tests`, `configuration` | G3 essentials; G8 full |
| Budget/account | Ceiling/headroom/held/known spend, per-work/provider/period, limits and reconciliation | `account`, spend projection | G4 essentials; G8 full |
| Guides/channels | Example/import/create, typed field/list editor, sidecars, validation, revision conflict | `authoring`, `creative` | G3 examples; G8 full |
| Backup/restore | Preview, verify, inactive restore, separately reviewed activation/reconciliation | `storage`, account maintenance | G8 |
| Library lifecycle | Copy/archive/trash with scope and undo where supported; explicit permanent removal | `library` | G8 |
| Delivery/presets | Edition/preset/aspect/output review, editor handoff, validation/history | `presets`, continuation export APIs | G9 |
| Advanced creative tools | Task-specific inspectors; file/schema inputs when needed, explained results | `creative` | G8 |
| Diagnostics/help | Searchable errors, health reports, redacted support export, relevant documentation | `protocol`, `doctor`, `dev` | G2 baseline; G8 full |
| CLI-only shell utilities | Explain how to use completion/man/shell output; no meaningless GUI imitation | CLI retains these utilities | G8 coverage audit |

Every operation is classified as dedicated GUI, advanced typed action, CLI-only with a documented
reason, or intentionally unavailable placeholder. No silent omissions. Common creator journeys
cannot pass merely because a generated form exists in the command palette.

## 9. Architecture and state ownership

### 9.1 Crates and dependencies

Start with one new workspace package, `crates/retold-desktop`, producing `retold-studio`.
Keep shell, components, screens, presentation state, and platform/media adapters modular inside it.
Do not pre-create a fleet of new crates; split a shared library only when a real second consumer exists.

```text
retold-desktop (GPUI Kit) ───────┐
                               ├── retold-app ── retold-engine ── domain crates
retold-cli (CLI and TUI) ────────┘
```

Allowed internal dependency: desktop → app. No desktop → CLI, engine, provider adapter, or account
SQLite access. App remains free of GPUI/Clap/Ratatui. Extend the architecture script to recognize
two application roots and reject UI dependencies below presentation. Ensure every production crate
is reachable from at least one declared product root; retain detection of unused/dead roots.

Suggested desktop module ownership:

```text
src/
  main.rs                   Native startup and worker-mode selection
  shell/                    Window, navigation, menus, command search, layout persistence
  design/                   Tokens and themed reusable components
  state/                    Typed read models, selection, dirty drafts, request generations
  bridge/                   Queries, reviewed actions, control lane, event replay
  screens/                  home, videos, video, series, library, activity, settings
  media/                    Decoder/player abstraction, thumbnails, waveforms, caches
  platform/                 Dialogs, clipboard, notifications, lifecycle and packaging hooks
```

Avoid giant render functions and universal mutable app state. GPUI entities own focused screen/
component state; app services own domain state; messages carry IDs and revisions, not mutable stores.

### 9.2 Typed desktop boundary

Add focused contracts to `retold-app` as consumers need them, using existing types and handlers:

- Read projections such as `VideoSummary`, `ReviewItem`, `EditionSummary`, `AssetUsage`, and `JobDetail`.
- Structured diagnostics with user-facing reason, affected object, and safe recovery action.
- Draft/edit requests carrying expected content version/hash and explicit scope.
- Reviewed execution requests retaining the plan hash, bounds, and stable submission key.
- Stop/control requests carrying scope, durable control ID, and receipt/outcome state.

Names above are proposed contracts, not claims of existing symbols. Reuse current compatible
types and keep changes small. Where CLI and GUI need the same projection or orchestration, move
that logic into app and adapt both callers; do not maintain independent execution rules.

Create a frontend-neutral operation registry for action identity, target kind, availability, and
effect classification. Presentation supplies labels/forms. It must cover conditional effects:
export, provider requests, file replacement, financial writes, and process control are not captured
adequately by one “read/write” bit. Unknown operations fail closed. Compare registry coverage to
the CLI command tree in tests without importing Clap into application services.

### 9.3 Workers, events, and UI lifetime

- Render and input handlers perform no filesystem walks, hashing, decode, SQLite, network, or waits.
- Send application work to bounded background lanes. Reserve separate capacity for control requests.
- Use GPUI tasks for lifetime-aware delivery; retain or explicitly cancel task handles. Confirm how
  the selected GPUI executor interoperates with existing blocking and Tokio work in G0.
- Keep workers independent of view lifetimes. Dropping a screen cancels its observation/preview work,
  not an already submitted production job.
- Tag results with workspace identity, object ID, edition, content version, and request generation.
  Discard late results after selection/workspace changes, especially media and plan replies.
- Replay durable events by monotonic sequence; reconcile with authoritative snapshots after reconnect.
- Bound each subscriber initially to 128 envelopes/256 KiB, matching the runtime contract. Coalesce
  ephemeral progress; disconnect/replay slow clients rather than blocking the daemon.
- Show observed-at/connection status and the last confirmed data; never replace a usable library
  with a false empty state because a scan failed or a projection is incomplete.
- One serialized mutation lane per workspace for desktop requests does not replace cross-process
  application locks, revision checks, ownership guards, or shared financial transactions.

[GPUI task model](https://gpui-kit.com/docs/task/) describes task lifetime; Retold's
[runtime contract](runtime-editing-contract.md) remains the authority for production semantics.

### 9.4 Daemon packaging and concurrency

Proposed first package: one signed `retold-studio` executable with an internal worker mode that
instantiates the shared `StudioServer`. Launch by an exact bundled executable path and argument
vector; do not require a global CLI install or invoke a shell. Reuse/move the small lifecycle
orchestration from the CLI into a shared app helper so GUI and CLI honor the same singleton checks.

The worker mode must branch before GPUI initialization and run without a display. Prove that this
is reliable in G0/G1; if platform initialization prevents it, package a small sibling worker binary
with the same app contracts. The decision is recorded before packaging, not discovered at release.

For the later Windows milestone, inventory Unix-only imports across app/engine first. Introduce a
local transport abstraction with bounded framing, timeouts, capability negotiation, and per-user
access control; qualify named pipes or another proven local transport rather than assuming socket
paths work unchanged. Port file permissions, atomic publication, ownership locks, process identity,
subprocess-tree cancellation/reaping, and path/Unicode behavior together. Test these shared backend
changes through CLI as well as GUI before advertising Windows. Choose the implementation from a
small native spike; do not promise equivalent process-control guarantees without that evidence.

Attach to the existing same-workspace daemon rather than creating a second owner. Inspect queue
and inhibition state before launch; no auto-start on ordinary app open. Add backward-compatible
capability/protocol negotiation for new controls. An older daemon must show the unsupported action
clearly; upgrades require reviewed shutdown/restart and must never enqueue duplicate work.

Two windows or CLI+GUI may observe/edit concurrently. Use stable identity and expected-version
checks; detect a changed artifact before save/approve/run. A conflict keeps the local draft and
offers compare/reload/reapply; never blindly last-write-wins. App-private UI drafts cannot grant
production authority after restart.

### 9.5 Media subsystem

Select a playback adapter in G0 using existing synthetic files. Compare compatibility with the
pinned GPUI graph, licence/redistribution, cross-platform packaging, A/V clock, seek latency,
GPU upload, error handling, and maintainability. Candidate approaches are an isolated FFmpeg-based
decode/audio pipeline or a compatible maintained native/player adapter; no library is selected yet.

- Required baseline: inline image, existing narration/audio playback, waveform, inline rendered-video
  playback, seek, pause/volume, timecode, keyboard controls, and teardown on selection/workspace change.
- Audio drives synchronized playback. Drop late preview frames; bound queues and decode to displayed
  size. Never decode a full movie or all thumbnails into memory.
- Keep GPU textures/window resources on the required executor. Prefer safe adapter APIs; native FFI
  cannot silently weaken workspace `unsafe_code = forbid`. Isolate reviewed external binding code
  if needed and document why; do not copy Zed application modules with different licences.
- Cache resized thumbnails and waveform summaries by content hash, transform, edition, and version.
  Start with a 256 MiB resident media budget and 2 GiB disposable disk cache; enforce eviction and
  tune after measurement. Production artifacts and financial evidence are never cache eviction targets.
- Validate dimensions, stream metadata, actual decode bounds, corrupt/truncated media, and path identity.
- Distinguish an approximate storyboard/audio preview from the final rendered video. Display the exact
  current rendition and quality; do not present a slideshow as final playback.
- External-open remains useful for unsupported files or diagnostics. It does not satisfy the inline
  playback release gate. Never generate a new preview through paid providers just to populate a screen.

## 10. Trust, state clarity, and data handling

| Action class | UI behavior |
| --- | --- |
| Inspect/search/select/play existing media | Immediate; cancel obsolete work; no production mutation |
| Local draft edit | Recoverable autosave with visible saved/unsaved status |
| Publish semantic edit / approve | Scoped effect/impact shown; expected version checked; history retained |
| Provider invocation / continue / redo | Exact reviewed plan and explicit allowance; app revalidation |
| Export / editor handoff | Separate target/preset review and explicit action |
| Stop current/scoped queue | Immediate explicit control request; visible pending/verified outcomes |
| Limits/reconciliation/restore activation | Preview, deliberate apply, and shared maintenance/revalidation |
| Permanent delete/overwrite/force terminate | Exact scope and consequences; explicit confirmation |

Avoid repetitive “Are you sure?” dialogs for harmless UI actions. High-impact reviews should show
the actual difference and consequences, with specific buttons such as **Set monthly limit**.

Store only presentation preferences and recoverable drafts in a versioned desktop settings location.
Use existing credential/profile references; never persist API secrets in layout, draft, logs,
screenshots, or support archives. Mask sensitive fields, support explicit reveal/copy where relevant,
and redact diagnostic export through a tested shared policy. No analytics or update network traffic
on ordinary launch unless configured explicitly.

Inactive/restored workspaces are browsable where existing read-only policy permits, with clear
status and a link to activation review. Do not enable writes by interpreting an open folder as
ownership. Safe media file resolution must use application-approved paths and reject traversal or
changed file identities before sensitive actions.

### 10.1 Required screen states

Each screen/component has fixture-backed coverage for:

- First loading, refresh with retained content, empty, filtered-empty, failed, stale/disconnected.
- Inactive/read-only workspace, missing resource, missing tool, unavailable provider, denied action.
- Dirty local draft, saving, saved, conflict, canceled review, lost response, and retry.
- Long titles, Unicode paths, mixed-direction text, zero/unknown/nonzero money, long durations.
- Work still stopping, finished during stop, uncertain remote work, and interrupted recovery.

Copy follows reason → impact → next action. Example: **Character sheet changed. Review the new
version before these scenes can continue.** Details can expose the hash mismatch; the banner need
not lead with it. Never replace an error with a toast that disappears before it can be acted on.

## 11. Performance and accessibility budgets

These are proposed measurable acceptance targets, not current measurements. G0 records a reference
machine, release build, fixture size, OS/driver, fonts, media, and measurement method. Tighten or
change a target only through a documented decision, not by raising it to hide a regression.

| Measure | Initial gate |
| --- | --- |
| Warm window interactive | p95 ≤1 s; no provider/tool spawn or complete library scan |
| Cold launch to usable shell | p95 ≤2 s on reference machine; deferred indexing visible |
| Input to visible feedback | p95 ≤100 ms under event/decode load |
| Scroll/selection frame | p95 ≤16.7 ms on reference 60 Hz fixture, with media decoded asynchronously |
| Stop/Pause persisted acknowledgement | p95 ≤250 ms during heavy background I/O; terminal completion reported separately |
| Warm library query | p95 ≤150 ms for 1,000 videos; 10,000-video stress fixture remains bounded |
| Existing preview first display | p95 ≤300 ms cached, ≤1 s uncached local thumbnail |
| Video seek to preview | p95 ≤250 ms cached/proxy fixture; document codec-dependent cases |
| A/V sync | Within ±80 ms on the qualified 10-minute synthetic fixture |
| Idle window | Average <1% of one core over 60 s after settling, no constant repaint loop |
| Memory after stress | Explicit per-cache bounds; no monotonic growth over repeated open/close/seek cycles |

Avoid hard total-RSS promises before G0 because toolkit/decoder baseline varies. Record idle and
loaded RSS, GPU memory where measurable, and peak decoder allocation, then set regression ceilings.
Capacity controls and reserved jobs remain shared backend rules, not frontend-only estimates.

Accessibility gates: useful roles/names/states/actions, visible focus, logical Tab/Shift+Tab order,
modal focus trap/restoration, keyboard alternatives for dragging, readable contrast, 200% text,
non-color status, reduced motion, and accessible media controls. Verify real VoiceOver on macOS;
add Orca and NVDA as their platforms qualify. Toolkit semantic-tree assertions alone do not prove
screen-reader announcements or usable IME composition.
[Accessibility guide](https://gpui-kit.com/docs/accessibility/).

## 12. Detailed phase gates

### G0 — Feasibility and design proof

Build an isolated, disposable GPUI spike and fixture gallery, then record:
dependency/MSRV/platform decision; native input and screen-reader checks; media adapter comparison;
real-frame captures; runtime/worker-mode proof; measured baseline. Review light/dark and compact
states using representative titles, Hindi/Urdu, notes, and errors.

**Exit:** every §4.3 proof has evidence or an explicit blocker. No production foundation is declared
ready while inline playback, required language editing, or accessible actions remain unknown.

### G1 — Shared contracts and reliable lifecycle

Inventory CLI/app capabilities and effect classes. Add only needed typed projections. Extract
shared lifecycle helpers, negotiate daemon capabilities, define the scoped admission inhibition
and Stop-all receipt, and close correctness gaps needed for desktop. Extend architecture, quality,
and source/licence gates for an additional application root.

**Exit:** CLI/TUI behavior remains covered; desktop test harness can query, preview, submit once,
observe/replay, and stop through shared contracts. Stale plans, duplicate clicks/reconnects,
inactive roots, concurrent edits, and older daemons cannot bypass backend rules.

### G2 — Visual and interaction foundation

Implement shell, navigation, menus, command search, themes, focus, persisted layout, notification
hierarchy, and reusable components. Gallery includes buttons, fields, typed lists, tables,
empty/error/blocked states, preview placeholder, inspector, run review, and conflict UI.
Codify the proven tokens, vocabulary, and checks in `docs/desktop/DESIGN.md`,
`FRONTEND_CONTRACT.md`, and a screen-state gallery inventory; avoid duplicating product architecture.

**Exit:** reviewed native captures at all sizes/themes and text scales; no clipping/focus traps;
selection and late-result tests; no blocking work in render/input paths.

### G3 — Open existing work and configure essentials

Implement workspace selection, first launch, Home, Videos, details, offline provider discovery,
explicit guide-example copying, readiness, and budget/tool prerequisites. Preserve incomplete
index visibility and older workspace resolution. Add saved-view browsing as existing APIs permit.

**Exit:** a clean install and an existing workspace are both useful without provider calls; missing
resources explain a path forward; native dialogs and path-with-spaces/Unicode cases pass.

### G4 — Creation and reviewed execution

Implement guided drafts, series part creation/linking, exact run review, allowance inputs, daemon
start review, idempotent submission, and progress/blocker links. Integrate basic Activity and Stop
before exposing any running workflow. No paid defaults inherited from saved production budgets.

**Exit:** isolated fake-provider flow creates a real draft, plans, submits one durable job, preserves
review cancellation, rejects changed plans, and survives GUI reopen without renewed authority.

### G5 — Review and editing vertical slice

Implement scene preview/sequence, Script, Characters, Audio essentials, note and approval loops,
semantic edit impact, version compare, conflict recovery, history, and explicit redo planning.
Add media decode/cancel/cache behavior and edition selection throughout.

**Exit:** review an actual saved artifact, edit safely, see exact invalidation, approve its current
hash, and continue deliberately. Pointer/keyboard/screen-reader paths and mixed-language input
work. Native pixel evidence covers normal, empty, long-text, blocked, and conflict states.

### G6 — Series and reusable production

Implement part matrix, shared-material/version inspector, character usage, immutable takes,
supported take selection, and update-impact review. Add aggregate/subset plans with per-part
receipts and proven completion/delivery coverage. Expose music/SFX provenance and licence state.

**Exit:** three fictional linked parts reuse a genuinely eligible source/character version; a version
change identifies affected work; canceled update changes nothing; unlinked/duplicate parts block;
partial submissions remain recoverable and cannot multiply the allowance.

### G7 — Activity and interruption hardening

Deepen live progress, ordering, pause/unpause, durable Stop-all inhibition, stale observations,
close/Quit choices, interrupted-worker recovery, and diagnostic drill-down. Test GUI crash, daemon
crash, lost control replies, blocked provider, slow client, event flood, and journal failure.

**Exit:** controls stay responsive and report verified outcomes; no new dequeue slips past accepted
inhibition; observer restart never retries generation; unknown remote cost/work stays explicit.
Existing process-inspection limits hold. Do not claim the historical spinning-daemon incident's
root cause was diagnosed by these tests; capture that as separate investigation if it recurs.

### G8 — Full product coverage and administration

Complete provider route editing/health/login/test, Budget reports and reviewed limits/reconciliation,
guide/channel schema editing with sidecars, presets/profiles, saved views, supported library lifecycle,
backup/verify/inactive restore/activation, advanced creative tools, and redacted diagnostics.

**Exit:** capability inventory has no unexplained gaps; common workflows have dedicated UI;
financial/storage previews match apply; cancellation preserves bytes; secrets remain redacted;
no completion claim relies solely on a generic action form.

### G9 — Deliver, qualify, and ship

Complete delivery/export validation and editor handoff; package native assets/fonts/icons/worker
and dependency inventory. Qualify clean install, upgrade, uninstall-with-data-preserved, missing
tool recovery, and workspace backup compatibility. Add signed/notarized macOS distribution when
release credentials are available. Add Linux package/desktop integration after native qualification.

GPUI Kit supplies UI, not a finished installer or signing pipeline. Every OS/architecture needs
its own artifact, dependency inventory, and packaged acceptance.
[Packaging guide](https://gpui-kit.com/docs/packaging/).

Automatic updates are deferred unless separately scoped; manual version/checksum/release notes
are enough initially. Update installation must never auto-resume production or discard drafts.

**Exit:** all acceptance journeys pass on every advertised target; actual users can complete the
core flow; native screenshots and packaged evidence are reviewed; known limits are documented.
An unsigned local build is a developer preview, not a signed production release.

## 13. Acceptance journeys and proof

Use existing isolated zero-price providers and prebuilt media. Live provider calls and exports
require the applicable explicit user authorization; G0/plan work itself performs none.

| Journey | Required proof |
| --- | --- |
| Fresh install | Empty app → workspace → copied template/channel → saved draft; zero provider invocation |
| Existing work | Open current saved workspace without migration/surprise writes; CLI and GUI agree |
| Reviewed run | Cancel preserves inputs/bytes; apply binds scope/hash/allowance; double click submits once |
| Visual review | Correct frame/audio/edition, approve current hash, note retained, next item works |
| Empty filter/race | Clear preview; delayed results cannot restore another language/shot/workspace |
| Semantic edit | Impact shown; expected revision enforced; unaffected artifacts/approvals remain valid |
| Concurrent edit | GUI draft conflicts with CLI/other-window change; compare/reapply loses neither version |
| Three-part series | Linked parts, proven reuse/pins, aggregate bound, subset run, truthful completion |
| Pause/stop/reopen | Durable request, responsive UI, verified termination where possible, no auto-resume |
| Stop all race | Inhibit plus snapshot; no newly dequeued job; retry/lost reply/restart remains safe |
| Provider blocked | Readable blocker/retry evidence; no silent fallback spend or unbounded retries |
| Budget | Known/held/unknown amounts distinct; micros exact; preview/apply parity; stale apply revalidates |
| Storage recovery | Verified backup → inactive restore → browse → separate activation review; source evidence retained |
| Export | Explicit authorized fixture export; nonempty file, expected A/V streams/duration, inline playback |
| Native behavior | Clipboard, dialogs, drag/drop, IME, text scale, focus, screen reader, OS sleep/reconnect |
| Load/recovery | 1,000/10,000 videos, event flood, stalled client, malformed media, disk/journal failure |
| Packaged app | Clean host/profile; bundled worker launches display-free; CLI/GUI daemon coexistence |

Three separate evidence layers:

1. **Behavior:** pure state/contract tests plus GPUI test-support input/focus/layout assertions.
2. **Pixels and native operation:** real offscreen/window captures with reviewed fonts, size, theme,
   and focus; real IME/screen-reader and packaged-app checks.
3. **Production result:** saved manifests/timelines, account receipts, job/control outcomes, and
   verified media. A green component test or screenshot does not establish these.

Kit documents separate interaction tests and offscreen pixel rendering, with platform limitations;
do not substitute semantic snapshots for pixel tests or assume Windows headless rendering support.
[Testing guide](https://gpui-kit.com/docs/test/).

Suggested GUI test targets are named during implementation, then wired explicitly into the gate.
Run focused tests after each slice and the authoritative `./scripts/check.sh` before publication.
Extend it for GUI interaction/rendering rather than assuming `cargo test --workspace` selects
every custom native rendering target. Maintain the Clean Code baseline and dependency constraints.
Linux/macOS graphics test setup is explicit; unsupported hardware reports a limitation rather than
quietly skipping a required proof. No Rust tests are needed for this documentation-only plan.

### 13.1 Usability and beauty acceptance

Run task sessions with at least five representative users: new creators and experienced operators.
Use the same fictional brief and saved fixture, with no command-line coaching. Separate time spent
waiting for generation from time spent understanding/operating the UI.

Targets before release:

- At least 4/5 create a correct saved draft within five minutes after workspace selection.
- At least 4/5 locate a blocker and reach its corrective action within 30 seconds.
- At least 4/5 review one scene and identify the effect of an edit without coaching.
- Every participant can distinguish run allowance, accrued/held spend, ready state, and exported file.
- Every participant can find scoped Stop and understand whether work is confirmed stopped.
- At least 4/5 complete explicit export from a ready fixture without searching technical documentation.

Record errors, backtracking, wrong assumptions, and task outcome. Revise the deepest common friction,
then repeat affected journeys. Visual review checks hierarchy, spacing, typography, contrast,
media quality, light/dark consistency, long text, and empty/error states on actual native captures.
Human aesthetic review remains necessary; “best possible” is a continuing quality target, not a
claim established by choosing a toolkit or passing screenshot tests.

## 14. Risks, decisions, and scope controls

| Risk | Handling / decision gate |
| --- | --- |
| Toolkit churn or dependency mismatch | Exact facade/lockfile; G0 graph proof; dedicated upgrade slices |
| Strong components but weak media integration | G0 media proof; isolated adapter; inline media required for release |
| Hindi/Urdu looks right but edits incorrectly | Real IME/bidi/grapheme/caret test, not just static rendering |
| GUI bypasses account/review rules | App-only actions; expected hashes; effect registry; cross-client tests |
| Daemon already has authorized queued work | Inspect-before-start; durable inhibition; no ordinary-launch auto-start |
| Index/runtime contracts exceed current implementation | G1 gap audit; shared backend slices before UI claims depend on them |
| Windows mistaken for an easy packaging target | Explicit IPC/filesystem/process portability milestone before Windows commitment |
| Attractive mockup accepted as product completion | Per-phase evidence; separate interactive/pixel/artifact proof |
| Scope expands into full video editing | Supported semantic edits only; separate plan for new composition features |
| Too many controls and destinations | Five primary destinations, task tabs, contextual inspector, tested progressive disclosure |
| Generic action palette mistaken for usability | Dedicated core journeys; capability audit and user task gates |
| Signature/release credentials unavailable | Complete reviewable artifact first; report developer-preview status accurately |

Decisions intentionally deferred to evidence: exact media library; proven desktop MSRV; offscreen
test harness details; index gaps requiring backend work; Windows IPC implementation; final typography
fallback packages; signing identities. None needs a user preference question to start G0.

### 14.1 Effort and sequencing

This is substantial application work. Initial planning range: approximately **65–100 engineering
days** for a polished macOS release with core backend gaps, native testing, and usability iteration;
Linux and Windows qualification are additional and depend strongly on G0/G1 findings. This is an
uncalibrated estimate, not a calendar commitment. Re-estimate after G0 and again after the G4 vertical
slice; larger index/supervision/media work must be called out instead of hidden in GUI polish.

Keep implementation slices reviewable: shared contract with regression proof, component or screen
with all states, then one actual journey. Commit verified slices before expanding. Finish a phase's
exit conditions before claiming it complete; record local/committed/remote/packaged evidence separately.

**Recommended first implementation slice:** G0 only—one real GPUI Kit window containing a
scene preview, multilingual narration editor, accessible controls, a fake slow query, and an inline
existing-media player. Prove the hardest integrations and the visual direction before building the shell.

## 15. Planning artifacts and current evidence

- Current implementation and outstanding G0–G9 gates are tracked in the
  [completion ledger](desktop/completion-ledger.md), including the actual
  [saved scene editor](desktop/scene-editing.md). The concept and planning-only observations below
  describe their original preparation, not the current implementation status.
- This document is the implementation plan, grounded in the inspected shared application boundaries.
- [Interactive concept](gui-concept.html) illustrates proposed Scenes, Series, Activity, run review,
  and phase gates. It is self-contained HTML for design discussion, not GPUI implementation or
  proof of native performance/accessibility/backend integration.
- No dependencies were added, daemon started, jobs resumed/stopped, provider invoked, or video exported
  while preparing the plan. Existing unrelated source edits are preserved.
- No implementation phase is marked complete. Toolkit research is current as of the date above;
  revalidate the pinned release and requirements at G0.
- Concept verification: rendered at 1120, 728, and 375 px; light/dark appearances visually inspected;
  13 browser-side interaction assertions passed for scene/note/review state, appearance, run-dialog
  scope and focus, simulated stop uncertainty, phase disclosure, and self-contained assets. These
  assertions use DOM activation and do not establish native pointer/keyboard or GPUI behavior.
- All local Markdown links resolve and `git diff --check` passes. Rust quality gates were not run
  for these documentation/prototype-only additions; no production Rust source was changed here.
