# Retold reliability audit

Started 2026-10-23 at `f4941d3`. This is an active audit, not a declaration of production
readiness. Exercise destructive, corrupt-state, concurrency and interruption scenarios only
in disposable workspaces. Real account data and provider profiles remain untouched. Paid
requests and production media exports require separate authorization.

## Acceptance passes

1. Configuration and provider evidence: path resolution, separate workspace isolation,
   missing or malformed configuration, credential redaction, truthful status and stale evidence.
2. TUI behavior: keyboard and mouse focus, Unicode and pasted input, small and resized
   terminals, asynchronous replies, repeated submissions, cancellation and confirmation gates.
3. Durable state: interrupted jobs, queue history, accounting holds and limits, inactive
   restores, malformed backups, stale locks and process ownership uncertainty.
4. Performance and qualification: bounded large-library rendering and inputs, repeated
   startup/exit, local fixture workflows, full repository gate and release executable smoke checks.

Each finding needs a reproducible trigger, a narrow fix, regression evidence and a verified
commit. Preserve detailed command logs under `.git/` rather than `target/`. A local fixture
pass does not qualify real provider image/audio quality or another operating system.

## Findings and checkpoints

- Providers overview displayed static inventory `not_checked` fields even after successful
  live tests. Fixed by showing the recorded health and offline profile evidence with periodic
  refresh; provider metadata and routes remain available. The regression failed before the
  fix. Nine status tests, both provider PTYs, the full gate including all 31 PTYs, and a release
  TUI smoke check against the configured providers passed. Log: `.git/provider-display-full.log`.
  Committed as `c6fcced`.
- Reproduced: offline login inspection labels an empty profile directory or any nonempty
  key variable as authenticated. Presence alone cannot verify a login or key. Both regressions
  fail against the old code. The fix retains the boolean field without claiming success and
  adds `authentication_checked: false` so unknown authentication is distinct from a failed
  verification. No credential contents or provider requests are needed for this inspection.
- Reproduced: a default CLI provider ignores inherited `CODEX_HOME` or `CLAUDE_CONFIG_DIR`
  during offline inspection, while the actual adapter inherits it. The new regression uses
  isolated homes and profile folders; inspection must report the same profile as execution,
  with explicit provider configuration taking precedence.
- Reproduced three configuration isolation failures: `RETOLD_LIBRARY` was interpreted as
  a flat configuration root even when `RETOLD_CONFIG_DIR` selected another folder; Home
  borrowed global providers for an explicitly selected portable root; an invalid explicit
  workspace still displayed global provider health. Three regression tests failed before the
  fix. Library environment overrides now remain library-only, explicit command-line roots
  retain precedence, and Home uses the same resolved context as the other commands.
- Reproduced: paste events were ignored and bracketed paste was not enabled. Nine reducer
  regressions failed before the fix. Paste now belongs to the active form, filter or editor;
  line breaks become spaces, controls never become keys, and oversized fields are refused
  atomically with an explanation. Terminal paste mode follows launch, editor handoff and exit.
- Reproduced: clipboard child processes ran synchronously on the event loop without a
  timeout. Both reducer and finite-stall PTY regressions failed before the fix. Clipboard work
  now runs on the action worker with one pending request, private file input, silent output,
  a three-second timeout, direct-child termination/reaping and tracked quit completion.
- Reproduced five automatic-preview failures with synthetic files: a harmless symlink name
  disclosed an `auth.json` target, normal filenames inside credential profiles were disclosed,
  image-suffixed directories were decoded as images, image-suffixed FIFOs blocked the read
  worker, and bounded UTF-8 previews misclassified a split final character as binary. All five
  regressions failed before the fix. Preview now checks requested/resolved sensitive paths,
  rejects special files before decoding, and preserves the valid prefix at a UTF-8 boundary.
- Reproduced five freshness failures: an invalidated read replaced the visible cached value
  and cleared its refresh request; removing an in-flight preview allowed a second overlapping
  read; duplicated wants dispatched duplicated work; cached financial previews and production
  plans could still dispatch while invalidated. All five regressions failed before the fix.
  Dispatch now starts a fresh read, invalidation rejects its obsolete reply, pending removals
  retain their in-flight identity, and review gates require current completed evidence. Completed
  event-page removals still release their cache slots; 1,000 pages leave no tombstones.
