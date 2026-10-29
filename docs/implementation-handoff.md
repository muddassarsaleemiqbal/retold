# Implementation loop handoff

GUI continuation (2026-10-26): the active full scope is [G0–G9](plan-gui.md), tracked in the
[completion ledger](desktop/completion-ledger.md). Source-backed creation, saved-media review,
versioned notes, retained editing drafts and semantic scene direction have native/shared evidence
there. The integrated scene and artifact-note checkpoint passed both complete shared and desktop
gates (including native restart/rebind/frame-comment and both production/export journeys). The
complete GUI plan remains open. Continue with semantic cast/localized contracts, the audio
workspace, series, remaining
administration and release qualification. The old review shot/line/cast handlers below record
history; they are not sufficient for native semantic editing. Preserve unrelated current edits in
engine `assets.rs`/`canon.rs` and user-installed skill files. Use isolated synthetic providers/media.

CLI cutover (2026-10-21): the legacy `crates/retold` implementation and its command-specific tests were retired. `crates/retold-cli` now ships the default `retold` executable and the `retold-next` alias from one shared entry point. The workspace Clean Code gate moved to `retold-cli`; historical commands below mentioning `-p retold --test clean_code` now use `-p retold-cli --test clean_code`. Earlier phase documents describe the implementation at their recorded dates. See [current installation and usage](../README.md).

Completed checkpoints:
- `b2a9382`: rewrite through provider configuration, including existing cleanup.
- `616e7b0`: durable cooldown admission/inspection.
- `8af5db7`: read-only account attempt inspection (`Phase 6A`).
- `a0424ee`: explicit billing reconciliation without dispatch authority (`Phase 6B`).
- `ea2db61`: provider health classification, reset-time parsing & configuration migration (`Phase 3I`); see `docs/implementation-phase-3i.md`.
- Phase 3J: explicit `providers test` with hashed plans, provider-test job authority and health recording; see `docs/implementation-phase-3j.md`.
- Phase 6C: read-only `spend` reports by provider/video/series/day with period attribution, carried holds and ceiling headroom; see `docs/implementation-phase-6c.md`.
- Phase 6D: targeted `account limit` changes with merge, dry run, headroom and exhausted-unit reporting; see `docs/implementation-phase-6d.md`.
- Previous milestone: artifact-level plan/reuse qualification and every-call resource accounting (`Phase 2 Qualification`); see `docs/implementation-phase-2-qualification.md`.
- Phase 3K: account maintenance barrier, account recovery backups and offline verification; see `docs/implementation-phase-3k.md`.
- Phase 3L: offline portable directory-backup index/checksum verification; see `docs/implementation-phase-3l.md`.
- Phase 3M: managed single-workspace backup creation and read-only preview; see `docs/implementation-phase-3m.md`.
- Phase 3N: version 2 managed-backup semantic validation and explicit byte-only compatibility for version 1; see `docs/implementation-phase-3n.md`.
- Phase 3O: inactive offline restore with stopped task state, preserved financial evidence and account/engine inactivity guards; see `docs/implementation-phase-3o.md`.
- Phase 3P: same-installation workspace activation with reviewed dry-run plan hash, financial continuity verification, atomic original-root retirement, and execution enablement; see `docs/implementation-phase-3p.md`.
- Phase 3Q: offline credential profile inspection and configuration (`providers login`), multi-workspace (`backup --all`) and selected-video (`backup --video`) archives, and cross-installation portable reconciliation (`storage activate --reconcile --attest-prior-stopped`); see `docs/implementation-phase-3q.md`.
- Phase 4A: full-screen app foundation and read-only Home screen (provider health, spend against caps, recent videos, actions), with `home` as its plain/JSON twin; see `docs/implementation-phase-4a.md`.
- Phase 4B: full-screen video screen (pipeline stages, blocker-to-action panel, plan details, related cost/plan inspection) with strict read-only guarantees; see `docs/implementation-phase-4b.md`.
- Phase 4C: dedicated providers & routing screen, spend & cap screen with interactive window/grouping toggles, video log pane toggle (`l`), and an automated PTY integration test suite (`tests/tui_pty.rs`); see `docs/implementation-phase-4c.md`.
- Phase 4D: TUI universal controls (Command Palette with `:` / `Ctrl-K`, Theme Engine with Default/High Contrast/No Color, and Crossterm mouse interaction); see `docs/implementation-phase-4d.md`.
- Phase 5: Background studio lifecycle daemon, durable queue store, worker execution loop, progress event bus, and queue attach (`studio start|stop|status`, `queue list|show|pause|resume|cancel`); see `docs/implementation-phase-5a.md`.
- Phase 7: Video review, note recording, atomic approval (`approve`/`lock`), explicit `unlock`, shot and line editing with selective descendant invalidation, and undo/redo history; see `docs/implementation-phase-7.md`.
- Phase 8: Rebuildable library index, licensed audio asset manager (`library audio list|import|show`), audio QA validation, and video asset lifecycle (`library video copy|archive|delete`); see `docs/implementation-phase-8.md`.
- Phase 9: Multi-part series registry, immutable asset versions, stable cross-part pinning, and series management CLI commands (`series new|list|show|plan|run|update`); see `docs/implementation-phase-9.md`.
- Phase 10: Workspace health inspection and safe repair (`doctor [--fix]`), host tool discovery (`ffmpeg`/`ffprobe`), creative resource inspection (`guides list|show`, `channels list|show`), export presets management (`export-presets new|list|show|remove`), export preset and aspect resolution in video export (`video export <ref> --edition <lang> --preset <name> [--aspect <aspect>]`), configuration profile management (`config profile list|new|show|use`), offline first-run onboarding (`setup`), shell autocompletions (`completions`), roff manual page (`man`), top-level ergonomics (`new`, `usage`, `explain`, `schema`), human and NDJSON machine projection parity; see `docs/implementation-phase-10.md`.

