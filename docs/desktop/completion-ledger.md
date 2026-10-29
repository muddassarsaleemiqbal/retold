# Full GUI completion ledger

Scope: all G0–G9 gates in [the GUI plan](../plan-gui.md). A local checkpoint does not close a phase.
Updated: 2026-10-26, starting at `71de9e5`.

| Phase | Implemented evidence | Open implementation and qualification |
| --- | --- | --- |
| G0 | Pinned GPUI Kit graph, native Metal/input/media spike, production media player, worker mode, dependency policy | Real IME/VoiceOver, playback synchronization/resource measurements, advertised platform qualification |
| G1 | Typed workspace/run/export/script/character projections, shared queue inhibition, worker lifecycle and socket guards; enforced operation inventory (`docs/desktop/operation-coverage.tsv`: 107 of 144 CLI operations native, 37 with stated CLI-only reasons) | bounded live observation and failure/resource qualification |
| G2 | Native shell, command search, themes, zoom, persisted presentation, responsive navigation; Go menu and ⌘1–⌘8 destination shortcuts; per-destination capture gallery at three window sizes with light, high-contrast and 200% text variants | focus order and cross-surface race/large-text coverage |
| G3 | Workspace creation/open/import, source-backed guided drafts, catalog paging/search/archive browsing, provider observations, saved views, multi-selection archive/restore/tag, readiness checklist and reviewed example-guide copying | Path-with-spaces/Unicode native dialog evidence |
| G4 | Offline source-backed native draft save; hash-bound run/allowance; idempotent queue submission; prepared-resource draft-to-ready/export with packaged worker | Native first-launch account/resource preparation, series creation/linking and actionable prerequisite recovery |
| G5 | Authoritative script and scene-direction revisions/history/conflicts/impact, actual character image approval, bounded edition-aware saved-media review/playback, versioned video and artifact notes, reviewed semantic scene casting and per-edition line overrides and a read-only narration waveform workspace, reviewed frame/narration retakes (engine/app/native), retained script/scene/note drafts across restart, actual recorded scene frame and searchable sequence | Native cast-edit journey; sheet/single-edition retakes |
| G6 | Shared backend series/version/assets/audio APIs and existing CLI fixtures; native Series destination with creation, part matrix, guided linking and reviewed subset plans/runs; native materials provenance/take/usage inspection with integrity checks and music/SFX licence state | (Three linked parts reusing one cast are proven by `series_reuse.rs`; take choice is character-sheet approval; reviewed part additions are native.) and three-part reuse acceptance |
| G7 | Queue pages/replay/ordering/pause/stop, durable Stop all, worker close/restart guards; bounded live job following, event-flood/observer-restart and damaged-journal qualification | Screen-reader-validated recovery links (crash, blocked-provider and slow-consumer faults and a 500 ms activity budget are qualified) |
| G8 | Reviewed provider setup/routes, offline observations, exact installation budget reports; native workspace health with reviewed repair and redacted diagnostics, preset removal, profile selection and guarded archive/copy/trash; reviewed account limit edits and provider test plans; reviewed backup creation, offline verification and inactive restore; first-launch account creation, provider key login and reviewed attempt reconciliation; preset/profile creation and guide/channel creation and field edits | schema/sidecar authoring, cross-installation reconciled activation and advanced tools |
| G9 | Separate native export review, capped QA, fixture MP4 streams, delivery file browsing/playback/editor handoff, packaged worker controls; square and combined export shapes, preset/aspect selection, verified copy-to-folder with explicit overwrite and re-validated delivery history; unsigned macOS app bundle with checksummed inventory, archive digest and packaged worker smoke verification | clean-profile install on another machine and an upgrade across two released versions (the upgrade check exists); signing and qualified platform/user evidence |

## Execution order

1. Finish the core native creation/review/media journey and its durable backend contracts.
2. Finish series and reusable assets/audio, including three linked parts and version impact.
3. Finish live activity and every administration capability with reviewed financial/storage changes.
4. Complete operation coverage, native state/interaction/load/accessibility evidence, packaging and
   platform qualification. Run the complete shared and desktop gates on the integrated result.

Continue through these groups after verified commits. Keep unrelated engine and skill-file work
outside GUI commits. Preserve all existing accounting, content/plan hash, cancellation, inactive-root,
bounded-I/O and dependency-policy guarantees. Use isolated zero-price providers and synthetic media;
historical recap jobs remain outside acceptance.

## Current source and media acceptance