- Reproduced in a real PTY: a successful workspace switch left the old provider draft visible
  in the new root. The transition now waits for tracked replies, clears root-scoped pages,
  selections, cursors, previews and draft choices, and loads the selected root's preferences.
  Confirmation explains the unsaved-draft reset. New mutations wait while a switch is pending;
  quit intent survives completion. Failed and cancelled switches retain the original draft.
- Initial onboarding now waits for a current workspace reply after an initial read is
  invalidated. Its regression failed before that guard; an obsolete reply cannot establish
  that setup is missing.
- Reproduced external default-workspace drift for both managed roots and separate legacy
  config/media paths. The formal PTYs fail before the fix. A worker now retains its resolved
  context for commands and local preference/resource tasks without changing process environment;
  account administration and explicit workspace selection remain global. The PTYs change the
  default in another CLI, update the original provider inventory, refresh it, then confirm a
  mutation in that original root while preserving the destination. The split-layout case also
  reads its original saved video. Changed/deleted managed layouts fail closed; global help stays usable.
- Reproduced: default workspace inspection ignored a configured separate media folder and
  reported the fallback `HOME/Retold` paths. Its CLI regression fails before the fix. Workspace
  show/check and Home can now consume the same resolved context, including separate config/media.
- Six preference regressions failed before the fix across delayed choices, hidden read errors,
  followed symlinks, blocking FIFOs and nonobject JSON. The delayed-choice cases also verify
  intentionally cleared fields and focus. Hydration preserves touched fields; regular-file and
  byte bounds precede reads, JSON objects are required and failures produce a notice without
  repairing files.
- Reproduced: reopening a production review reused its earlier plan without requesting a fresh
  read. The regression fails before the fix; each reopened review now invalidates its old plan.
- Five health regressions failed before the fix: expired cooldowns hid later success, login
  failure and ordinary failure; active cooldowns hid a new login failure; an empty inventory
  skipped validation of a corrupt ledger. New outcomes now replace old observations while
  preserving any active longer admission deadline. Login failures remain admission blockers.
  Failure streak updates now share the ledger's writer lock; a concurrent-writer regression
  verifies that every accepted write contributes to the shared alias count.
- Health inspection now validates one ledger snapshot per inventory instead of parsing the
  entire ledger for each provider. The same 1,000-history-record fixture at 300 providers fell
  from an 848 ms median to 19 ms in the debug executable. These are local measurements, not
  a cross-platform latency guarantee; both runs include a cold-start outlier in the one-provider
  samples. Evidence: `.git/audit-health-scaling-before-refresh.json` and
  `.git/audit-health-scaling-after.json`. The fixture configuration and ledger stayed unchanged.
- Offline login copy now scopes the authentication limitation to this inspection, preserving
  the separately recorded live outcome and timestamp. Home advertises the implemented `new`,
  `series new` and `doctor` commands. Plain Home also had a second obsolete action mapping;
  its integration test caught that mismatch. A separate regression reproduced plain Home
  rendering a recorded cooldown as unchecked; cooldown and elapsed evidence now remain distinct.
- Renderer regressions reproduced cursors outside zero-sized or narrow frames, filter cursors
  that ignored Left/Home movement, a long query's edited suffix disappearing outside its list,
  and the underlying filter cursor showing through a quit confirmation. Root cursor ownership
  now follows the top modal and stays within the frame. Filter titles use the existing grapheme
  input viewport and preserve the complete query. Additional red checks caught form and palette
  cursors on panel padding when no input cell fit; those owners now require visible input room.
  The zero-width test initially hit the test backend's display formatter, which chunks by width;
  the corrected regression draws the frame directly and independently checks cursor geometry.
- The PTY driver now compares complete terminal attributes before launch and after exit, in
  addition to checking cleanup sequences. A stand-in deliberately leaving echo and canonical
  mode off is rejected by that guard, while an unchanged stand-in passes. Two new PTYs stress
  repeated tiny resizes with an unsaved form, command palette and cancellable reviewed mutation.
  They preserve configuration and prove the form survives returning from the palette.

## Verified audit checkpoint