All focused and full workspace checks passed cleanly:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `python3 scripts/check-architecture.py`
- `cargo test -p retold --test clean_code` against `clean-code.toml` preset `strictest` (zero baseline growth).
- Full test suite via `./scripts/check.sh --fast` and `./scripts/check.sh` (all workspace tests passed).
No live provider calls, login changes, or unrequested video exports were made.

## CLI cutover validation (2026-10-21)

`./scripts/check.sh` passed after the cutover: formatting, workspace/all-target Clippy, the lowered Clean Code ratchet, opt-in acceptance compilation, dependency architecture and reachability, unused dependencies, advisories/licenses, rustdoc, and all workspace tests. The protocol suite compares both executable names for schema/help and rejection of the retired `make` command. Existing integration tests now exercise `retold`, including studio startup and provider supervision using local fixtures. No live provider calls or production video exports were performed. Local log: `/tmp/retold-cutover-final.log`.

## Full TUI checkpoint (2026-10-21)

The TUI now includes command-derived forms and route parity, stepped provider setup,
onboarding, file browsing, editor handoff, reviewed spend plans, saved media review,
and studio event streaming. See `docs/plan-tui.md` for remaining acceptance work.

The blocked commit was recovered by removing an unnecessary breadcrumb collection,
updating the invalid-provider test to submit all wizard steps, and reviewing/updating
the provider form snapshots. All 16 CLI library tests and the complete
`./scripts/check.sh` passed, including all five PTY journeys. Full gate log:
`/tmp/retold-tui-final-check.log`. Production provider journeys, platform-specific
editor/audio/clipboard behavior, and performance budgets remain unqualified.
No live provider calls or production media exports were performed for this checkpoint.

## Dedicated provider routes editor (2026-10-21)

Providers now has a Routes tab listing every task tier, including unconfigured tiers.
Enter opens a local draft: arrows select, J/K reorder, + adds an entry, - removes it.
While adding, Tab cycles adapter-compatible providers and the user enters the selector.
Incomplete or duplicate entries are rejected; an empty chain cannot be submitted.
Enter reviews the exact CLI command and confirmation dispatches through the existing
provider route writer, preserving its validation and atomic configuration transaction.
Esc discards the draft. Provider inventory must be available before editing opens.

