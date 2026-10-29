# Using the terminal interface

Run `retold` in an interactive terminal. Use at least 80 columns and 24 rows.
At 100 columns the sidebar expands; at 140 columns inspectors appear beside the main pane.
Use `retold --plain <command>` for text output without terminal interaction, or `--json`
for machine output. `retold --help` lists available commands.

## Navigation and forms

- `1`–`9`, `0`: select a section; `Tab` / `Shift-Tab`: change landing tabs.
- `:` / `Ctrl-K`: search commands; `?`: help; `Esc`: close the current modal or page.
- `/`: filter a collection; arrows select; `Enter` opens; `.` shows the selected item's actions.
- `F2`: guided setup; `F3`: choose a field value; `F6`: browse/edit a path with the configured editor.
- Forms: arrows/Tab move between fields, Space switches a flag, left/right choose an option.
  `Ctrl-S` advances a wizard or submits its final step; `F7` returns to the previous step.
- Text: `Ctrl-A`/`Ctrl-E` move to the beginning/end; `Ctrl-U` deletes text before the cursor;
  `Ctrl-W` deletes the previous word. These keys do not select all text.
- Paste goes into the active text field or filter. Line breaks and tabs become spaces;
  pasted controls never navigate or submit. A paste exceeding the field's 64 KiB limit is
  refused with a notice and leaves the draft unchanged. Paste never confirms an action.
- `Ctrl-R`: refresh; `y`: copy the displayed CLI command to the host clipboard; `T`: cycle themes.
- `q` backs out of a result page, then quits from a landing. `Ctrl-C` requests quit globally;
  when an action is running, the interface asks before leaving and can wait until idle.

The footer shows the CLI equivalent. Writes require review. Cancelling returns to the filled
form. Production actions start with a zero-dollar allowance and use the shared plan gate.
A completed configuration action does not establish provider authentication or readiness.

The workspace switch action in the palette asks before clearing unsaved TUI drafts and
selections. Running actions must finish before switching. A successful switch returns to Home
and reloads the new workspace's saved choices, pages and provider data. Cancelling or a failed
switch preserves the current drafts and selections.
An open session keeps its selected configuration and media paths when another CLI changes
the remembered default. Use the switch action to move the open session. A changed workspace
layout requires an explicit selection again; shared account administration keeps its global scope.

## Money and configuration

In Money, `w` changes the spend window and `b` changes grouping. The chart shows charged plus
held money; the table keeps those amounts separate. Limits opens saved scope/window rows;
`Enter` edits the selected ceiling. Attempts opens the exact recorded attempt for reconciliation;
`>` loads its next cursor page and `<` returns to the previous page.

Money → Processes lists recorded owners and their status. Select a row to inspect saved
supervisor/worker identity and live evidence. `/` filters; `>` and `<` page independently of
Attempts. Inspection cannot stop a process or clear unresolved evidence.
Money → Backups explains account recovery scope: `b` chooses a new output folder and reviews
contents/active owners before creating it; `v` verifies a chosen saved folder offline.
Account backups exclude credentials and workspace media. System → Storage backs up workspaces.

Limit changes and reconciliation load a read-only preview before applying. A failed, loading or refreshing
preview cannot dispatch. Applying revalidates current account state. Cancelling preserves the
accounting files. Reconciliation records billing evidence and grants no authority to retry work.

Configure → Guides/Channels → `Enter` opens structured fields, with the name first. Select a field
and press `Enter` to edit its existing type, then review the validated before/after change.
A stale document hash rejects application; refresh before trying again. List fields (including
empty lists) open as one editable list. Enter a JSON array such as `["youtube","tiktok"]`;
element types and schema are validated before saving. Individual items remain selectable:
`Insert` appends after review, `Delete` removes the selected item and `J`/`K` move it down or up;
each produces a reviewed whole-list change.
Schema-declared omitted fields with existing parent sections appear as `(not set)`.
Their editor starts with a suggested value; nothing is added until you review and apply it.
A missing object section is offered with its required members filled so it validates.
Identity/schema are excluded from editing. Guide sidecars are preserved. Scaffold/import/check and editor handoff
remain available through the action menu and palette.

To create a guide from the bundled example, press `n` on Guides, enter its new identifier and
choose `story-recap` in Example with left/right or `F3`. Leave scaffold fields and From blank.
The explicit copy includes all writing sidecars; no provider is invoked. Choose a language and
leave Example/From blank for a neutral scaffold. Channels keep explicit voice/font settings;
validation reports any remaining CONFIGURE placeholders.

Successful draft creation remembers language, quality and budget for each guide/channel pair
in the workspace configuration. Spend allowances and execution authority are never remembered.
Saved choices load in the background and fill untouched draft fields. Fields you edit or clear
keep your values. A failed read produces a notice and leaves manual entry available.
The draft review lists each provider tier the guide's roles use and the fallback chain that
serves it; tiers with no route are flagged so they can be fixed before planning.

## Videos, review, queue, and series

- Videos: Space selects rows; `A` reviews archive for the selected set; `D` reviews moving them
  to trash; `t` reviews adding a tag. Esc preserves selection. Each dispatched action keeps its own result.
