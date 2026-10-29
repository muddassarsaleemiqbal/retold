# G0 checkpoint: the first native GUI slice

Date: 2026-10-25. Status: **implemented native preview; G0 incomplete**.
Source: [isolated crate](../../experiments/retold-desktop/README.md).
Plan: [desktop GUI](../plan-gui.md).

## What is concrete

`experiments/retold-desktop` is a separate Cargo workspace with its own Cargo.lock.
It selects GPUI Kit `=0.8.0` and GPUI `=0.3.9` through the facade. The production
workspace membership, Cargo.lock, compiler floor, CLI/TUI entry points, and dependency policy
were not changed. The new crate uses the shared application crate only for the worker probe.

The native screen implements the first design direction, editable language editions,
per-scene drafts/notes/approvals, native file selection, existing-file media inspection and
experimental playback/seek, and a cancellable responsiveness check. No production job,
provider request, generation, rendering, or export action is connected.

Drafts are explicitly session-only. Select/edition changes capture the current text before
loading another draft. Narration changes revoke the current edition's approval; notes do
not. Media selection generations invalidate outstanding decode replies, and obsolete preview
processes receive cancellation. Media probes/timeouts and process waits run off the UI thread.

## Evidence

Measured host: macOS Apple Silicon, Rust 1.99.0. The default application binary builds and
remains running after a three-second native launch smoke. That owned preview process was
terminated after the smoke; no production process was stopped.

- Default `cargo build --locked --bin retold-studio`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed for the native crate.
- Contract tests: edition/scene isolation, approval invalidation, cancellation, existing
  audio inspection/source-byte preservation, bounded still-image decode, canceled playback,
  audio process termination, empty-state worker probe, and refusal of existing worker state.
- Process tests: oversized child output is rejected; cancellation of a nonproducing child
  reaps it without waiting for its full runtime.
- Final behavioral result: 2 process tests and 10 contract tests passed; no ignored cases.
- Main-thread GPUI Kit native test: real Metal images at 1440 × 900, 1280 × 800, and
  1024 × 700 in light/dark; narration role/name/visible bounds; actual pointer/key input;
  mixed-script retention, Hindi deletion/undo, edition/scene restoration, and session approval.
- Shared worker probe: a separate process initializes `StudioServer` in an explicit empty
  temporary directory without graphics initialization and reports `dispatched: false`.
- Production dependency architecture check: passed; production crate graph unchanged.

Native images: `/tmp/retold-gpui-g0-evidence/scene-1440x900-dark.png`, corresponding size/theme
variants, `scene-hindi-dark.png`, and `scene-urdu-dark.png`. These are native GPU captures,
not the earlier HTML concept. They were inspected for layout, clipping, frame aspect, themes,
and script rendering. This is a developer visual review, not user aesthetic acceptance.
Compact-height windows move review behind a labeled control. Native layout assertions also
verify that scene-selection buttons are not covered by the status bar.

Logs are under `/tmp/retold-gpui-g0-*.log`. They are local developer evidence, not a remote CI,
signed package, or production-provider result. Tests use existing WAV/PNG fixtures and a dummy
audio sink for lifecycle verification; they do not produce a new video or prove audible quality.

## Blocking findings

### Initial dependency policy result

Update 2026-10-25: the user approved narrow desktop-only exceptions. The pinned desktop
audit now passes; root policy is unchanged. See [the exception record](dependency-exceptions.md)
and [the continuation checkpoint](g1-checkpoint.md). The findings below preserve the original
strict-policy result; they are not the current desktop gate result.

`cargo deny --manifest-path experiments/retold-desktop/Cargo.toml --config deny.toml --locked check`
fails against the repository policy. Bans and sources pass. All-target findings:
The complete `check.sh` reached this final step after its formatting, Clippy, tests, native
captures, and default build passed, then exited with status 5 for these findings.

| Finding | Dependency |
| --- | --- |
| 0BSD is outside the current allowlist | `enum-iterator 2.3.0`, `enum-iterator-derive 1.5.0` |
| RUSTSEC-2024-0384: unmaintained | `instant 0.1.13` |
| RUSTSEC-2024-0436: unmaintained | `paste 1.0.15` |
| RUSTSEC-2025-0134: unmaintained | `rustls-pemfile` |
| RUSTSEC-2026-0206: unmaintained | `rustybuzz` |
| RUSTSEC-2026-0192: unmaintained | `ttf-parser` |
| RUSTSEC-2026-0371: unmaintained | `zbus_names` |
| RUSTSEC-2026-0370: unmaintained | `zvariant` |
| RUSTSEC-2026-0372: unmaintained | `zvariant_derive` |

Some findings are for other target platforms. They are reported rather than silently filtered
away. These are unmaintained advisories, not a claim that each is an exploitable vulnerability.
No advisory ignore, source allowance, renderer substitution, or license-policy exception was added.
Resolve with a verified compatible upstream graph or an explicitly reviewed narrow dependency
change before promotion. Do not mark the dependency gate passed from successful compilation.

GPUI also emits a future-incompatibility warning for transitive `block 0.1.6`; the selected
compiler accepts it today. Include it in the compatibility follow-up.

### Production full gate

`./scripts/check.sh` was attempted and stopped at formatting in the pre-existing
`crates/retold-engine/src/canon.rs` edits (two long schema-serialization lines). The source
edits were preserved. Later full-gate stages did not run, so this checkpoint does not claim a
green production full gate. The standalone native gate is separate from that existing failure.

### Remaining G0 acceptance

- Real VoiceOver workflow and announcements; keyboard focus order at every breakpoint.
- Real Hindi/Urdu IME composition, selection, mixed bidi caret movement, grapheme deletion,
  multiline wrapping, and reading-direction/alignment. The pinned input engine's multiline
  alignment path needs investigation; rendering readable Urdu does not close this gate.
- Inline moving-video/seek/teardown evidence and a measured shared-clock A/V sync decision.
  Only existing audio and still-image decode/lifecycle have been exercised automatically here.
- Measured input latency under native background/media load, sustained memory/texture behavior,
  actual idle CPU, long-duration playback, and malformed/oversized media stress.
- Native file-dialog/clipboard/drag-drop/OS lifecycle operation and font fallback packaging.
- Linux/Windows/platform-specific dependencies and backend portability; signed packaging.

## Next implementation slice

The isolated workspace now includes a real native workspace shell and shared queue/run
contracts; see [the continuation checkpoint](g1-checkpoint.md). Toolkit dependency policy was
resolved through user-approved pinned exceptions. The native input, accessibility and media
gaps above remain open, and the package has not been promoted into the production workspace.
No historical recap jobs were resumed. Production export remains disconnected from this preview.