Five new reducer/rendering tests cover the draft, picker, argument round-trip,
confirmation and cancellation. A new PTY journey proves cancellation preserves file bytes
and saving persists the reviewed order while retaining another route, with nonexistent
provider binaries. All 21 CLI library tests, all six PTY tests, and the complete
`./scripts/check.sh` passed. Full log: `/tmp/retold-route-full.log`.
No provider requests or video exports were made. `docs/plan-tui.md` now enumerates the
remaining implementation and acceptance gaps; the full TUI plan is not complete.

## Provider health and login inspection (2026-10-21)

Dedicated Health and Login tabs open provider-scoped recorded health and offline login
evidence. Status refreshes in the background; cooldown observations retain raw timestamps
and add UTC display fields. The provider shortcuts open status (`l`), login configuration
(`c`), and a reviewed test plan with a $0 default (`t`). Configuration cancellation and
test-plan review preserve the existing dispatch gates.

The complete `scripts/check.sh` passed, including all seven PTY journeys, responsive
status rendering, offline error states, and preservation of workspace files after status
inspection and cancellation. Log: `/tmp/retold-provider-status-resumed.log`.
The previous storage activation AccountBusy failure did not reproduce in either the
focused 29-test storage-backup suite or the full gate; its cause remains unconfirmed.
No account-lock code was changed. No live provider calls or video exports were performed.
The remaining TUI implementation and acceptance gaps are listed in `docs/plan-tui.md`.

## Guided provider discovery and ownership-lock release (2026-10-21)

Providers now has a Discover tab backed by the existing offline scan. Supported programs,
Claude/Codex profile folders, and OpenRouter key-variable presence open editable provider
drafts. Discovered paths and variable names are preserved as individual CLI arguments;
model selectors and local model files remain explicit user choices. Saving uses the shared
provider writer and confirmation. Existing aliases are rejected rather than overwritten.

Reducer tests cover discovery projection, exact arguments, confirmation/cancellation,
Unicode paths, empty/error states, mouse selection, and rendering at three sizes. A compact
snapshot records the picker. The new PTY journey proves cancellation preserves configuration,
saving preserves existing routes, and the discovered fake executable is never invoked.

Full verification exposed recurring account and run ownership locks after guard drop.
Three deterministic regressions reproduced retained locks with duplicate descriptors before
the fix. Account and Store guards now explicitly unlock on drop, releasing locks even when a
child retains inherited descriptors. Tests also prove that independent account owners and
live stores still block maintenance/reopening. All three regressions pass after the fix.

All 37 CLI library tests, all eight PTY journeys, the focused storage-backup and voice-workflow
suites, and the complete `scripts/check.sh` passed. Full log:
`/tmp/retold-discovery-release-final.log`. No live provider calls or production media exports
were performed. Remaining TUI work, including onboarding route suggestions and fresh-install
acceptance, is tracked in `docs/plan-tui.md`.

## Account reconciliation preview (2026-10-21)

`account reconcile --attempt <id> --cost <usd> --dry-run` computes the existing
reconciliation transition against a read-only state copy. JSON includes lifetime totals
before and after; plain output shows charged and held money changes. The preview grants
no dispatch authority and is advisory: applying reconciliation revalidates current state.
CLI regression coverage checks a nonzero hold, predicted versus applied totals, unknown
attempt rejection, and unchanged accounting database bytes for both JSON and plain previews.
Dedicated TUI attempt selection and reviewed reconciliation remain open in `docs/plan-tui.md`.

The complete `scripts/check.sh` passed after this slice, including strict Clippy,
the unchanged Clean Code ratchet, architecture, dependency/security checks, rustdoc,
all nine PTY journeys, workspace tests and doctests. Log:
`/tmp/retold-reconcile-preview-check.log`. No live provider calls or production exports
were made. Changes remain local and uncommitted.