Configuration/evidence fixes are committed as `96438c0`. The combined configuration and
preview source passed `scripts/check.sh --fast` and the complete `scripts/check.sh`, with
the existing Clean Code ratchet unchanged. All 31 PTY journeys passed. Logs:
`.git/audit-configuration-preview-fast.log` and `.git/audit-foundation-full.log`.

The original full run caught a fixture inheriting the real `CODEX_HOME` after profile
selection was corrected. The fixture now removes both CLI profile overrides before applying
its explicitly supplied test environment. Its status assertions and timeouts were retained.

Six preview regressions cover the five reproduced failures plus preservation of ordinary
Urdu, Hindi and combining-character text. Separate fault probes covered 16 malformed,
oversized, symlinked, traversal, cursor, missing-state and missing-backup cases; each retained
the filesystem snapshot and returned one structured envelope without exposing the synthetic
credential value. Evidence: `.git/audit-readonly-faults.json`.

The input/clipboard checkpoint passed `scripts/check.sh --fast` and the complete
`scripts/check.sh`, retaining the Clean Code baseline. All 125 CLI unit tests, all 33 PTY
journeys and both production-fixture journeys passed. Logs: `.git/audit-input-focused.log`,
`.git/audit-input-fast.log` and `.git/audit-input-full.log`. The regressions include twelve
paste ownership/Unicode/boundary cases, two clipboard reducer cases and two host-child cases.

Clipboard PTYs wait for the visible completion acknowledgement as well as the shim's saved
input; the shim file can exist before the worker's completion reply reaches the UI. Exit
assertions, command-content assertions and timeouts remain intact. The stalled shim separately
proves that navigation stays responsive, host output stays out of the TUI and quit completes.

The release executable was rebuilt with all audit fixes through this checkpoint. Three isolated
release smoke checks covered ordinary paste, an actual 65,537-byte bracketed paste and a stalled
clipboard. All exited successfully, restored terminal modes, preserved fixture configuration and
sent no provider requests. The oversized paste was refused with the limit notice. The clipboard
used an isolated stand-in; the real host clipboard was untouched.
Evidence: `.git/audit-input-release-build.log` and `.git/audit-input-release-smoke.json`.

The freshness/explicit-workspace-switch checkpoint passed the fast and complete gates, with
the Clean Code baseline unchanged. All 138 CLI unit tests, all 36 PTY journeys and both
production-fixture journeys passed. Focused logs: `.git/audit-workspace-freshness-focused.log`,
`.git/audit-workspace-tui-green.log` and `.git/audit-workspace-cancel.log`.
Full gate: `.git/audit-workspace-freshness-full.log`.

The rebuilt release passed isolated switch and cancellation smoke checks. The destination's
existing files remained unchanged; source files were preserved, with the confirmed switch
updating only the remembered default. Cancellation preserved both complete fixture snapshots.
Terminal modes were restored and no provider requests were sent. Evidence:
`.git/audit-workspace-release-build.log` and `.git/audit-workspace-release-smoke.json`.
That checkpoint covered explicit transitions. The session-context checkpoint below extends
qualification to external changes of the remembered default.

The session-context/startup checkpoint passed fast and complete repository gates with the
Clean Code baseline unchanged. All 148 CLI unit tests, 38 PTY journeys, 17 workspace integration
tests, Home tests and both production-fixture journeys passed. Logs:
`.git/audit-session-all-focused.log`, `.git/audit-session-scope-fast.log` and
`.git/audit-session-scope-full.log`. Red evidence is preserved in `.git/audit-session-root-red.log`,
`.git/audit-preferences-red.log`, `.git/audit-reopen-plan-red.log` and
`.git/audit-workspace-inspection-red.log`.

The rebuilt release passed managed-root and separate-layout drift smoke checks, including a
reviewed provider mutation in the original root after another CLI changed the default. The
other workspace retained its complete file snapshot. A third release probe showed a FIFO
preference error and continued provider navigation while preserving the FIFO. All three exited
cleanly, restored terminal modes and sent no provider requests. Evidence:
`.git/audit-session-scope-release-build.log` and `.git/audit-session-release-smoke.json`.

