# Phase 4D: TUI Polish & Universal Controls (Command Palette, Theme Engine, Mouse Interaction)

Date: 2026-10-19. Status: implemented and verified across full test suite and quality gates.

Closing out Phase 4 interactive capabilities (`docs/plan-cli.md` §6.7), Phase 4D delivers universal controls across all application screens:
1. Modal Command Palette with incremental substring filtering and action dispatch.
2. Complete Theme Engine (`Default`, `High Contrast`, `No Color`) with runtime toggle key and CLI/environment color suppression honoring.
3. Crossterm mouse capture for clicking menu entries, scrolling detail panels and tables, and selecting provider cards.

---

## Capabilities & Architecture

### 1. Command Palette (`:` or `Ctrl-K`) (`plan.md` §6.7)
- **Modal Overlay**:
  - Activated from any screen in the full-screen TUI with `:` or `Ctrl-K`.
  - Centers a distinct overlay dialog (`64x14`) over the active screen state with a search input row, separator line, and scrollable action list.
  - Pressing `Esc` dismisses the palette and restores the underlying screen and selection state without any side effects.
- **Incremental Filtering**:
  - Characters typed are dynamically added to the query; `Backspace` deletes characters; Up/Down arrow keys or mouse clicks navigate results.
  - Matches case-insensitively across command labels and action descriptions.
  - Pre-populates global screen actions (`Home`, `Spend & Cap`, `Providers & Routing`, `Videos`, `Account`, `Toggle Theme`, `Refresh`, `Help`, `Quit`) and contextual video actions (`Costs`, `Plan`, `Toggle Logs`).
- **Action Dispatch**:
  - Pressing `Enter` executes the selected item, closing the palette and transitioning to the target screen or toggling the targeted state.

### 2. Theme Engine
- **Supported Themes**:
  - `Default`: Subtle borders, cyan titles, yellow selection markers, magenta commands, green/yellow/red status badges.
  - `High Contrast`: Bright white borders, bold yellow titles, bold inverted selection highlight, high-visibility badges.
  - `No Color`: Plain ASCII/monochrome styling with bold and reversed text formatting, strictly avoiding ANSI color codes.
- **Runtime and Startup Control**:
  - Pressing `T` (or selecting "Toggle Theme" in the Command Palette) cycles runtime themes: `Default` → `High Contrast` → `No Color` → `Default`.
  - Active theme badge displayed in the footer (`[Theme: <name>]`).
  - Automatically initializes to `Theme::NoColor` if the CLI flag `--no-color` is supplied or the `NO_COLOR` environment variable is set.

### 3. Mouse Interaction
- **Terminal Event Capture**:
  - Enables crossterm mouse capture (`EnableMouseCapture`) on terminal initialization and guarantees clean release (`DisableMouseCapture`) on application exit or panic.
- **Mouse Navigation & Selection**:
  - **Scroll Events** (`ScrollUp` / `ScrollDown`): Scrolls list cursors on Home and Providers screens, and scrolls viewport offsets on detail/logs/spend tables.
  - **Left Click Events** (`MouseButton::Left`):
    - On Home: Single click selects a menu entry; clicking the currently selected entry activates it (`open()`).
    - On Providers: Clicking a provider card selects that card and updates the detail inspection view.
    - On Command Palette: Clicking any listed command immediately selects it.

---

## Architectural Constraints & Clean Code Compliance

- **Strict Layering**:
  - All presentation crates (`ratatui`, `crossterm`) remain strictly confined within `crates/retold-cli`.
  - `crates/retold-app` and `crates/retold-engine` remain completely free of presentation dependencies, validated by `scripts/check-architecture.py`.
- **Clean Code Gate**:
  - Zero new violations against `clean-code.toml` preset `strictest` (functions do one thing, no vague function names, max 4 match arms per method, no boolean flag arguments, one level of abstraction, use of explanatory variables).
- **Automated PTY Integration**:
  - Added `pty_command_palette_and_theme_toggle` in `crates/retold-cli/tests/tui_pty.rs` verifying end-to-end PTY interaction:
    1. Opening Command Palette via `:` and filtering with query strings.
    2. Theme cycling via `T` and verifying `[Theme: High Contrast]` and `[Theme: No Color]` in raw terminal output.
    3. Exiting cleanly back to normal terminal state without disk mutation or lingering processes.