## TUI money, authoring and workbench continuation (2026-10-22)

Recovered the interrupted local implementation and repaired its doctor assertion to verify
`doctor --fix` arguments rather than reading a flag as text. The flag accessor itself now
returns boolean values, so an explicit dry run skips the apply-preview wrapper. A regression
checks both dry-run and apply paths. Generic previews use the title "Review change" for both
financial and document edits.

Money adds saved scope/window limits, attempt pagination and guarded diff previews. Structured
guide/channel edits validate before publishing, retain the reviewed document hash, and preserve
sidecars. Real PTY journeys now exercise cancellation and application for limits and guide fields.
Document editing starts with the name; Ctrl-U clears existing text before replacement.

The workbench additions include bulk archive/trash selection, review filters and metadata
comparison, queue controls, saved series parts and audio row playback, and system reports.
Mouse-wheel input now follows the active page/modal rather than scrolling the underlying page.
The thousand-video render regression exposed redundant collection projection and cloning;
unfiltered collections now avoid the second clone and the list renderer reuses its projection.
The original 150 ms first-frame and 16 ms median frame limits are unchanged.

`docs/tui.md` documents actual controls and terminal integration limits. `docs/plan-tui.md`
separates implemented work from still-open product and acceptance requirements. The full TUI
plan is not complete: see that remaining-work list before continuing. Changes remain local.

Verification: the complete `./scripts/check.sh` passed with exit status 0, including strict
Clippy, the unchanged Clean Code ratchet, opt-in acceptance linting, architecture/reachability,
unused dependencies, advisories/licenses, rustdoc and every workspace test/doctest. All 63 CLI
library tests passed, as did the 13-test PTY suite. `git diff --check` passed. Full gate log:
`/tmp/retold-tui-resume-final.log`. No live provider calls or production video exports were made.


## Bundled guide selection and authoring acceptance (2026-10-22)

`guides create --example story-recap` explicitly copies the bundled guide into a new user-owned
identifier, including English/Hindi/Urdu writing sidecars and the two narrative examples.
The package is embedded in the application, so installed binaries do not depend on the source
checkout. Optional display names are preserved; the target identifier is rewritten. Nothing is
preinstalled. Example/import sources reject scaffold flags, and `--from`/`--example` conflict.
Dry runs validate without publication; existing resources are never overwritten.

The guide wizard exposes the example choice, and F3 now opens a value picker for fixed choices
rather than a filesystem browser. Picker copy distinguishes choosing a value from searching
commands. Snapshots record the example picker at 80×24, 120×40 and 200×60. A real fresh-HOME
PTY creates the example guide and a channel with explicit voice/font settings, then runs both
configuration checks from the TUI. No provider is invoked.

Structured document lists are editable as arrays, including initially empty lists. The editor
preserves the list type and the application validates element types and the complete manifest
before publication. Long current values are truncated and the input follows its cursor so lists
do not hide the editor controls. Omitted optional fields and per-item list controls remain open.
The authoring and terminal guides now explain the actual TUI workflow and source choices.

Verification passed: `./scripts/check.sh` exited 0, including all 66 CLI library tests,
14 PTY journeys, workspace tests/doctests, strict lint, Clean Code, architecture, dependency,
security and rustdoc checks. The final `./scripts/check.sh --fast` also exited 0 after preserving
individual array-item controls. `git diff --check` passed. Logs:
`/tmp/retold-tui-author-final.log` and `/tmp/retold-tui-author-final-fast.log`.
Changes remain local and uncommitted. No live provider calls or video exports were performed.
The full TUI plan remains partial; see the current remaining-work list in `docs/plan-tui.md`.


## Recovered TUI completion slice (2026-10-22)

Recovered the interrupted draft-defaults, review, delivery and studio acceptance work.
The saved-artifact PTY records a note, approval and unlock and verifies unchanged image bytes.
Its fixture setup now separates manifest lookup from image creation so the unchanged Clean Code
ratchet passes. A regression also checks that switching guide/channel defaults replaces remembered
values while preserving an explicitly changed quality setting.