The provider-health checkpoint passed focused engine and CLI tests plus the fast and complete
repository gates, with the Clean Code baseline unchanged. All 149 CLI unit tests, 38 PTY
journeys and both production-fixture journeys passed. The focused health suite has 13 cases,
including concurrent accepted-write counts and both active and expired cooldown behavior.
The CLI recovery regression executes only a free local stand-in and verifies the resulting
saved health and account job count. Evidence: `.git/audit-health-transitions-red.log`,
`.git/audit-home-health-red.log`, `.git/audit-health-transitions-green.log`,
`.git/audit-health-cli-integration.log`, `.git/audit-health-fast.log` and
`.git/audit-health-full.log`.

The rebuilt release recorded recovery and a later login failure after expired cooldowns using
three calls to an isolated fake Codex executable. The TUI displayed the matching saved outcome
alongside offline profile evidence and scoped authentication copy. Both inspections preserved
complete sandbox file snapshots and emitted alternate-screen and bracketed-paste cleanup.
No real provider requests were made. Evidence: `.git/audit-health-release-build.log` and
`.git/audit-health-release-smoke.json`. The driver disables Python bytecode writes so its own
cache files cannot contaminate application-preservation assertions.

Exploratory debug PTYs survived 33 total resizes across a provider landing page, an edited
provider form and the command palette, including 0×0, 1×1 and the compact-layout boundary.
Each target was observed, complete sandbox files stayed unchanged and exit cleanup sequences
were emitted. This proves survival, not cursor placement or visual usability at tiny sizes.
Evidence: `.git/audit-resize-probe.json`. Invalid-UTF-8 path creation was refused by the host
filesystem before Retold ran; it remains a Linux boundary probe, not an application pass.

The cursor/terminal checkpoint passed the fast and complete repository gates with the Clean
Code baseline unchanged. All 155 CLI unit tests, 40 PTYs and both production-fixture journeys
passed. The six geometry regressions cover empty and narrow frames, actual input-panel bounds,
supported-size cursor visibility, grapheme movement, long-query visibility and modal ownership.
All 40 PTYs now directly compare terminal attributes at exit, including ordinary editor handoff
and stalled clipboard completion. The guard itself rejects a stand-in that leaves echo and
canonical input disabled. Evidence: `.git/audit-geometry-red.log`,
`.git/audit-geometry-root-red.log`, `.git/audit-geometry-panel-red.log`,
`.git/audit-geometry-green.log`, `.git/audit-terminal-tui-focused.log`,
`.git/audit-terminal-resize-pty.log`, `.git/audit-terminal-guard-selftest.json`,
`.git/audit-terminal-fast.log` and `.git/audit-terminal-full.log`.

Both release executable names were rebuilt. Four isolated release probes passed: unsaved-form
and palette resizing, review cancellation, long-filter Home/End editing and `retold-next`
inspection. Complete sandbox snapshots stayed unchanged, complete terminal attributes matched
their pre-launch values and alternate-screen/paste cleanup was emitted. No provider requests
were made. Evidence: `.git/audit-terminal-release-build.log` and
`.git/audit-terminal-release-smoke.json`.

The terminal recovery pass reproduced three failure classes in isolated PTYs. A real caught
worker assertion invoked Ratatui's process-wide panic hook, disabled raw input while the TUI
continued and printed a private panic canary. Recoverable dispatch scopes now suppress that
hook on their own thread; fatal panics retain the previous restoring hook and additionally
disable mouse capture and bracketed paste. The regression exercises an actual assertion,
checks that the canary stays absent and navigates after the worker failure. A separate fatal
UI assertion verifies cleanup. Neither test needs a production panic-injection option.

External SIGINT, SIGTERM and SIGHUP previously bypassed terminal cleanup. The TUI now observes
session signals and drains tracked work through its ordinary shutdown path. Three idle-signal
PTY regressions verify restoration and unchanged workspace files; another sends SIGTERM while
a clipboard child is stalled and verifies its timeout, termination and reaping before exit.

An editor that ran `stty -echo -icanon` and exited unsuccessfully left those damaged settings
as the TUI's eventual restoration target. Unix editor handoff now snapshots and restores the
terminal settings around the editor invocation before enabling raw input again. The regression
checks continued raw input, the visible editor error, unchanged files and restored exit modes.