See [source and media contracts](source-and-media.md). The complete desktop gate passed locally:
`.git/retold-gui-source-media-desktop-final.log`. Both mandatory production journeys passed,
including a GUI-created source-backed draft, packaged-worker generation and the exported MP4's
native player. The suite still prepares guide/channel/account fixtures explicitly before GUI
creation; this is not a fully native first-launch setup claim. The complete shared gate also passed:
`.git/retold-gui-source-media-shared-final.log`. The Clean Code baseline remains unchanged.
The final targeted native production recheck also passed in 103.32 seconds, including the
Unicode source path ending in a space: `.git/retold-gui-source-media-native-final.log`.
No phase is closed by this checkpoint. Changes are local; publication is separate.

Read-only CLI inventory (2026-10-26): 168 command-tree nodes and 144 leaves were inspected through
the compiled CLI's help output in an isolated profile. Snapshot:
`.git/retold-gui-cli-inventory.json`. Runnable parent groups and option-dependent effects still
need the shared operation registry and exact coverage test; leaf count is not a completion metric.

## Current review-note acceptance

See [review-note contracts](review-notes.md). The full shared gate passed in
`.git/retold-gui-notes-shared-final.log`; the full desktop gate passed in
`.git/retold-gui-notes-desktop-final.log`, including both mandatory native production journeys
(102.49 seconds). Eight focused note regressions and the native conflict/double-click journey
passed. The TUI artifact-review regression now reads saved comments through the shared contract
while retaining its legacy approval, unlock and unchanged-media assertions. The Clean Code
baseline remains unchanged. The subsequent editing-recovery checkpoint adds restart persistence;
the original note checkpoint did not include it.

## Current editing-recovery acceptance

See [retained editing drafts](editing-recovery.md). The complete shared gate passed in
`.git/retold-gui-recovery-shared-final.log`; the complete desktop gate passed in
`.git/retold-gui-recovery-desktop-final.log`, including both mandatory native production journeys
(104.23 seconds). Eleven application regressions exercise persistence, concurrent updates/cleanup,
identity, corruption, limits and maintenance/inactive guards. Separate GUI processes prove script
and note restart recovery, explicit comparison with an external edit, lost-response deduplication,
unapproved production save and checkpoint cleanup without creating a queue. Compact light captures
were visually reviewed. The Clean Code baseline remains unchanged. These results do not close G5.
The final section-name comparison correction also passed all-target desktop Clippy, four native
library regressions and the entire native shell journey in
`.git/retold-gui-recovery-native-recheck.log`.

## Current scene-direction acceptance

See [saved scene direction](scene-editing.md). Thirteen focused engine editing regressions and
the real one-image reuse workflow passed in `.git/retold-gui-scenes-engine-final.log`; the shared
panel-group regression passed in `.git/retold-gui-scenes-panel.log`. Nine saved-media and thirteen
draft-recovery application regressions passed in `.git/retold-gui-scenes-app-final.log`. The fast
shared gate passed in `.git/retold-gui-scenes-fast-final.log`, preserving the Clean Code baseline.
The initial full desktop gate passed in `.git/retold-gui-scenes-desktop-final.log`, including both
native production journeys (114.47 seconds). Updated compact light, loaded-frame dark and keyboard
picker captures were visually reviewed. The final status-message correction passed the full desktop recheck. The scene slice is included
in the shared and desktop artifact-note integration below. No phase is closed by this work.

## Evidence requiring external participation or release access

The plan requires real IME composition, VoiceOver/task sessions, five representative usability
participants, and OS/architecture-specific qualification. Automated harnesses cannot certify those
human outcomes. Signed/notarized distribution requires release credentials; Linux qualification
requires a supported graphics session, and Windows additionally requires shared backend portability.
These requirements stay open until actually exercised. Complete independent implementation and a
reviewable package before requesting any missing release access or human acceptance.

## Current artifact-note integration

The parallax workflow failure in the interrupted scene gate was traced to a redundant check of
generated direction against the on-disk guide instead of the active pipeline guide. Generated
direction remains gated by its callers; manual revisions still validate their saved source. Both
parallax workflows, thirteen scene-editing regressions and the one-image reuse workflow passed in
`.git/retold-gui-scenes-regression.log`. The prior desktop recheck passed in
`.git/retold-gui-scenes-desktop-recheck.log`.