The plan and terminal guide now reflect the existing workspace-scoped draft defaults,
reviewed bulk tagging, saved delivery coverage, pinned image comparisons, failed-status filter,
studio start/stop PTY and mouse confirmation controls. Broader draft-to-export, background job,
recovery, language filter and platform acceptance remain open. Changes remain local and uncommitted;
no live provider calls or production video exports were performed.

Verification: `./scripts/check.sh` exited 0, including strict Clippy, unchanged Clean Code
ratchet, architecture/reachability, dependency/security checks, rustdoc, all 76 CLI library tests,
all 16 PTY tests and the remaining workspace tests/doctests. Log:
`/tmp/retold-tui-continue-check.log`. `git diff --check` also passed.


## Review language acceptance and stale preview fix (2026-10-22)

Finished the existing L-key language filter slice. The saved-artifact header now identifies
its selected language, and an empty filtered result explains how to change filters instead
of suggesting production. Reproduced and fixed a stale-preview bug: when selection has no
artifact, refresh clears both the requested path and preview, so delayed worker results cannot
repopulate the old file. Cycling returns to all languages; shared files remain visible there.

Reducer coverage checks language cycling, action target paths, empty-state recovery, delayed
preview rejection and rendering at 80x24, 120x40 and 200x60. A real PTY verifies the language
indicator and empty-result message while preserving all saved library file hashes.

Verification: `./scripts/check.sh --fast` exited 0 with unchanged Clean Code baseline;
`cargo test -p retold-cli --lib --test tui_pty` exited 0 with 78 library tests and 17 PTY tests.
`git diff --check` passed. Logs: `/tmp/retold-language-fast.log` and
`/tmp/retold-language-cli-tests.log`. The full workspace gate was not rerun for this focused
TUI slice. Changes remain local and uncommitted; no live provider calls or production exports.


## Optional authoring field state clarity (2026-10-22)

Finished the presentation of the existing schema-declared optional-field editing path.
The document projection now preserves whether a field is missing. The list shows `(not set)`
instead of displaying a schema suggestion as a saved value; the editor explains that its initial
input is a suggestion. The existing typed replacement, validated preview, stale-hash guard and
explicit application path remain responsible for publication. Entire missing object sections
and richer list-item controls remain open in the plan.

Reducer coverage checks the omitted-field list/editor at all three breakpoints, a numeric
replacement, the reviewed document hash and cancellation without dispatch. Existing backend
integration tests verify optional-field dry-run/publication, unknown-field rejection, stale
hash rejection and sidecar preservation.

Verification: fast gate passed with unchanged Clean Code baseline; all 79 CLI library tests,
17 PTY tests and 5 authoring-document integration tests passed. `git diff --check` passed.
Logs: `/tmp/retold-optional-fast.log`, `/tmp/retold-optional-cli.log` and
`/tmp/retold-optional-backend.log`. Full workspace gate was not rerun for this focused slice.
Changes remain local and uncommitted; no live provider calls or production exports.

## Offline fixture and character-review recovery (2026-10-22)

Recovered the interrupted local fake-provider failure: delivery requests a thumbnail `ranking`
schema, which the HTTP stand-in did not handle. It now returns all three candidates in score
order with valid scores, reasons and usability flags. Importing the fixture no longer starts
its HTTP server. A CLI integration regression imports it and deserializes its answer into the
engine's actual `ThumbnailReview`, checking candidate coverage and score ordering.

The existing uncommitted character-approval implementation had a Clippy ownership error and
three new Clean Code findings. The approval helper now borrows its path; missing saved records
load as the requested type's default. No lint baseline was increased. A full-workflow regression
proves the review blocks continuation, explicit approval permits reaching ready, and a changed
sheet hash invalidates approval. It uses only in-process deterministic providers.