The PTY guard compares flags, speeds and control characters while normalizing only `PENDIN`,
the transient kernel pending-input marker. On macOS a standalone raw/restore sequence sets
that marker until the next read even when all mode settings match. Independent stand-ins prove
the guard accepts this transient state but still rejects disabled echo/canonical input.
Evidence: `.git/audit-termios-pending-input.json`, `.git/audit-tty-guard-proof.json`,
`.git/audit-worker-panic-red.log`, `.git/audit-panic-paths-red.log`,
`.git/audit-panic-paths-green.log`, `.git/audit-terminal-signals-red.log`,
`.git/audit-terminal-signals-green.log`, `.git/audit-editor-modes-red.log`,
`.git/audit-editor-modes-green.log` and `.git/audit-signal-clipboard.log`.

The recovery checkpoint passed the complete repository gate with the Clean Code baseline
unchanged, all 157 CLI unit tests, 45 TUI PTYs and both production-fixture journeys. Both release
executable names were rebuilt. Five isolated release probes passed: SIGINT, SIGTERM, SIGHUP,
the unsuccessful editor that changes terminal modes, and `retold-next` SIGTERM. Each preserved
the complete sandbox snapshot, restored terminal mode settings and emitted alternate-screen,
mouse and paste cleanup. No real provider requests were made. The first release editor probe
had a fixture root mismatch and timed out before reaching the editor; rerunning with the same
flat library/config layout as its regression passed. Evidence: `.git/audit-recovery-full.log`,
`.git/audit-recovery-release-build.log` and `.git/audit-recovery-release-smoke.json`.

The studio diagnostic pass repeated the complete 22-test execution suite six times without
reproducing the original reorder failure (132 passing tests). It did reproduce a separate
diagnostic-loss bug: a live studio's error reply or a malformed status response was silently
converted to an offline status. Queue pause/stop/unpause/move then instructed the user to start
studio, discarding the real failure. Two socket-level regressions and a queue-control regression
failed against the old behavior. Status now retains communication and remote errors alongside
local queue issues; queue controls return the diagnostic before checking the offline flag.
The existing genuine-offline queue tests still pass. This improves truthful failure reporting;
it does not identify the cause of the original intermittent failure. Evidence:
`.git/audit-studio-repeat-{1..6}.log`, `.git/audit-studio-status-red.log`,
`.git/audit-studio-status-green.log`, `.git/audit-studio-control-red.log` and
`.git/audit-studio-control-green.log`.

The diagnostic checkpoint passed the full repository gate without increasing the Clean Code
baseline, including all 22 studio-execution tests, 45 TUI PTYs and both production-fixture
journeys. Both release command names were rebuilt. Twenty isolated release probes verified
busy and malformed replies for studio status and pause/stop/unpause/move through `retold`
and `retold-next`. Every probe returned the expected diagnostic/exit code, sent only a ping
and preserved all sandbox files. No real provider requests were made. Evidence:
`.git/audit-studio-diagnostics-full.log`, `.git/audit-studio-diagnostics-release-build.log`
and `.git/audit-studio-diagnostics-release.json`.

The Linux VM was started successfully through the existing OrbStack installation. A native
ARM64 Linux container with Rust 1.99.0 passed all 22 studio-execution tests, 46 TUI PTYs and
58 provider unit tests, including process identity, owner/supervisor death and provider/helper
termination. Sources were mounted read-only; build and Cargo caches used separate volumes.
The studio and PTY suites also passed as UID 1000 with a container init reaper. A deliberately
missing init reproduced shutdown uncertainty: `/proc` showed orphaned zombie `sleep` processes
parented by PID 1, and Retold retained its recovery barrier. Adding `--init` restored all
22/46 passes; the exit-confirmation guard was not weakened. Evidence:
`.git/audit-linux-runtime.log`, `.git/audit-linux-unprivileged.log`,
`.git/audit-linux-unprivileged-init.log` and `.git/audit-linux-reaper-negative.log`.

Three isolated macOS release probes sent SIGINT/SIGTERM/SIGHUP while an editor had changed
terminal modes and was still running. Retold waited for its finite two-second editor to exit,
then restored terminal modes and preserved workspace files. This qualifies completed editor
handoff, not forced interruption of an unresponsive editor with unsaved work. Evidence:
`.git/audit-editor-signal-probe.log` and `.git/audit-editor-signal-smoke.json`.

