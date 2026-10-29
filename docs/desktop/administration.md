# Native administration

**Help → Workspace health** runs the shared workspace doctor read-only and lists every check,
disk usage and orphaned files. When checks are fixable, **Apply listed repairs** runs the doctor's
repair mode only if a fresh inspection still lists exactly the reviewed fixable checks; otherwise
it asks for a new review. **Copy shareable diagnostics** places a text report on the clipboard with
the workspace and home paths replaced by `<workspace>` and `~`; the report contains no credentials.

**Settings** lists export presets (with a two-step remove) and configuration profiles (select the
active one), and can create both: a new preset is assembled from the shared validator's allowed
aspects, containers, codecs and caption modes; a new profile is created empty and not selected. `retold_app::desktop::admin` is the typed projection; `tests/desktop_admin.rs`
verifies that health inspection writes nothing, redaction hides the workspace path, and unknown
presets or profiles are rejected.

Still CLI-only: account initialization/limits/reconciliation, explicit provider login and test,
guide/channel authoring, library copy/archive/delete, backup, restore and
activation.

## Video lifecycle

A video's overview has **Archive / Restore from archive**, **Copy as new video** and a two-step
**Move to trash**. Each change acquires account activity and first proves the video's run lock is
free, so it refuses while generation or repair owns the video. Trash moves the folder into the
workspace's `.trash`; permanent deletion remains an explicit CLI action. `tests/desktop_lifecycle.rs`
covers the full archive → restore → copy → trash sequence and refusal while the run lock is held.

## Account limits and provider tests

**Budget → Change the account limit** edits the account-wide daily, monthly or lifetime USD and
paid-request caps. Review shows the shared dry run (before/after, headroom, exhausted units); the
change applies only if a fresh dry run still equals the reviewed one, so concurrent usage or
another edit forces a new review. Limits never erase recorded spend.

**Providers → Test providers** reviews the shared provider-test plan (one free probe per configured
provider, no spend) and runs exactly that plan hash. `tests/desktop_account_admin.rs` re-runs
itself with an isolated `HOME`, initializes a fresh account, and proves a reviewed limit applies
once while repeated, stale and forged reviews are refused. Provider probes are not exercised by an
automated native test because they contact real tools.

## Backups and restore

**Settings → Show presets, profiles and backups** contains **Back up this workspace…** (choose a
parent folder; a new `retold-backup-<time>` folder is previewed, then created only if a fresh
preview still matches), **Verify a backup…** (offline index and checksum verification) and
**Restore a backup…** (choose the backup, then a destination; preview, then restore as an
*inactive* workspace). **Activate a restored workspace…** shows the shared read-only activation plan with a prominent
warning (it transfers ownership and retires the original root) and activates only under that
exact plan hash. Cross-installation reconciliation with prior-stop attestation stays CLI-only. `tests/desktop_backups.rs` runs with an
isolated `HOME`, registers a workspace in a fresh account, and proves preview writes nothing, the
reviewed backup is created once and verifies without authorizing execution, and a forged review
creates nothing.

## First-launch account and provider login

When no account report exists, **Budget** offers **Set up the installation account**: review the
shared dry run, then create the account once in the system time zone. **Providers → Provider
login** lists configured providers; choosing one shows its offline login state (credentials are
never read), and a key environment-variable name can be previewed and saved. **Budget → Reconcile a held attempt** takes an attempt ID and actual cost; the shared
`onboarding::reconcile` contract records a provider's real cost for a held attempt only while its
preview is unchanged. `tests/desktop_onboarding.rs` (isolated `HOME`) proves the preview creates
nothing, creation happens once, and an unknown attempt cannot be reconciled.

## Guides and channels

Inside **Show presets, profiles and backups**, **Guides and channels** creates a guide or channel
in one language (review the shared dry run first), or opens an existing one to show its hash and
missing required fields. A single text field, addressed by JSON pointer such as `/name`, is
previewed and saved through the shared structured editor, which validates the whole document and
refuses the save if the file changed since it was opened. `tests/desktop_authoring.rs` covers
preview-before-create, the hash-bound edit and refusal of a stale second edit.

## Saved views

Below the video list, **Saved views…** lists saved catalog views; choosing one loads the first page
with its search, sort, tag, channel and archive filter. **Save current filter** stores the
current catalog options under a name through the shared view registry (names are validated).
`tests/desktop_admin.rs` round-trips a filter and rejects an invalid name.

## Selecting several videos

Each video row has **Select**. With videos selected, a bar below the list offers **Archive
selected**, **Restore selected** and **Add tag to selected**; each video is changed in turn through
the same guarded lifecycle contract, and the run stops at the first video that is being generated,
reporting how many were changed.

## Readiness and the example guide

Home ends with **Ready to run?**: **Check readiness** lists tools (ffmpeg, ffprobe, editor),
guides, channels, provider routes and the installation account, each marked ready or needing
attention with a path forward; the checks are offline and read-only. **Preview example guide**
shows what copying the bundled story-recap guide would write, and **Copy example guide into this
workspace** writes it into user-owned resources only after that preview; copying twice is refused.
`tests/desktop_authoring.rs` covers preview-before-copy, single copy and the readiness checklist.

## Every operation as a form

Every page ends with **More actions**, listing the operations that belong there (video actions on a
video's overview with that video filled in, account actions on Budget, provider actions on
Providers, series actions on Series with the open series filled in, configuration and library
actions in Settings, diagnostics and creative tools in Help). Choosing one opens a form generated
from the operation's own definition (`retold_cli::forms`):

- on/off options are toggles, fixed sets of values are choice buttons, files and folders have a
  **Choose…** picker, whole numbers are validated, and repeatable options take comma-separated
  values; every field shows its label, help and a required marker;
- read-only actions run at once; actions with a dry run go **Preview → Apply previewed change**;
  other changes show a plain summary of the entered values and run only after **Confirm** (the
  form's confirmation replaces the command line's `--yes`);
- fields that name something that exists offer a pick-list: videos, series, queue jobs, providers,
  guides, channels, presets, views, profiles, materials and music/effects; actions that create
  something new never suggest existing names;
- a reading action becomes a reviewed change when a changing switch (reconcile, fix, remove,
  force) is turned on;
- results are rendered as text, labelled values and tables, and failures show the message and the
  suggested next step.

Forms run through the same dispatcher as the command line, so behaviour cannot drift. The
operation coverage table maps every one of the 144 operations to its page; dedicated pages
(Scenes, Script, Narration, Budget limits, backups and others) remain for the common journeys.