Verification: `./scripts/check.sh` exited 0, including all 28 PTY journeys, workspace tests and
doctests, strict lint, unchanged Clean Code baseline, architecture, dependency/security checks
and rustdoc. The final fast gate also passed after the added regressions; `git diff --check`
passed. Logs: `/tmp/retold-tui-recovered-check.log`, `/tmp/retold-tui-recovered-fast.log`,
`/tmp/retold-provider-fixture.log`, `/tmp/retold-character-approval.log`.

Changes remain local and uncommitted. No live provider calls or video exports were performed.
The historical `/tmp/fx` workspace and its helper scripts are absent on this host, so that
interrupted export was not resumed. The HTTP-fixture draft-to-export journey, dedicated
process/backup screens and real-terminal/platform acceptance remain open in `docs/plan-tui.md`.

## Dedicated process inspection and account backups (2026-10-22)

Money → Processes now uses a selectable, filterable collection rather than a prose report.
Rows retain status and owner purpose; details show saved owner/supervisor/worker identity and
live observations without interpreting uncertainty as permission to clear or stop anything.
The screen has no mutating row actions. Cursor pagination uses exact cursor strings and keeps
its history independent of billing attempts. Compact snapshot and reducer coverage exercise
uncertain identity evidence, filtering, pagination boundaries and absence of action dispatch.

Money → Backups now explains account recovery scope, excluded secrets/media, reviewed creation
and offline verification. Existing account handlers still own publication, maintenance barriers
and checksum validation. No backup inventory is invented: saved folders are chosen by path.
The terminal guide and plan describe both screens. Two new real PTYs prove that cancellation
preserves accounting and creates no destination, and that confirmed creation can be verified
from the same tab while leaving the source account bytes unchanged.

Verification: `RUST_TEST_THREADS=2 ./scripts/check.sh` exited 0, including 102 CLI library tests,
all 30 PTY journeys, strict lint, unchanged Clean Code baseline, architecture, dependency/security,
rustdoc, workspace tests and doctests. Log: `/tmp/retold-process-backup-check-bounded.log`.
`git diff --check` passed. The default-concurrency full run failed in the existing editor-handoff
and audio-preview PTYs (watchdog exit / shim-output timeout); both passed when rerun individually.
Those fixed-delay tests appear sensitive to concurrent load, but the cause is not confirmed.
No test assertions, timeouts or lint baselines were weakened for the successful bounded run.

Changes remain local and uncommitted. No live provider calls or production exports were made.
Draft-to-export and real-terminal/platform acceptance remain open in `docs/plan-tui.md`.


## Published checkpoint and P9 readiness follow-up (2026-10-22)

Committed the accumulated TUI work as `c1d240f` and pushed `HEAD` to `origin/main` with
an explicit force-with-lease against the observed remote SHA. Commit/push hooks were bypassed
at the user's explicit request; no tests or lint checks were run for this publication.
The earlier two-thread full-gate evidence remains recorded above.

Continued P9 with local, unverified PTY harness changes. Editor and audio journeys now wait
for loaded rows, editor return and published shim records rather than fixed sleep intervals.
Screen waits search output produced after the latest key, avoiding old transcript matches.
Each readiness wait is bounded to five seconds; the existing twelve-second journey watchdog
is unchanged. Shim records publish by rename after all arguments/input are written. Failed
waits kill the fixture process and include the missing condition in the captured transcript.

These follow-up changes have not been tested, linted, committed or pushed. Do not claim that
they fix the default-concurrency failures until the affected journeys and full gate run again.
No live provider calls or production exports were performed.


## P9 host-program readiness continuation (2026-10-22)

Extended the local readiness-wait work to clipboard copy (including Linux xclip fallback)
and the host file viewer. The journeys now wait for the relevant loaded screen and a complete
shim record before quitting. Recording scripts fail immediately on write/read/rename errors,
so failure cannot publish a partial record. The prior editor/audio waits and captured timeout
diagnostics remain in place. No journey watchdog, assertions or lint baselines were relaxed.

