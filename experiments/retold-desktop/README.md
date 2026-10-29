# Retold Studio

The native desktop app for Retold, built with GPUI Kit `=0.8.0`. It covers everything the
`retold` CLI does: workspaces, guided video creation, script, scene and character review,
narration, exports, the background renderer, providers and spending limits.

## Run

From the repository root, with Rust 1.99 and the platform dependencies from
[GPUI Kit installation](https://gpui-kit.com/docs/installation/):

```sh
cargo run --manifest-path experiments/retold-desktop/Cargo.toml \
  --target-dir target/gpui-g0 --locked --bin retold-studio
```

Release builds and installers come from `scripts/package.sh` and the Release workflow.
Headless CLI packages do not gain GPUI dependencies.

## Workspace shell

- Home, Videos, Activity, Settings and Help, with light/dark appearance and a 760 × 640
  minimum window. Workspace selection uses a native folder chooser.
- Create a managed workspace after reviewing its destination/name. Import a guide or
  channel through the shared validators, under an explicit ID, without overwriting resources.
- Read actual saved videos and installed guide/channel choices. Search and page through the saved catalog, with sort and archive filters; invalid manifests and listing limits are visible. A failed refresh preserves
  the last successful workspace snapshot.
- Save a genuine offline video draft using the installed guide's questions, channel and
  narration editions. Choice questions use explicit choices. The video cap starts at $0;
  saving makes no provider requests and admits no job. Guide switching retains field edits.
- Review a run with separate $0 / zero paid-request defaults. Show its exact hash, stages,
  provider-account bindings and blockers. Confirming submits through the shared hash check
  and stable idempotency key; it does not silently accept a new plan. Run stops at ready.
- A packaged `--worker --library PATH` mode branches before GPUI initialization. Explicit
  startup can run saved pending jobs; closing the GUI leaves that worker running. Shutdown
  drains its owned work. Long Unix socket paths use a private, validated short directory alias.
- Activity has bounded queue pages, saved job/event inspection, a 128-event visible window,
  pause/stop/resume and queued-job reordering. Observing saved events grants no execution.
- Stop all has its own request lane, available while the normal lane is occupied. It
  persists an atomic workspace queue hold, affected-job receipt and stop transitions before
  requesting cancellation. Active work remains stopping until a terminal outcome is saved.
  Holds survive worker restart. Enabling the queue leaves stopped jobs stopped.
- Review and edit actual master-script sections, compare saved text, handle another writer's
  changes while retaining local text, approve current content, undo/redo and browse revisions.
  Script saves invalidate downstream work while keeping research/writer cache reusable.
- Native command search, menus and Cmd/Ctrl+K/O/N/R shortcuts share the same actions.
- Narrow windows use a navigation drawer. System/light/dark appearance, high contrast,
  100–200% text, window size and recent workspaces persist without saving run authority.
- Unsaved script/video/workspace/import changes are protected by the native quit prompt.
  A retained script draft stays reachable after navigating or searching elsewhere.

All filesystem, SQLite, process and planning work runs outside input/render callbacks.
The native controls use typed `retold-app` contracts. An external `RETOLD_STATE` queue
outside the selected workspace is rejected by native queue/run controls.

This is not full command parity or a release-qualified GUI. Series, asset administration,
provider/budget/storage screens, semantic scene/localized editing and explicit delivery/export
are still pending. Activity uses explicit refresh/replay rather than live subscriptions;
catalog queries still use a bounded filesystem scan. See the
[navigation and presentation checkpoint](../../docs/desktop/g2-checkpoint.md).
See the [script editing contract and evidence](../../docs/desktop/script-editing.md).

## Fictional input and media lab

Help opens the original G0 review fixture with English, Hindi and Urdu session drafts,
notes, approvals and a cancellable slow-query check. Those fictional edits are session-only.
Open media uses the native file chooser to decode existing audio/video/image files off-thread.
It does not generate or export media.

Media preview needs host `ffmpeg`, `ffprobe` and `ffplay` on PATH. Video decode is bounded to
640 × 360 BGRA at 12 fps with a one-frame mailbox. Metadata/output reads, cancellation,
timeouts and child reaping are bounded. Audio and video do not yet have a shared clock;
this adapter remains experimental. The built-in lighthouse is repository-owned SVG artwork.

The explicit `--worker-probe EMPTY_DIRECTORY` mode initializes the shared StudioServer in an
empty scratch directory and exits without ticking or dispatching any queue job.

## Verify

Requires Python 3.11 for the standard-library TOML policy guard, plus Cargo Clippy and deny:

```sh
bash experiments/retold-desktop/check.sh
```

This runs formatting, warnings-as-errors Clippy, behavioral tests, real Metal rendering and
native interaction tests, display-free packaged worker tests, a default application build,
policy-guard regression tests and the desktop dependency audit.

The user approved narrowly pinned desktop-only exceptions for eight unmaintained packages
and two exact `0BSD` packages. The [exception record](../../docs/desktop/dependency-exceptions.md)
and `audit-policy.py` enforce those bounds and parity with the unchanged root policy.
No platform filter or blanket maintenance suppression is used.

Native captures go to `/tmp/retold-gpui-g0-evidence`; override with `RETOLD_GUI_EVIDENCE`.
The shell tests cover three window sizes, multilingual guide input, an isolated actual draft
save, zero run defaults, blocked planning, 66-job pagination, stop during normal-lane occupancy,
and saved terminal-event inspection. They never start paid providers or export production media.
The draft journey runs in a child process with isolated installation/home paths.

Audio lifecycle tests use SDL's dummy driver. They prove cancellation/reaping, not audible
quality. HTML concept captures do not substitute for these native tests. Human IME/VoiceOver,
full bidi/caret behavior, sustained media/input performance, A/V sync, native OS dialogs and
cross-platform signed packaging remain separate acceptance gates.