Artifact-specific note publication, version comparison/rebinding and retained targets are integrated
with the existing journal and native review surfaces. Four focused context tests, the original
eight note tests and recovery checks passed before the final integration run. Complete shared and
desktop gates are recorded in `.git/retold-artifact-notes-shared-final.log` and
`.git/retold-artifact-notes-desktop-final.log`. The complete desktop gate passed, including both
mandatory native production journeys (168.31 seconds), process-crash recovery after an intervening
scene edit, explicit note rebinding and a loaded-frame comment preserving media bytes. The dark
version-conflict and compact light recovery captures were visually reviewed. The complete shared gate also passed with the unchanged Clean Code baseline, all workspace tests
and doctests, strict lint, architecture/reachability, dependency/security checks and rustdoc. The
final all-target desktop Clippy recheck passed in `.git/retold-artifact-notes-native-lint.log`. No
phase is closed by this checkpoint.

## Continuation checkpoint (2026-10-28)

Added in this continuation: semantic scene casting, per-edition line overrides, a narration
waveform workspace, reviewed frame/narration retakes, the native Series destination (matrix,
linking, reviewed subset runs), materials provenance/usage/integrity with music/SFX licence state,
workspace health with reviewed repair and redacted diagnostics, preset removal, profile selection
and guarded archive/copy/trash. Each slice passed its focused regressions, strict lint and the
Clean Code baseline on a clean tree. The complete desktop gate (`experiments/retold-desktop/check.sh`,
including both native production journeys, audit policy and cargo-deny) passed on the integrated
result: `.git/retold-gui-continuation-desktop.log`. No phase is closed by this checkpoint.

Second continuation (2026-10-28): reviewed account limits and provider tests, backups (create,
verify, inactive restore), export square and combined export shapes, preset/aspect selection, verified copy-to-folder with explicit
overwrite and delivery history, an inventoried macOS app bundle with package verification, and
live job following with event-flood and damaged-journal qualification. The complete desktop gate
passed again on the integrated result: `.git/retold-gui-continuation2-desktop.log`.

Workspace journey (2026-10-28): the native production journey now opens Narration (edition,
waveform, loaded player, retake control), Edition lines (lines or the source-only Script hand-off),
Materials (detail and integrity control) and Series via ⌘3 on the generated video. It caught and
fixed a missing narration retake control. Both production journeys passed:
`.git/retold-gui-workspaces-native.log`; captures `native-narration-dark`, `native-lines-dark`,
`native-materials-dark`, `native-series-dark`.

## Final integrated checkpoint (2026-10-28)

Third continuation: first-launch account set-up, provider login and reconciliation; guide/channel
creation and field edits; preset/profile creation; inactive-restore activation; series creation and
reviewed part additions; saved views, tags and multi-selection; licensed audio import; square and
combined export shapes; readiness checklist and example-guide copy; Go menu shortcuts; capture
gallery (dark/light/high-contrast/200% text); enforced CLI→desktop operation inventory; restart
recovery for casts and edition lines; proof that approved edition lines reach speech; narration
seek; packaged-upgrade check; a 500 ms full-queue activity bound; and a production journey through
the newer workspaces.

The complete desktop gate passed (`.git/retold-gui-final-desktop.log`) and the complete shared gate
passed on a clean checkout of the same commit (`.git/retold-gui-final-shared.log`).

Remaining, by owner:
- Requires people: real IME and VoiceOver sessions; five-participant usability study; human visual
  review of the capture gallery.
- Requires credentials or machines: Developer ID signing and notarization; clean install on a second
  Mac; Linux graphics session; Windows backend portability; an upgrade across two released versions.
- Requires the in-progress engine canon work to settle: generating three linked parts that reuse one
  character version (the engine `canon.rs`/`assets.rs` edits in the working tree are the user's).
- Deliberately CLI-only (see `operation-coverage.tsv`): 33 operations such as developer creative
  tools, permanent deletion, bulk limit files and cross-installation reconciled activation.

## Owner decisions (2026-10-28)

The app is for a single user on this Mac. Code signing/notarization, a second-machine install,
Linux/Windows builds and a cross-release upgrade are not required. Real IME/VoiceOver sessions,
the usability study and human screenshot review are deferred indefinitely.

With those decisions, all remaining implementation is complete: the engine schema/sheet fixes were
finished and committed, three linked series parts are proven to reuse one cast, and every CLI
operation is available in the desktop app (typed pages for the common ones, Advanced tools for
the rest).

Completion run (2026-10-28): after the owner decisions above, the complete desktop gate
(`.git/retold-gui-complete-desktop.log`) and the complete shared gate on a clean checkout
(`.git/retold-gui-complete-shared.log`) both passed. A terminal-UI clipboard test that raced the
operating system's first-run scan of a freshly written stand-in program was stabilized by warming
the stand-in before the timed interaction.