The user's instruction to skip tests and lint remains in effect: no checks, test runs or
formatter runs were performed. Changes remain local, uncommitted and unverified. The plan
explicitly retains the default-concurrency failure as unresolved pending execution, alongside
real-terminal/platform and draft-to-export acceptance.


## TUI acceptance recovery and full gate (2026-10-22)

Resumed the interrupted series PTY failure. Extracted its two-part-series fixture setup
into a named helper, retaining every assertion and restoring the unchanged Clean Code baseline.
The reviewed add-parts journey passes with the updated form field order.

Verification: `scripts/check.sh` exited 0 at default concurrency, including strict lint,
Clean Code, architecture, dependency/security checks, rustdoc, workspace tests and doctests.
All 30 PTY journeys passed (28.62 seconds); the earlier editor/audio timing failures did not
reproduce in this run. Both production journeys passed (148.69 seconds): local loopback
providers drive the CLI through character approval to ready, project export and a nonempty
rendered MP4; the real TUI/studio drives character review, ready and saved project export.
The host FilmCraft renderer participates in MP4 export. `git diff --check` also passed.
Log: `/tmp/retold-tui-final-check.log`. No paid or live-provider requests were made.

The plan records these proofs without treating them as complete platform acceptance.
Remaining: TUI-initiated rendered MP4 export, end-to-end series-part execution, and hand
checks on terminal emulators, multiplexers and Linux desktop integrations. Changes remain
local and uncommitted; this continuation did not publish a commit or push.


## Remaining TUI journeys and host qualification (2026-10-22)

The production PTY now starts a linked series part from the Series matrix, receives the
character-review blocker, approves all sheets through the TUI, resumes the part to ready,
and requests a rendered MP4 from the TUI Spend Gate. It waits for the actual studio job
result, probes the saved file for video/audio streams and positive duration, and reopens
the matrix to check `1/1 parts complete`. The separate CLI journey retains project and
rendered-video exports. Both focused production journeys passed in 93.17 seconds.

The journey exposed a real completion bug: the matrix used a registry flag that generation
never updated. Read-only series inspection now derives completion from a verified saved
timeline and nonempty ready deliveries. Invalidating delivery evidence clears completion;
the regression checks both states. No completion is inferred from submission or job status.
The fixture stops its studio on teardown, including assertion failures, and checks PTY exit
status. Test guide copies retain full-HD caption layout but use 5 fps for acceptance rendering,
avoiding concurrent full-HD/30 fps rendering contention. Product defaults are unchanged.
The original 120-second job wait remains; no lint baseline was raised.

Real-host smoke evidence: Terminal.app 2.15 rendered at 80×24, 120×40 and 200×60; Ctrl-C
restored the exact original stty mode. Captured screens are in
`/var/folders/4y/vzylswt90kj7skqcxf_6ptvh0000gn/T/retold-terminal-dwk99jv3/`.
GNU Screen 4.00.03 passed launch, Videos navigation and Ctrl-C exit; transcript:
`/var/folders/4y/vzylswt90kj7skqcxf_6ptvh0000gn/T/retold-screen-2wni9q04/screen.raw`.
Real macOS afplay exited 0 for a 100 ms silent WAV. These are smoke checks, not a human
usability study or audible-quality assessment. Linux desktop integrations and additional
terminal emulators cannot be qualified on this macOS-only host.

Focused log: `/tmp/retold-tui-completion-journeys.log`. The final full gate is recorded below.
Changes remain local and uncommitted; all providers are local zero-price loopback fixtures.

Final verification: `scripts/check.sh` exited 0 at default concurrency against the final code,
including strict lint, unchanged Clean Code baseline, architecture/dependency/security checks,
rustdoc, workspace tests and doctests. Both production journeys passed in 90.15 seconds; all
30 PTY journeys passed in 27.88 seconds. Log: `/tmp/retold-tui-completion-check.log`.
`git diff --check` passed. No live-provider calls, paid calls, commit or push occurred.
