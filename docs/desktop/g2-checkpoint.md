# Native navigation, catalog and presentation checkpoint

The shell now uses a shared destination registry, native menus and a keyboard command
palette. Cmd/Ctrl+K opens search, O opens a workspace, N creates a draft and R refreshes.
Search lists visible videos and offers an explicit full-library search when no command
matches. The Videos page uses typed shared catalog pages with search, sorting, archive
filtering, and next/previous navigation. Cursor authority includes every effective filter,
sort and page limit; changing them rejects an old cursor. The CLI uses the same projection.

A retained script draft stays reachable from other pages, even when a later search no
longer includes that video. Workspace refresh refuses a changed library mapping while
unsaved text or setup is present. Conflicts preserve local multilingual text and require
an explicit choice before replacing the saved version.

Below 60 rem the sidebar becomes a native navigation drawer. Search and Stop all remain
in the header. Presentation uses relative text sizes, System/Light/Dark appearance,
optional high contrast, and 100–200% text. Contrast contracts cover reading text, semantic
messages, focus and high-contrast controls. The native harness checks reachable controls
at 125%, 150% and 200% in a 760×640 window.

Presentation preferences persist appearance, text size, window size and at most eight
recent workspaces. Writes merge changed fields under a lock and publish atomically.
Malformed preferences are preserved and reported with their path. Failed saves retain
pending changes for explicit retry. Startup restores presentation and the last workspace;
it never restores spending authority or starts a worker. Production persistence is opt-in
for native test shells, and its real journey runs under an isolated home/configuration.

See [design tokens](DESIGN.md), [frontend contract](FRONTEND_CONTRACT.md), and the
[native capture inventory](screen-gallery.md). Native captures are actual Metal output
in `/tmp/retold-gpui-g0-evidence`.

## Verification and limits

The authoritative root `./scripts/check.sh` and standalone desktop `check.sh` both passed
for this slice. This includes unchanged quality/security gates, native Metal interaction,
packaged worker checks, and the existing isolated production/export acceptance journeys.
Verification includes the preserved pre-existing local edits in discovery, assets and canon;
those files are excluded from this desktop commit.

Focused shared tests cover preference validation/merging, read-only missing preferences,
lock release, cursor/filter mismatch, and CLI/typed catalog parity. Native journeys cover
older-video search across 201 records, pagination/archive, command focus and dismissal,
retained drafts/conflicts, responsive navigation, presentation restore and zero run defaults.

Catalog queries still use the bounded shared filesystem scan; this is not an incremental
index or evidence of the planned warm-query performance budget. Activity remains explicit
refresh/replay. Scene/cast/localized semantic edits, full administration, series/assets,
positive native generation/export, media synchronization, assistive technology and platform
release qualification remain open under [G0–G9](../plan-gui.md). This checkpoint does not
mark any entire phase complete.