The browser pass reproduced two failures. A directory over 2,000 entries silently hid the rest,
so a filter could show no matches without explaining its restricted scope. On Linux, one
non-UTF-8 filename panicked JSON serialization and hid valid sibling files behind a generic
worker error. The browser now reports partial listings and unavailable entries, keeps its
bounded scan, omits unsupported paths without manufacturing lossy replacement paths, and
returns an error for an unsupported directory root. Two macOS and four Linux regressions
failed before the fix. All eight macOS file/preview tests, ten Linux file/preview tests and
46 Linux PTYs passed; a live Linux probe retained the invalid filename while displaying valid
files and the unavailable-entry notice. Guide validation separately rejects unsupported asset
names; the browser probe creates its filename after guide discovery to isolate the browser.
Evidence: `.git/audit-browser-truncation-probe.log`, `.git/audit-browser-partial-red.log`,
`.git/audit-browser-partial-green.log`, `.git/audit-linux-browser-red.log`,
`.git/audit-linux-browser-green.log` and `.git/audit-linux-invalid-name-green.log`.

The browser checkpoint passed the complete macOS `scripts/check.sh`, including 159 CLI
unit tests and all 45 macOS PTYs. Both release binaries rebuilt successfully. Four seeded
release PTY probes then exercised 1,200 mixed navigation, modal, Unicode-paste and resize
events, including zero-sized terminals. Every process exited successfully, restored terminal
attributes and cleanup modes, and left parseable fixture configuration. Host editors/viewers
were disabled and provider configuration was empty. This is randomized interaction smoke
coverage, not proof of every action or generated-media quality. Evidence:
`.git/audit-browser-full.log`, `.git/audit-browser-release-build.log` and
`.git/audit-tui-chaos.json` (seeds `a11d`, `b00b`, `c0de`, `d00d`). The probe runner requires
Python 3.11 for `tomllib`; the host Python 3.9 failed before launching any probe.

The expanded ARM64 Linux workspace run exposed macOS-only account-directory assumptions
in the reconciliation and video-cost integration fixtures. Both now select the same native
platform paths as the existing storage fixture. All seven focused Linux tests passed. The
unfiltered workspace rerun then failed both production journeys; the CLI reported missing
FilmCraft I/O, and the TUI export job failed. The container has no external FilmCraft renderer.
A separate workspace run excluding exactly those two journeys passed 1,031 tests, including
all 46 Linux PTYs and all 22 studio-execution tests (131 suite results). This used the image's
installed Rust 1.99.1, rather than the repository-pinned 1.99.0, with `--init`, offline Cargo,
read-only sources and no external network. It does not qualify Linux renderer exports.
Evidence: `.git/audit-linux-workspace.log`, `.git/audit-linux-account-paths.log`,
`.git/audit-linux-workspace-rerun.log` and
`.git/audit-linux-workspace-without-filmcraft-journeys.log`.

Running the macOS gate concurrently with Linux media tests failed the unchanged 16 ms
render budget (30.8 ms median). With Linux idle, the focused 1,000-video check passed:
10 ms first frame and 10.7 ms median over 30 samples. The concurrent failure is preserved
in `.git/audit-browser-final-full.log`; idle proof is in
`.git/audit-browser-idle-performance.log`. The serial full macOS `scripts/check.sh`
then passed with the unchanged budget, all 159 CLI unit tests, all 45 PTYs, both production
journeys, workspace tests and doctests. Log: `.git/audit-browser-final-serial-full.log`.

## Full TUI acceptance continuation (2026-10-23)

The user accepted readability of all twelve live release captures across 80×24, 120×40
and 200×60. These are rasterized ANSI-cell captures of Home, Providers, a stepped form and
a destructive confirmation, not screenshots of every terminal emulator. Files are under
`.git/tui-acceptance/`; the review gallery was published in the conversation.

Real Linux xterm launch/exit probes at all three sizes restored the exact terminal modes.
Their screenshots exposed a Dark-theme failure on a white terminal: the app inherited its
background while painting light text. A renderer regression failed before the fix. Colour
themes now own the root canvas and every cleared modal background; Light has its own ANSI
fallback foregrounds. No colour continues to preserve terminal defaults. Evidence:
`.git/audit-linux-xterm.log`, `.git/linux-xterm-before/xterm-80x24.png`,
`.git/audit-theme-background-red.log`, `.git/audit-theme-background-green.log` and
`.git/audit-linux-xterm-final.log`. The refreshed screenshots show the owned dark canvas
on the same white-default xterm, and all three final exits restore the original modes.

