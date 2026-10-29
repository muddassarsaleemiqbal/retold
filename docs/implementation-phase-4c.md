# Phase 4C: dedicated providers and spend screens, log pane toggle, and PTY test suite

Date: 2026-10-19. Status: implemented and verified across full test suite and quality gates.

Building on the Phase 4A Home foundation and Phase 4B Video screen, Phase 4C delivers the dedicated
Providers and Spend screens, video log pane toggle, and an automated PTY integration test suite.

## Capabilities & Visual Layout

### 1. Dedicated Providers & Routing Screen (`plan.md` §6.5)
Opened directly from Home (selecting `Providers` and pressing Enter):
- **Configured Providers Panel (Top Left)**:
  - Scrollable list of provider cards showing model ID, provider binary name, configured environment variables, and protocol status.
  - Live health status badge with remaining cooldown countdowns (`⏳ 42m`, `✓ ok`, `✗ logged out`, `✗ failing ×n`).
  - Selection cursor (`▸`) movable with `j`/`k` or arrow keys.
- **Routing Chains Panel (Top Right)**:
  - Active model tier routes (e.g. `smart`, `fast`, `cheap`) formatted with arrow steps (`codex:smart -> claude:smart`).
  - Explicit empty state when no routing chains are configured.
- **Provider Details Panel (Bottom Left)**:
  - Deep inspection of selected provider card: binary location, account ID, health reason / error diagnostics, and cooldown expiration timestamp.
- **Action Notices**:
  - `t` displays an informational notice with the exact CLI test command: `retold-next providers test <id> --dry-run` (explaining explicit authority requirements).
  - `l` displays an informational notice with the login command: `retold-next providers login <id>`.
  - `r` refreshes provider health from the ledger.
  - `Esc` / `q` returns cleanly to the Home screen.

### 2. Dedicated Spend & Cap Screen (`plan.md` §6, Phase 6 TUI Dependency)
Opened directly from Home (selecting `Spend` and pressing Enter):
- **Spend Summary Panel (Top)**:
  - Current aggregation window (`Today`, `This Month`, `Lifetime`) and breakdown grouping (`By Provider`, `By Video`, `By Series`, `By Day`).
  - Total charged and held USD currency amounts (`Total Charged: $X.XX · Total Held: $Y.YY`).
  - Account ceiling limit and available headroom calculation, or an explicit warning when no ceiling is configured (`Paid work is blocked`).
- **Usage Breakdown Table (Bottom)**:
  - Formatted multi-column table displaying `KEY`, `ATTEMPTS`, `HELD`, `CHARGED`, and `HEADROOM`.
  - Formats every row directly from the application's transactional ledger projection.
- **Interactive Toggles**:
  - `w` cycles the time window: `Today` → `This Month` → `Lifetime` → `Today`.
  - `b` cycles the breakdown grouping: `By Provider` → `By Video` → `By Series` → `By Day` → `By Provider`.
  - `Esc` / `q` returns cleanly to the Home screen.

### 3. Video Screen Log Pane Toggle (`plan.md` §6.2)
- Added log tailing projection `library::video_events` in `retold-app` to read up to 30 trailing event lines from `events.jsonl` under the video run root.
- Pressing `l` on the Video screen toggles between the execution plan panel and the events log pane.
- Sets a temporary informational status footer: `Log pane Events Log. Press l to toggle.` or `Log pane Stage List. Press l to toggle.`.

### 4. Automated PTY Integration Test Suite (`crates/retold-cli/tests/tui_pty.rs`)
- End-to-end pseudo-terminal execution driving the compiled `retold-next` binary in real PTY sessions via Python's standard `pty` and `termios` subsystem.
- Tests verify:
  1. `pty_lifecycle_terminal_restoration_and_read_only`: validates entering alternate screen (`\x1b[?1049h`), exiting alternate screen (`\x1b[?1049l`), restoring cursor (`\x1b[?25h`), exit status 0, and asserts zero file writes or mutations.
  2. `pty_navigation_spend_providers_and_actions`: exercises Home → Spend (cycling window `w` and grouping `b`) → return → Providers (testing `t` and `l` banners) → return → quit.
  3. `pty_video_screen_and_log_toggle`: initializes an isolated sandbox with a saved video run, enters the video screen, toggles the log pane with `l`, opens costs with `$`, returns to Home and exits cleanly, verifying byte-for-byte read-only preservation across all video files and `.retold/lock`.

## Architectural Cleanliness & Quality Gates

- `ratatui` and `crossterm` remain strictly isolated in `crates/retold-cli`.
- `retold-app` remains pure business logic without any presentation dependencies (verified by `scripts/check-architecture.py`).
- Clean Code rules (`clean-code.toml` at `strictest` preset):
  - Handled single-responsibility functions (< 30 lines).
  - No flag arguments (boolean arguments avoided in favour of enums like `VideoPane`).
  - Command-query separation strictly preserved on state machine sessions (`should_quit(&self) -> bool` vs `apply_effect(&mut self)`).
  - No output arguments: functions return transformed values rather than mutating arguments.
- Quality gates:
  - `./scripts/check.sh --fast`: 100% passing (0 warnings, 0 clean-code findings).
  - `./scripts/check.sh`: 100% passing in full (formatting, clippy pedantic/nursery, clean code gate, acceptance clippy, architecture check, machete, cargo deny, rustdoc, full workspace tests: 601 passed).
