# Phase 4A: full-screen app foundation and Home

Date: 2026-10-19. Status: implemented; local verification recorded below.

The first Phase 4 slice. `retold-next` without a command, on an interactive terminal, opens
a full-screen app. Its first screen is Home (plan §6.1), and it is read only. The foundation is
built so later screens inherit three properties:
- every value comes from an application projection;
- every screen shows the command that does the same thing;
- the terminal is always restored.

## Try it

```sh
retold-next                    # interactive terminal: the full-screen app
retold-next home               # the same Home as plain lines (linear, accessible path)
retold-next --json home        # the same Home as one JSON envelope
retold-next | cat              # piped: usage, never the app
```

## Behavior

- **When the app opens.** Only for a bare invocation whose stdin and stdout are both
  terminals, without `--json`, `--plain` or `--quiet`. Pipes, scripts and machine output
  keep line output; a bare non-terminal invocation still prints usage.
  - `--library PATH` selects the library for the app too.
- **One snapshot, three renderings.** `retold-app::home` composes existing read-only
  projections, each with the command that produces it:
  - `video list --limit 8` (recent videos, newest first);
  - `providers health --root CONFIG`;
  - `spend --today` and `spend --month`;
  - the Home actions, each with its command and whether this phase offers it.

  A failing section carries its own diagnostic and the others still show. `home`, its plain
  rendering and the app read this same data. Shared formatting in `retold-cli/src/home.rs`
  produces the header lines for both plain output and the app, so they cannot disagree.
- **Header.**
  - Provider badges: `✓` ok, `⏳ 42m` until a recorded cooldown ends, `✗ logged out`,
    `✗ failing ×n`, and `? not checked`.
  - Today's charged money, plus held money when there is some.
  - This month's money against the account month limit, or a note that without one paid work
    is blocked.
  - How many videos the library holds.
- **Home list.**
  - Groups: CONTINUE (recent videos), NEW, LIBRARY and SETUP.
  - Actions not offered in this phase (New video, New series, Doctor) are marked
    "not available yet". Opening one runs nothing and names `RG-NOT-AVAILABLE`.
  - With no videos, CONTINUE says so.
- **Opening an item** runs only that item's read-only query and shows its plain output
  exactly as the CLI prints it, in a scrollable panel:
  - a video runs `video show REF`;
  - Videos, Spend, Providers and Account run `video list`, `spend`,
    `providers list --root …` and `account show`.
- **Footer.** It shows the keys for the current screen, and on the bottom line the command
  that does the same as the selection (`$ retold-next video show drishyam/2`).
- **Keys.**

  | Key | Action |
  |---|---|
  | `↑↓` / `k j` | Move, or scroll a detail panel |
  | `g` `G` | First or last item |
  | `⏎` | Open the selected item |
  | `esc` | Back to Home |
  | `space` / `PgUp` `PgDn` | Page through a detail panel |
  | `r` | Refresh Home; the selection stays on the same command |
  | `?` | Help; any key closes it |
  | `q` | Quit from Home |
  | `ctrl-c` | Quit from anywhere |
- **Terminal safety.** `ratatui::init` installs a panic hook that restores the terminal.
  Normal exits, including Ctrl-C (a key event in raw mode), restore the alternate screen,
  cursor and echo. Drawing errors exit with code 4 after restoring.
- **Readability without colour.** Selection uses reverse video and headings use bold. The app
  never relies on colour alone.

## Design

- `tui/app.rs` is the state machine. `press(key)` updates state and records an `Effect`
  (`None`, `Quit`, `Refresh`, `Open(entry)`), which the loop reads with `effect()`. It has no
  terminal access.
- `tui/view.rs` turns state into panels. `view::frame(app)` is the closure `terminal.draw`
  renders, and the tests use the same closure.
- `tui/mod.rs` owns the terminal and runs the read-only queries.
- The app depends on `ratatui` 0.30 (MIT; crossterm backend only). It passes `cargo deny` and
  the architecture check; `retold-app` stays free of presentation crates.

## Deliberate limits

- Read only. Continue, export, stop and every other action stay in the CLI.
- The video screen is the plain `video show` output, not yet the stage/blocker/plan layout
  of §6.2. Live updates, the log pane, providers/spend screens, the command palette, themes
  and mouse support are later Phase 4 slices.
- CONTINUE lists recent videos; it cannot yet tell finished from unfinished.
- No automated PTY suite yet. One manual pseudo-terminal run is recorded below.

## Verification

- App unit tests (`retold-cli/src/tui/tests.rs`, in-memory terminal):
  - Home shows provider badges with a cooldown countdown, spend against the month limit, the
    library count, groups, and the selected command in the footer;
  - the app and plain `home` show the same provider and spend lines;
  - moving updates the footer; unavailable actions run nothing and say why; Spend opens;
  - detail panels show command output, scroll, and return Home; `q` then quits;
  - help closes on any key before anything else; Ctrl-C always quits;
  - refresh keeps the selection by command; an empty library says so.
- CLI (`retold-cli/tests/home.rs`, isolated home, config and library):
  - JSON sections report their own failures with their commands, and list unavailable actions;
  - plain `home` is the linear path; a bare piped invocation prints usage;
  - nothing is written.
- Manual pseudo-terminal run (`script` with a 100×30 terminal). It entered the alternate
  screen, drew Home, moved to Spend, opened the spend report, returned with Esc and quit with
  `q`, then left the alternate screen and showed the cursor again.
- `./scripts/check.sh` passes in full, with the Clean Code gate at its existing baseline;
  `cargo test --workspace`: 595 tests passed, 0 failed.

No provider requests, account changes or video exports were made.