A deliberately stalled editor ignores INT/TERM/HUP and retains an unsaved draft until a
separate release marker appears. The new PTY sends each session signal before releasing
that editor, verifies the draft is saved exactly, and checks terminal restoration on exit.
All three cases passed without a product timeout or forced editor termination. This proves
preservation and deferred shutdown; it does not promise bounded exit from an editor that
never exits. Evidence: `.git/audit-editor-stalled.log` and the permanent PTY regression.

Forty additional studio reorder runs passed (`.git/audit-studio-reorder-stress.log`). The
original intermittent failure is still unexplained. A separate deterministic test did
reproduce loss of a socket-path error: all connect errors became "not running". Only absent
or refused sockets now map to offline; other failures retain their I/O diagnostic. The
regression failed before the fix and all four client tests passed afterwards. Evidence:
`.git/audit-studio-connect-red.log` and `.git/audit-studio-connect-green.log`.

Linux export had a concrete implementation gap: its default FilmCraft executable and
Doctor inspection both used a macOS app path. They now share a native default: the macOS
app executable, `filmcraft` on Linux PATH, or `filmcraft.exe` on Windows PATH. Explicit
engine-session executable overrides retain precedence. Windows remains untested.
The Linux Doctor regression checks present/missing PATH entries without invoking the editor.
Its first full-run attempt omitted explicit workspace selection and therefore received a
setup error instead of a report; the corrected fixture supplies its isolated library. All six
focused Doctor/setup tests pass (`.git/audit-linux-doctor-native.log`). The failed fixture run
is preserved in `.git/audit-linux-completion-final-full.log`.

The official ARM64 FilmCraft 0.9.0 archive from
[the release](https://github.com/storytold/filmcraft/releases/tag/v0.9.0) matched its published
SHA256SUMS entry: `ee0a54c2154ab34600a6d1c98a3348ac3bbcc5f44961dfa42c2c16b3a6f13c79`.
Both Linux production journeys then passed with real FilmCraft under Xvfb/software graphics
in 143.24 seconds. They cover project/video export and the TUI series-part review/ready/export
flow, with ffprobe checking actual streams/duration. The full-workspace run is recorded below.
The container uses Rust 1.99.1, read-only sources, separate caches, an init reaper and no
external network during testing. Evidence: `.git/audit-linux-production-renderer.log`,
`.git/linux-acceptance-image/Dockerfile` and `.git/run-linux-production.sh`.

Real TUI command copying through xclip passed under isolated X11 and returned
`retold video list`. Real ffplay exited 0 against the silent fixture with SDL's dummy audio
device; this does not establish audible quality. Evidence: `.git/audit-linux-host-programs.log`.

## Next work

- Continue interruption, stale-response, process-ownership, recovery and performance passes
  before declaring broad confidence.
- Keep editor shutdown deferred until its owner exits, preserving unsaved work. The stalled
  editor regression covers that boundary; bounded forced exit is intentionally not promised.
- Investigate the intermittent studio reorder failure seen in the first recovery full gate:
  `studio_reorders_only_pending_work_while_an_existing_provider_remains_active` received
  `RG-STUDIO-REQUIRED` while moving pending jobs. The exact test and the complete 22-test
  studio-execution suite passed on immediate reruns; the cause remains unproven. Preserve
  `.git/audit-recovery-full-first-failure.log`, `.git/audit-recovery-studio-repro.log` and
  `.git/audit-recovery-studio-suite.log`. A later green gate does not close this finding.
- Extend platform qualification beyond the tested Terminal.app, GNU Screen and isolated
  Linux xterm/Xvfb environments as new supported host environments become available.

## Open qualification limits

- Live human navigation, audible playback quality and generated image quality. The user
  accepted capture readability at all three planned terminal sizes.
- Hardware Linux desktop/audio integrations and additional terminal emulators. The tested
  Linux environment is ARM64 OrbStack plus Xvfb/software graphics, not a hardware desktop.
- Production-provider generation and paid paths beyond isolated fixture and contract tests.
