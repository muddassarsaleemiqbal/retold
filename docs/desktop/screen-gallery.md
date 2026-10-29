# Native screen gallery

Captures are written to `/tmp/retold-gpui-g0-evidence` by the native harness. They are
actual GPUI/Metal frames. The interactive HTML concept is a separate design artifact.

| Surface/state | Evidence family | Behavioral checks |
| --- | --- | --- |
| Welcome and open workspace | `shell-*-welcome-dark`, `shell-*-home-dark` | Read-only open; no implicit worker |
| Guided draft | `shell-*-draft-*` | Guide choices, Unicode, zero cap, actual offline save |
| Blocked run review | `shell-1440x900-saved-draft-blocked-plan` | Zero allowance, blockers, no admission |
| Queue hold | `shell-*-held-light` | Separate Stop lane; durable hold; stopped/stopping distinction |
| Job inspection | `shell-1440x900-job-stopped-dark` | Saved terminal outcome and bounded event replay |
| Actual script approval | `shell-*-script-approved-*` | Persisted content, current-hash approval |
| Script conflict | `shell-1440x900-script-conflict-dark` | Stale save refused; local multilingual text retained |
| Command search | `shell-*-command-search-dark` | Native selection/confirmation, shortcut reopen, Escape |
| Older library search | `shell-1024x700-catalog-older-search-dark` | 201-video fixture, pagination, archive, older selection |
| Persisted presentation | `shell-760x640-preferences-*` | Appearance/zoom/window restore; run defaults remain zero |
| Provider save review | `shell-1024x700-provider-reviewed-change-dark` | Offline setup, stale save refusal and retained form |
| Offline provider status | `shell-1024x700-provider-offline-health-dark` | Saved observations; no implicit test or authentication claim |
| Installation budget | `shell-1024x700-budget-month-headroom-dark` | Exact ceiling/headroom; read-only missing account and period/group changes |
| Explicit export review | `shell-*-export-blocked-*` | Separate project/render authority, invalidated scope changes and blocked confirmation |
| Completed native export | `target/gpui-g0/evidence/native-production/shell-*-native-*` | Zero-price fixture, explicit allowance/review, one completed job and actual MP4 streams |
| Saved character sheets | `target/gpui-g0/evidence/native-production/shell-*-native-character-*` | Actual artifact preview, stale approval refused, refresh, current-hash approval and continuation to ready |
| Fictional review/media lab | `scene-*` | Input/shaping fixture and bounded existing-file preview |

The compact presentation cases include 125%, 150% and 200% text. A control's own bounds
must fit the relevant scroll viewport after it is brought into view; the global Stop action
must remain reachable in the header. Preserve failing captures when diagnosing layout.

Remaining release evidence includes real IME composition/VoiceOver, common flows on other
qualified platforms, media synchronization/resource stress, first-launch/source-backed draft
creation and packaged-worker media execution. A gallery row does not establish an entire phase.

| Every destination | `shell-*-gallery-<destination>-dark` | One capture per destination (Home, Videos, Series, Activity, Providers, Budget, Settings, Help) at 760×640, 1024×700 and 1440×900 with a workspace open |
| Light and 200% text | `shell-*-gallery-{home,series}-light`, `shell-1440x900-gallery-{home,series}-text200` | Appearance toggle and text-size preference applied, captured and restored |
| High contrast | `shell-1440x900-gallery-{home,series}-contrast` | High-contrast preference applied, captured and restored |