- Video workspace: Tab changes views; `s`/`h` open line/shot edit forms, `b` browses files.
- Review: arrows select a saved artifact; `a` approves, `u` unlocks, `n` notes, `r` opens redo
  for its stage and item (shown as a dry-run of what rebuilds before applying), `e` edits its shot,
  `o` opens its saved file, Space plays audio. `f` cycles all/pending/stale/immutable/failed filters.
  `L` cycles the saved edition languages and then returns to all; the header shows the selection.
  Shared files appear when the language filter is all. Empty filters clear the previous preview.
  `v` pins or clears a comparison baseline; selection then compares saved hashes and sizes, with saved images shown together
  when previews are available. It does not generate another take.
- Studio: select a saved queue row; `p` pauses, `r` unpauses, `x` stops, `J`/`K` review a move
  between queued neighbours. Running jobs cannot be reordered by these controls.
- Series: Enter opens its saved parts matrix: a completion gauge, then each part's stages
  marked cached (✓), needing repair (!) or not run (·). Arrows select, Enter opens an existing
  video, `n` creates a draft, `l` links an existing video, `p` plans that part, `r` opens its run
  form and `a` reviews adding parts. A reviewed run submits real studio jobs. The total allowance
  is divided across the selected parts. Completion reflects saved ready evidence and clears when
  that evidence is unavailable or invalid.
- Library Music/SFX: Space plays the selected saved file. Unsafe relative filenames are rejected.
  Music detail shows which score moods (calm, tense, dark, sad, warm, mysterious, epic, hopeful)
  have a track tagged with them; untagged moods play unmatched music. WAV imports record their
  measured duration; other formats show the duration as unknown.
- System Health/Storage: inspect reports first; `f` opens repair review, `b` backup, `v` verification.

## Terminal integration and qualification

Mouse clicks select sidebar/tabs/collection rows. Clicking an already selected row opens it.
The wheel follows page/modal navigation; keyboard controls remain the complete navigation path.
Confirmation buttons support mouse review/cancel using the keyboard dispatch gate.
A test clicks every field of every generated form and palette rows; clicks focus fields and
never dispatch work.

Images use a portable raster preview with a no-colour text fallback. Host programs:

| Integration | macOS | Linux |
|---|---|---|
| Audio preview (Space) | `afplay` | `ffplay -nodisp -autoexit` |
| Open a file (`o`) | `open` | `xdg-open` |
| Copy the shown command (`y`) | `pbcopy` | `wl-copy`, then `xclip`, then `xsel` |
| Edit a file (F6) | `$VISUAL`, then `$EDITOR`, then `vi` | same |

Copying runs in the background and reports success or failure. A stalled clipboard program
times out after three seconds; navigation stays responsive and quit can wait for it to finish.
Additional copy requests are ignored until the current copy finishes.

In the file browser, `o` and Space act only while the filter is empty; once you type a filter,
letters and spaces go into it. After the editor exits the screen is repainted without asking the
terminal for its cursor position, so slow multiplexers and SSH sessions no longer fail the edit.
PTY tests put recording stand-ins for these programs on `PATH` and check what the TUI passes them;
no sound plays and the real clipboard is untouched. They prove the TUI's side on the test host.
On macOS, automated real-host smoke checks passed in Terminal.app 2.15 at 80×24, 120×40
and 200×60, including Ctrl-C restoring the original terminal mode. GNU Screen 4.00.03 passed
launch, Videos navigation and exit. The real `afplay` exited successfully for a 100 ms silent
WAV; this is a process smoke check, not a listening-quality assessment. Linux desktop programs and other terminal emulators remain
unqualified on this host; recording stand-ins do not establish their behaviour.

Automated checks use isolated HOME/config fixtures, reducer tests, terminal snapshots at 80×24,
120×40 and 200×60, and real PTYs. The performance regression uses 1,000 saved video rows,
a 150 ms first-frame ceiling and a 16 ms median key-to-frame ceiling over 30 samples on the host.
These are test-host measurements, not guarantees on every machine. No live provider requests or
production video exports are needed for the automated TUI checks. See [the full plan](plan-tui.md)
for remaining implementation and acceptance work.

## Platform verification (2026-10-23)

Colour themes paint their own canvas and modal backgrounds, so Dark and Light remain
readable when a terminal starts with a different background. No colour keeps the terminal's
own colours. The user accepted captured readability at 80×24, 120×40 and 200×60.

FilmCraft export uses the installed app executable on macOS, `filmcraft` on PATH on Linux,
and `filmcraft.exe` on PATH on Windows. An explicit engine session executable still takes
precedence. Doctor checks the same default. macOS and ARM64 Linux production fixtures
exercise real FilmCraft project/video export; this does not qualify Windows. Linux acceptance
uses FilmCraft 0.9.0, Xvfb and software graphics in an isolated container.

When shutdown is requested during editor handoff, Retold waits for the editor to exit.
Save or recover unsaved work in that editor before closing it. Retold does not force-kill
a stalled editor; on return it restores the terminal and finishes the pending shutdown.

See [the reliability audit](reliability-audit.md) for logs, container scope, and remaining
production-provider, audible-quality and historical intermittent-failure limitations.
