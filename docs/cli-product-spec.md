# Retold CLI product specification

Status: finalized for implementation planning after independent revision 11 approval (see `reviews/cli-plan-review.md`) · Revision 11 · 2026-10-18.
Parent: [CLI rewrite plan](plan-cli.md). This specification is part of its cutover gate.

Implementation scope update (2026-10-18): legacy command parity/inventory/adapters are no longer
required by the user. New commands and data/authority safety remain required. See
[Phase 1 implementation](implementation-phase-1.md) for delivered scope and evidence.

## 1. Product promise and scope

Retold is a local production workspace for people creating standalone videos and multilingual
series. It should feel like a well-organized editing desk: a quiet library, an inspectable plan,
an actionable production queue and a complete delivery folder. Novices can finish every workflow
through guided forms; experts and agents use the same typed application functions.

“Complete” means that each advertised action has a CLI command, a TUI route where applicable,
a versioned machine contract, error/recovery behavior and acceptance evidence. Command listings,
mock screens and fake success messages do not count as working features. Every required feature
below must pass before default-binary cutover. The retained engine must provide real capabilities;
no UI hides an unavailable engine behind a success state.

The [production stage contract](production-stage-contract.md), the safety contracts in plan §19 and the [resource efficiency and quality contract](cost-quality-contract.md)
and the [runtime and editing contract](runtime-editing-contract.md) remain normative. Subscription/local resource ceilings apply even when USD spend is zero. This specification refines product
behavior and storage; it cannot weaken authority, accounting, locking or migration. Implementing
this plan is separate from running real paid calls or producing videos.

Fresh installs have no bundled production guide, channel, voice identity or genre assumptions.
Examples use test fixtures. Guide-defined inputs, folder rules and creative rules remain user-owned.
The workspace can support recaps, explainers, documentaries and other user-configured workflows.

## 2. Complete journeys

### J1. First workspace to ready-to-export

1. Run `retold workspace init <directory>` or choose Create workspace. Show exact config,
   library, state and cache paths, writable-space checks and existing-directory conflicts.
2. Choose/create a guide and channel. Scaffold unfinished fields as `CONFIGURE`; Check shows
   each missing input with its editor action. No sample identity is silently installed.
3. Scan local providers, select routes, validate capabilities offline. Explicit Test is separate.
4. New video collects only missing guide inputs; preserve form answers on validation errors.
   Duration, languages, caption scripts, aspect targets and budget are explained with examples.
5. Preflight shows missing prerequisites, disk estimate, routes and unsupported capabilities.
   Plan shows reused/generated/blocked work, uncertain estimates and maximum authorized spend.
6. Start preparation explicitly; allow detach only after studio admission is acknowledged.
   At the script/delivery-cue checkpoint, include material event evidence/uncertainties and explain
   what needs human review, what voice work is
   waiting and which independent branches may continue. Review and approve exact versions. Before images, approve coherent storyboard participants/
   events and used visual states; a script mentioning an investigator never makes them a witness.
7. Show a renewed plan and the separate spending and usage-policy decision: default USD allowance is zero; a paid-only
   voice route remains blocked until a positive bounded allowance is explicitly granted. If no
   suitable route exists, offer provider setup or a saved draft rather than promising free output.
8. Continue under fresh/valid job authority to `ready`. No project/video exists yet. Offer Export
   separately with explicit edition/format selection and export authority.

Proof: clean temporary HOME/workspace with fake providers completes this journey without editing
TOML and without creating a FilmCraft project/video. Include a paid-only voice fixture: default
allowance makes zero paid calls, review approval alone cannot dispatch it, and a separately
confirmed bounded allowance can. A free-route fixture reaches ready for $0; arbitrary real
installations are never promised a free sample.

### J2. Failed production to safe continuation

Home highlights the actual blocker, affected items, work still running and a concrete fix.
Open its error details → inspect sanitized log/evidence → edit or retry the affected item → view
the changed dependency plan → approve its allowance → continue within the existing owner.
Already committed and approved assets remain intact. Unknown charges block automatic retry.
Proof: fail one language while images continue; repair it, verify unchanged image request IDs.

### J3. Series, shared assets and multilingual editions

Create an ordered series plan, review shared cast/places/props/style and pin versions before
admitting parts. Queue selected parts with one aggregate allowance and per-part ceilings.
Series view shows a part-by-language matrix with images, voice, captions, review and export
readiness separately. A series run prepares through `ready`; it never implicitly exports.
Add a language with a dry-run reuse report; inspect original text, translation, pronunciation
notes and caption script together. Any paid voice starts only from the approved current script.
Proof: two parts and two languages share verified assets once and keep independent output states.

### J4. Review, compare and deliver

Filter review items by pending/stale/failed and language; inspect two saved takes side by side,
listen to a selected line, approve the chosen immutable version. Notes remain attached to their
version. Review/export readiness reports missing media, outdated approvals and licence evidence.
Explicitly export selected language/aspect outputs, inspect delivery QA, then package committed
outputs into a verified folder with checksums, credits and metadata. Packaging never renders.
Proof: a missing export causes Package to fail with an explicit export command, not render it.

### J5. Move, back up and recover

Inspect workspace storage → preview a portable backup excluding secrets → create it → verify
checksums → restore as an inactive workspace → inspect offline → explicitly activate after
ownership/accounting reconciliation. No restored queue resumes automatically. Lost library
indexes can be rebuilt; lost authoritative accounting cannot be reconstructed by guessing.
Proof: corrupt/truncated archive, path traversal, running owner and stale accounting snapshots
fail safely; verified restore preserves manifests, approvals, provenance and unresolved charges.

## 3. Feature coverage and delivery owners

`P` is a CLI phase in parent §17. `C` is a companion series/quality phase. Every row is required.
Additional interface dependencies do not allow advertising an action before its use case works.

| ID | Feature and observable behavior | Owner / phase | Required proof |
|---|---|---|---|
| F01 | Workspace init/show/check/switch; explicit roots and remembered UI selection | app paths/config, P3; TUI P4 | Two isolated roots, precedence and write-conflict fixtures |
| F02 | Guided setup, input drafts, zero-call readiness, named user templates | app authoring, P3/P8/P10 | Incomplete scaffold fails usefully; non-recap fixture completes |
| F03 | Searchable library, filters, saved views, tags, archive and recent activity | app index, P8 | Stable paginated search over 1,000 runs; rebuild from manifests |
| F04 | Video inspector: stages, editions, current pins, costs, blockers and next command | app projections, P2/P4 | Same snapshot through CLI, TUI and JSON |
| F05 | Explainable plan diff, cited event/shot constraints and dependency inspection; reason per changed artifact | engine/app, P2 + C2 | Music-only edit does not regenerate research/script |
| F06 | Production queue: reorder pending jobs, pause/drain, stop, attach and schedule | studio, P5 | Two clients, crash/restart and fair provider admission |
| F07 | Series dashboard and selection by part/language, shared versions | engine/app, P9 + C5 | Shared publication race suite from parent §19.3.1 |
| F08 | Edition editor for translation, caption script, pronunciation and voice | engine/app, P7/P9 + C8 | Editing one edition preserves others and shared images |
| F09 | Asset browser with provenance, usages, integrity and immutable takes | app/artifact store, P7/P8 | Missing/corrupt asset blocks dependents; no silent replacement |
| F10 | Saved-take comparison and selection; notes and review filters | app review, P7 + C3 | Selecting a take changes only pinned version and true descendants |
| F11 | Review coverage, locks, stale acknowledgements and undo history | engine/app, P7 + C3 | Approval-hash and stale-state parity across all interfaces |
| F12 | Provider scan/setup/routes/capabilities and rate-limit health | providers/app, P3 | No calls on scan/add; limited profile receives zero dispatches |
| F13 | Unified money/subscription/local-resource ceilings, holds, waste and quality-adjusted usage reports | app accounting, P2/P6 | Concurrent callers and period rollover cannot escape caps |
| F14 | User music/SFX library with licence/source, preview and cue coverage | engine/app, P8 + C4 | Missing required audio blocks export; credits propagated |
| F15 | Safe resource controls: concurrency, disk headroom and queue admission | engine/studio, P5/P10 | Low disk stops new writes safely; reduced concurrency drains |
| F16 | Explicit export profiles and per-edition delivery readiness/QA | engine/app, P2/P9 + C7/C9 | Requested outputs only; no generation from package/check |
| F17 | Structured folder browser, output manifest and deterministic delivery pack | layout/app, P8/P9 | Relocation/checksum/collision/partial-publication fixtures |
| F18 | Portable backup, offline restore, integrity verify and activation | app storage/config, P3/P8 | Interrupted restore and stale financial-state refusal |
| F19 | Storage usage, dry-run clean and recoverable deletion | app storage, P8/P10 | Keep approved/source/financial artifacts; verify Trash recovery |
| F20 | Offline diagnostics, redacted support bundle and planned repairs | app diagnostics, P10 | Secret canaries absent; no implicit login/download/provider calls |
| F21 | Discoverable help, completions, schema and machine pagination | CLI/app schema, P1/P2/P10 | Closed-stdin tests and parser-error NDJSON fixtures |
| F22 | Terminal design, accessibility, navigation/focus and Unicode handling | CLI/TUI, P4/P7/P10 | Resize, no-color, key-only and Hindi/Urdu visual fixtures |
| F23 | Reproducible preferences: named settings profiles and export presets | app config, P3/P8 | Profiles reference IDs/settings only; never store authority/secrets |
| F24 | Local activity/audit timeline and plan/review history | app events/history, P1/P7/P8 | Stable sequence/replay, redaction and referenced-history retention |

No remote sync, cloud execution, plugin marketplace, self-updater, team collaboration service,
automatic publishing or new generation-provider integration is part of this rewrite. Local FLUX
remains optional companion C10. This scope is broad because the complete production workflow is
required, not because a new extension framework is needed.

## 4. Terminal design system

### Visual direction

Use an editing-desk layout: a navigable production list on the left, detailed selected work in
the center and a contextual inspector on wide terminals. Timeline/stage progress is the primary
visual anchor. Avoid walls of cached-task lines and decorative boxes around every value.

Theme roles: ink `#D8DEE9`, canvas `#202733`, muted `#A9B5C6`, focus `#88C0D0`, warning `#EBCB8B`,
error `#BF616A`. Resolve defaults against terminal light/dark background; provide a true light
palette and ANSI-16/no-color modes. Semantic labels and focus markers always accompany color.
Use the user's terminal font; no font install. Sentence case, one strongest label per region,
right-aligned money/counts, ISO timestamps with local display timezone available on inspection.
No blinking, animated spinners on completed work or automatic focus changes from background jobs.

### Responsive layouts and state design

- At 120×36 or wider: library + stage/edition view + inspector. At 80×24: list/detail split,
  inspector as a deliberate overlay. At 60×18: single pane, preserved breadcrumb and actions.
  Below 60×18 show a resize notice and `--plain` alternative; never corrupt terminal state.
- Keep title/breadcrumb, focused content, next action and key hints visible. Long refs truncate
  in the middle with full value on inspect/copy. No clipping of an error's actionable fix.
- Loading shows the operation and cancellation path; empty library offers Create video;
  empty search offers Clear filters; unavailable image protocol offers Open file; disconnected
  studio preserves last snapshot with its timestamp and an explicit reconnect action.
- Startup and expensive file scans are cancellable. Preserve selection, scroll and form drafts
  on refresh/resize. Background failures add an alert without stealing keyboard focus.
- Full-screen mode restores cursor, echo and alternate screen on normal exit, Ctrl-C and panic.
  Use horizontal ellipsis only for display, never copied commands or stored refs.

### Navigation and input

`Tab`/`Shift-Tab` traverse focus; arrows move; Enter opens/activates; Esc backs out or closes one
layer. `/` searches the current list, `:`/Ctrl-K opens commands, `?` contextual help. `q` leaves
the current screen using the dirty-form choices in §4.2; it detaches rather than cancels studio work.
Do not intercept terminal copy or shell shortcuts. Mouse is optional; every action is keyboard
reachable. Disabled actions show their prerequisite and do not appear successful.

Primary action and composite status follow §4.3; independent running work stays visible
while a blocked edition is repaired. Risky actions display the target, affected set, quoted ceiling and
exact CLI command. A spend control edits this operation's envelope only, never persistent consent.

Hindi/Urdu/CJK/combining-character filenames and captions use grapheme-aware selection/truncation.
Bidirectional prose is isolated from paths/commands; never insert invisible display controls
into copied commands. Inline text uses terminal-supported rendering with an external UTF-8
review fallback; plain mode remains readable without terminal images or Unicode decorations.

### Screens required for design acceptance

Home/library; video overview; plan/diff; queue; series matrix; assets/take comparison; script and
edition review; providers/routes; spend/holds; channel/guide forms; audio library; storage/backups;
diagnostics/help. Each has loaded, empty, loading, invalid-input, blocked and recoverable-error
fixtures where applicable. Capture snapshots at 120×36, 80×24 and 60×18 in dark/light/no-color.
Conduct actual PTY navigation checks, not just string snapshots. No generic “Unknown error”.

### 4.1 Product cognition and navigation hierarchy

This is a creator workbench being rebuilt, not a shell dashboard. Core users are independent
creators and production operators; their first decision is Create new work or Inspect existing
work. Success means reaching the next production milestone knowingly, with recoverable edits
and a verified delivery release. Technical IDs, hashes and raw events belong in Inspect/details.
The UI foundation remains ratatui/crossterm; terminal conventions, not web/mobile widgets, govern
interaction. At narrow sizes the active task comes first, with other panes reached as destinations.

Root navigation, in stable order: **Home, Library, Queue, Spend, Setup**. Each is reachable by
Tab/arrows/Enter from the root navigation and by a labeled Back to Home action on detail pages;
no core action requires remembering a shortcut or using the command palette.

| Destination | Content and object-scoped routes |
|---|---|
| Home | Workspace identity; active/blocked work; recent videos; Create video/series; setup gaps |
| Library | Videos, Series, Templates, Saved views, Music & sounds; filters stay with their list |
| Video detail | Overview, Plan, Review, Editions, Assets, Delivery, History; Logs in Inspect |
| Series detail | Overview/part-edition matrix, Shared assets, Plan, Review, Delivery |
| Queue | Pending/running/paused jobs; selecting a row opens its job and parent video/series |
| Spend | Account totals, workspace breakdown, reservations and reconciliation; video costs link back |
| Setup | Workspaces, Providers/routes, Channels, Guides, Settings/profiles, Export presets, Storage/backups, Diagnostics/help |

Persistent header shows workspace name and a concise root hint; Inspect/copy reveals full paths.
Every mutation preview repeats workspace, object and edition/target, even after switching lists.
First launch opens Home with Create workspace/setup gaps; empty configured Home offers Create
video. Returning launch opens Home, highlights Continue working on <last object>, and preserves
its saved selection without automatically entering or starting it. Explicit object commands
open that object's detail. Workspace switch returns Home and restores that workspace's history.
Back retraces in-app navigation and restores list filter/selection/scroll; unavailable/deleted
objects return to the parent list with an explanation. Esc closes one overlay before navigating.

### 4.2 Focus, text entry, drafts and save failures

Browse shortcuts apply only when no text control/editor owns focus. In text entry `/`, `:`, `q`
and ordinary shortcut letters are literal input; Ctrl-K keeps the text editor's meaning and
does not open the palette. A visible Leave field action/Esc returns to browse scope. Tab leaves
a single-line field; multiline controls show their Tab/newline rules and an explicit Done editing
action. Text insertion, commands and shortcuts are never interpreted from pasted content.

Modal dialogs are named, take initial focus on the least destructive choice, confine focus to
visible enabled controls and return focus to their invoker. Nested overlays unwind one at a
time. Background updates never steal focus. On validation failure focus the error summary with
links to invalid fields, keep entered values and show the problem beside the field.

Leaving a dirty form by Back/Esc/q or closing its parent presents **Save draft, Discard changes,
Keep editing**. Save draft persists values and their base version only, never generation,
approval or live config; invalid values may be saved as an explicitly incomplete draft. Cancel
returns to the exact field/caret. Discard does not modify the committed object; destructive
Discard requires explicit selection. While editing, debounce-recoverable drafts into private
workspace state; force-kill recovery offers Restore draft/Discard after restart. Never imply
that unsaved in-memory keystrokes survived an abrupt kill before the last draft checkpoint.

Save first validates and previews the exact change/invalidation; confirmation commits it
atomically. A concurrent base-version change rejects the commit and offers Compare current,
Copy my edits, Save separate draft or Reload after confirming discard; never silently overwrite
or auto-merge approved text. External editor launch is explicit interactive-only; failed launch,
nonzero exit, invalid output or file disappearance leaves the committed version unchanged and
preserves any readable edited file as a recovery draft. Errors name its path and Resume editing.

PTY proof: type `/ : q` and Ctrl-K within fields, paste a command as text, escape nested dialogs,
restore focus/selection, revisit invalid drafts, reject a stale save and recover an editor error.
No failed save may discard previously committed or recoverable edited content.

### 4.3 Composite status and action selection

Represent three axes independently: run preparation milestone; active job execution state;
and per-stage/per-edition/per-release outcomes. A stopped job is not a failed video. Progress
counts completed work and active branches against the current plan; unknown totals stay unknown.
Never flatten an independent branch failure into a full-run failure while other work continues.

| Situation | Human status and primary action | Preserved context/secondary action |
|---|---|---|
| Queued | Queued, position/reason → View queue | No work claimed running; Stop waiting |
| Running, no blocker | 3 stages running → View progress | Pause/Stop with their different consequences |
| Running plus Hindi blocker | 3 stages running; Hindi voice needs attention → Fix Hindi voice | Running image count stays visible; repair is owner control, not another Continue |
| Pausing | Pausing after 2 in-flight steps → View progress | Explain no new steps start; Stop remains explicit |
| Paused | Paused at storyboard → Review resume plan | Unpause uses the same still-valid authority or requests a fresh plan if changed |
| Unresolved charge | Payment confirmation needed; $X held → Reconcile charge | Independent permitted work still visible; no automatic paid retry |
| Waiting for review | Script/cues need review → Review script and delivery cues | Show which branches can proceed and which voice is held |
| Budget/route/disk blocked | Hindi voice waiting for allowance/provider/storage → Fix named prerequisite | Existing outputs and permitted branches remain visible |
| Preparation ready | Ready to export, no video created → Choose export targets | Review assets/editions |
| Some target releases exported | Hindi portrait exported; English landscape pending → View delivery | Explicit Prepare/export missing targets after plan; never overall Finished |
| Exported, QA passed, human review missing | Exported; final viewing needed → Review exported video | Package is visibly unreviewed until confirmed |
| Selected targets human reviewed | Selected releases ready to deliver → View delivery | Exporting a new release creates its own review requirement |

If multiple blockers coexist, primary action opens a ranked attention list: uncertain billing,
required review, missing required inputs/routes/resources, recoverable generation failures.
Tie-break by earliest blocked stage then stable item ID. Show affected edition/item counts and
allow another blocker to be selected; selecting an error never stops independent work. New-job
admission/Continue is not offered over an already active owner. Safety gates remain separate:
review approves exact content, allowance authorizes bounded spend, export intent authorizes only
the selected project/render. One decision never grants the others.

### 4.4 Blocker-to-action contract

| Blocker | Allowed user-facing actions | Forbidden implication |
|---|---|---|
| Current unreviewed script/cues/take | Review content, Approve current version, Edit | Continue/--yes is not approval |
| Protected stale asset | Compare versions, Keep pinned version, Preview targeted redo | No generic Accept; acknowledgement never unlocks |
| Failed required image/audio/QA | Inspect issue, Edit named item, Preview retry/repair | No Skip or approval masquerading as passed QA |
| Missing required media or licence evidence | Import/select valid media, Add verified licence/source, Review guide requirement | No Accept/Skip to bypass required inputs |
| Unresolved paid charge | Inspect request/result, Reconcile charge with evidence | No Retry/Skip/Accept that releases an unknown hold |
| Rate limit or logged-out provider | Wait until reset, Choose allowed route, Explicit login | No silent paid fallback or auth action |
| Budget or export authority missing | Review plan, Set this operation's allowance, Explicit export selection | Confirmation never raises a cap/exports implicitly |
| Optional guide-declared asset | Omit optional <asset> with consequence preview | Never call required media optional from the UI |

An omission is a recorded artifact decision, invalidates true descendants and appears in delivery
coverage; the guide must explicitly permit it. Changing a guide requirement is a separately
validated/versioned edit with an impact plan, never a workaround button labeled Accept. Labels
name the actual object/action, not the engine's error kind. Every blocker includes what happened,
what work it affects, what remains usable and at least one honest next action.

### 4.5 Release-scoped human review and packaging

Keep statuses separate: render/export completed, automatic QA pending/passed/failed, human
review unchecked/changes requested/confirmed. Publish `current.json` only for a complete release
with required automatic QA passed; label it **Latest exported**, not Finished. QA-failed attempts
remain inspectable as failed attempt records, do not replace the last good pointer and cannot
be packaged as verified delivery. Missing/unknown QA is not passed.

Delivery → select exact release → Review exported video opens its files/report. Confirmation
requires the person to attest they watched/listened to the full selected release and checked
language/captions; a highlight or generated report alone is insufficient. Controls are **Confirm
final viewing** or **Request changes** with notes. No tool infers a human watched from playback.
CLI: `video review <ref> --release ID --expect-manifest-hash HASH --confirm-viewed --note TEXT`,
or `--request-changes --note TEXT`. These mutually exclusive explicit actions write an append-only
review record containing manifest hash, release/edition/aspect, timestamp, attestation and notes.
They make no provider/export calls; `--yes` alone cannot attest. New release/hash = unchecked.

Immutable export manifests carry the initial human-review status `unchecked` and identify the
review-record schema/location. Later reviews live in authoritative `.retold/history/` records,
not overwritten release manifests. Delivery/query output derives current status from the latest
record matching that exact hash. Package includes a review-status snapshot and referenced record
hashes in its own manifest, preserving original export checksums. Unreviewed/changes-requested
packages are allowed for review handoff but must be labeled **Review copy — not final delivery**
in human output, README and machine manifest. Only automatic-QA-passed and human-confirmed
selected releases may be labeled Ready to deliver. Packaging never approves or regenerates.

### 4.6 Linear and external accessible task path

`--plain` is a noninteractive linear alternative, not a promise that every full-screen terminal
works with every screen reader. `dev schema --kind commands|config` exposes required field names,
types and examples; `guides new <id> --input-file PATH` and `channels new <id> --input-file PATH`
accept schema-validated authored values to complete setup without TUI/editor prompts. Their schemas
cover the same user-owned fields/assets as the guided forms and never embed runtime defaults.
Creation validates a staged tree and rejects unsafe imports/paths, incomplete CONFIGURE fields,
unknown fields and overwrites; dry-run explains missing values. Templates remain optional.

The linear path is workspace init → schema/help → guide/channel creation/validation → provider
scan/add with explicit options → new draft → plan/continue to review checkpoint → export review
bundle → explicit version approval → continuation → explicit export → release inspect/verification
and final-viewing record. Each command returns actionable text or structured errors without cursor
control, prompts or automatic external application launch; all may use closed stdin.

`video review <ref> --write-bundle PATH [--lang LANG]` writes a read-only local review bundle with
UTF-8 original script/translation/captions, accessible labeled HTML, image alt text describing
subject and review status, referenced still/audio files, and a version/hash index. It validates
paths/collision/size, accepts `--dry-run`, and never renders media or calls providers. Existing
assets are copied; missing ones are listed. No hidden executable content or automatic upload.
Users choose their text/browser/media/assistive application. Audio files have available transcript
links; image descriptions are aids, not invented evidence that visual QA passed. HTML uses
semantic headings, labeled controls/links and language/direction metadata; mixed-script controls
remain separate from prose. This read-only handoff is distinct from live HTML review actions.

Return with `video review <ref> --lang LANG --approve script --expect-version ID
--expect-hash HASH` for one item; use the expected ID/hash from the bundle. Multiple
items use `--approval-file PATH` (JSON item/version/hash list), not one hash applied to all items.
Changed content rejects stale approval with an export-new-bundle action. Original text bytes and
copyable IDs remain available even if terminal shaping is unreliable. Final-video confirmation
uses the separate release command in §4.5. User confirmation remains human evidence, not an
automated accessibility/linguistic judgment.

At implementation, record the tested terminal/OS, no-color/ASCII mode, external browser/media
viewer and assistive technology versions. Test keyboard-only and one real screen-reader linear
workflow on macOS; record Linux coverage and limitations separately. Unsupported Urdu shaping
uses external review while preserving the same version identity; no claim of universal support.

### 4.7 Task-quality acceptance and issue closure

Before cutover run moderated, consented sessions with at least three first-time creators and
two experienced CLI operators, using fixture media/fake providers and no production spending.
If participants/equipment are unavailable, mark the gate pending; scripted agent walkthroughs
are useful preparation but never substitute for participant results. Do not fabricate outcomes.

Tasks: clean-home setup/new draft; find and approve script/cues; distinguish review, allowance
and export; fix failed Hindi voice while images run; compare/pin a take; locate and review an
exact portrait release; package that release; recover a rejected save/unsaved draft. Experts
also complete the equivalent closed-stdin path and inspect JSON without changing another edition.
Use a dense 3-part × 3-language series fixture, long titles/paths, Hindi/Urdu/mixed text, uncertain
charge, partial exports and 60×18 inspection. Every task states its target and success condition,
not the navigation steps; no advance documentation or coaching for the first novice attempt.

Record completion, first meaningful action, wrong turns, help/recovery requests, draft retention,
interpretation of next action, unintended side effects and participant comments; time-to-complete
is diagnostic rather than an invented speed promise. All participants must correctly distinguish
Prepare from Export and review from spending authority after using the flow. At least 80% of
core novice task attempts must finish unassisted. No accidental dispatch, export, bypassed review,
unrecoverable draft loss or unresolvable task blockage is acceptable. Accessibility participants
may use their normal assistive tools and the specified linear/external path.

Log issues with task, evidence, severity, owner and disposition. Any critical task/safety failure
blocks cutover; repeated comprehension/navigation failures require interaction changes and retest
of the affected journey. Cosmetic differences alone do not block. After fixes, rerun affected
tasks with recorded fresh attempts; aggregate pass percentages cannot hide a critical failure.

### 4.8 Required interaction evidence and phase assignment

P3/P4: first-run orientation, guided/linear authoring, navigation, focus and dirty drafts.
P5/P6: mixed-state feedback, queue/control/billing actions and independent-branch repair.
P7: version-aware approval, external handoff, blocker matrix and draft-conflict recovery.
P8/P9 with companion C9: release-specific QA/human review and review-copy packaging.
P10: terminal/assistive compatibility and all task-success sessions/retests.
These refine existing F01–F24, not optional new feature families. Unit/PTY tests cover every
row/state above; visual/manual evidence and participant task outcomes remain separate gates.


## 5. Command grammar and new capabilities

Canonical order is `retold <group> <action> <ref> [args]`; nested edit action is
`retold video edit <kind> <ref> [item]`. Ref-less actions (list/search/new) are explicit exceptions.
All mutating commands accept `--dry-run`; no mutating default ref is inferred. Generation,
provider tests and library media requests also require a finite selected `--usage-policy`; the
resource contract §2 defines setup/default-selection/error behavior independently of USD spend.

```text
retold workspace init <path> [--name NAME]
retold workspace show|check [--workspace PATH]
retold workspace list
retold workspace switch <path>
retold video list [--search TEXT] [--tag TAG] [--sort updated|title|state]
                   [--limit N] [--cursor CURSOR]
retold video tag <ref> --add TAG|--remove TAG
retold video plan <ref> [--compare PLAN_ID] [--save]
retold video inspect <ref> [--stage STAGE|--asset ASSET_ID|--why ITEM]
retold video history <ref> [--kind plan|edit|review|export|all]
retold video compare <ref> --left VERSION --right VERSION
retold video take <ref> --item ITEM --select VERSION
retold video note <ref> --item ITEM --text TEXT
retold video edition <ref> --lang LANG
retold video edit pronunciation <ref> --lang LANG --file PATH
retold video package <ref> --edition LANG --aspect ASPECT --release EXPORT_ID --output PATH
retold video verify <ref> [--scope work|delivery|all]
retold video paths <ref> [--kind all|work|delivery|state]
retold video clean <ref> [--scope temporary|rebuildable]
retold queue list|show
retold queue move <job-id> --before OTHER_JOB
retold queue pause|unpause|stop <job-id>
retold queue attach <job-id> [--after-sequence N] [--once]
retold series plan|run|review <ref> [--parts SELECTOR] [--lang LANG]
retold assets list [--video REF|--series REF] [--kind KIND]
retold assets show|verify|uses <asset-id>
retold storage usage [--by video|kind]
retold storage backup --output PATH [--video REF|--all]
retold storage verify <archive>
retold storage restore <archive> --into PATH
retold storage activate <path>
retold doctor bundle --output PATH
retold config profile list|new|show|use <name>
retold dev schema [--kind events|manifest|commands|config]
```

Queue commands require a studio; they report `RG-STUDIO-REQUIRED` when it is absent. Equivalent
foreground control remains `video stop/pause/unpause`. Profile application, switch and init are
configuration mutations, not execution. They cannot start jobs or select a surprise channel.
Config profile `list` takes no name; `new/show/use` require one. `queue show` requires a job ID.

`--workspace PATH` explicitly selects a registered workspace everywhere; `workspace switch`
sets the remembered default for later invocations and never moves or mutates existing jobs.
Precedence: explicit flag, `RETOLD_WORKSPACE`, remembered workspace, legacy discovery. Existing
`RETOLD_CONFIG_DIR`/`RETOLD_LIBRARY` overrides stay supported: conflicting explicit roots cause
a typed conflict error; they are never silently mixed across workspaces. All legacy discovery
paths are shown by `workspace show`; workspace creation has no implicit move/migration.

Plain list output is compact human-readable text. `--json` always uses parent §19.5 NDJSON;
no ad hoc JSON arrays or CSV stdout. A paginated list returns `items`, `next_cursor`, total when
known and its snapshot ID in the terminal payload. Cursors bind workspace, filters, ordering
and snapshot; tie-break with immutable ID, reject mismatched/expired cursor with a restart hint.
CLI callers can request 1–500 items; JSON defaults to 100 and never truncates without a cursor.

All compare/inspect/verify/paths/history queries are offline and read-only. `plan --save` is an
explicit journal mutation. Package/backup/bundle write files and accept dry-run and collision
preflight. Selecting a take or adding notes is a versioned edit; it does not generate media.
Notes guide only a subsequently authorized redo; no prompt-injection authority flows from them.

### 5.1 Required authoring action contracts

This table supplements the grammar above and is authoritative for these actions. All mutations
support read-only `--dry-run` (runtime contract §6), closed stdin and `--json`; schema-validated input is supplied explicitly,
never inferred from a terminal editor. TUI forms serialize the same request types. Every revision-changing request must include
`base_version` AND `base_hash` (or both expected-target CLI flags), checked under ownership;
source/alignment versions are additional preconditions, not substitutes. First creation expects
absence. Runtime §6 defines RG-EDIT-CONFLICT and preserved recovery proposals.

| Action / exact CLI | TUI route | Typed request → result; effect | Phase |
|---|---|---|---|
| `video edit translation <ref> --lang LANG --file PATH` | Edition → Translation | UTF-8 translation JSON `{base_version,base_hash,source_script_version,segments:[{id,text}]}` → edit ID, version, invalidation plan; validate stable segment IDs; offline edit revokes edition approval and stales its voice/alignment/captions/assembly and any mapped semantic visual/cue impacts under runtime §5 | P7/C8 |
| `video edit captions <ref> --lang LANG --script SCRIPT --file PATH` | Edition → Captions | Caption JSON `{base_version,base_hash,source_alignment_version,cues:[{id,text}]}` → edit/version/plan; BCP-47 script label, unchanged cue times; stales subtitle burn-in/assembly only, never voice | P7/C8 |
| `video edit voice <ref> --lang LANG --provider ID --voice ID --expect-version ID --expect-hash HASH` | Edition → Voice | Provider/voice IDs and edition → assignment version/plan; offline capability validation; stales that edition's voice/alignment/captions/assembly, not text or other editions | P7/C8 |
| `video edit pronunciation <ref> --lang LANG --file PATH` | Edition → Pronunciation | JSON `{base_version,base_hash,entries:[{word,spoken_form}]}` → pronunciation revision/affected sentence groups; offline edit, revoke affected audio approvals, stale true descendants | P7/C8 |
| `templates new <name> --from <ref>` / `templates show <name>` / `templates list` | New → Templates → Save/manage | Settings snapshot → template ID/version; only guide/channel IDs, inputs and preferences, never authority/credentials | P8 |
| `templates remove <name> --yes` | Templates → Remove | Name/version → tombstone; existing videos keep snapshots; no regeneration | P8 |
| `new --template NAME --input-file PATH --ref REF` | New → From template | Template plus JSON guide-input overrides → draft ID/plan; validates all required inputs; draft only, no dispatch without separate Continue | P8 |
| `views save <name> --filter-file PATH` / `views list` / `views show <name>` | Library → Save/manage view | JSON filters/sort/visible columns → named view/version; no cached results or runtime authority | P8 |
| `video list --view NAME` / `views remove <name> --yes` | Library → Saved views | Resolved saved filters → same paginated list; remove tombstones preference only | P8 |
| `export-presets new <name> --file PATH` / `export-presets show <name>` / `export-presets list` | Export → Presets → Manage | JSON `{aspects,container,video_codec,audio_codec,caption_mode}` → preset/version; validate actual engine support | P8/C7 |
| `export-presets remove <name> --yes` | Export presets → Remove | Name/version → tombstone; completed releases unchanged | P8 |
| `video export <ref> --edition LANG --preset NAME [--aspect ASPECT] [--spend USD]` | Video → Export → Edition/preset | Explicit edition and resolved preset snapshot, optional aspect subset → authorized job ID and selected target list; only this action may render/create project | P2/P9/C7/C9 |
| `video package <ref> --edition LANG --aspect ASPECT --release EXPORT_ID --output PATH` | Delivery → Release → Package | Exact committed release ID and destination → verified package manifest/path; copy only, never fallback to rendering or another release | P8/P9 |

Caption edits cannot rewrite timing silently; timing corrections use a separate approved alignment
redo plan. Assignment/translation edits do not approve new content or dispatch calls. Following an
edit, `video plan` identifies precise versioned changes and `continue` requires fresh authority
and current script approval before paid voice. Voice identities are references to configured user
assets/providers; templates never embed biometric samples or discover new identities automatically.
`video review <ref> --lang LANG --approve script|translation|captions|ITEMS` approves exactly the
specified current hashes; include `cues` for delivery cues and use §4.6 expected-version/hash
checks or an approval file. It is an explicit review action, never implied by Continue/`--yes`.
`--spend` applies to paid provider tests/export retries under parent §19. Every provider call
additionally uses the finite usage envelope and attempt identity in the resource contract.

Preset `--aspect` selects a subset, never silently adds an unsupported target. Initial supported
containers/codecs are exactly those exposed by the retained export adapter, recorded in schema;
an unsupported preset is rejected at write and export. Without a preset, require explicit
`--format` and `--edition`; TUI asks, noninteractive returns missing-input. `--project-only` still
requires explicit edition/targets and export authority. A missing, incomplete, wrong-edition or
corrupt package release returns an error with the exact export/verify action; never select latest.

Closed-stdin reference scenario (fake-provider fixture inputs, not real-media authorization):

```text
retold templates new harbor --from harbor/1 --json
retold new --template harbor --input-file inputs.json --ref harbor/2 --json
retold views save pending --filter-file blocked-view.json --json
retold video list --view pending --json
retold export-presets new portrait --file portrait.json --json
retold video continue harbor/2 --until storyboard --spend 0 --yes --json
retold video review harbor/2 --write-bundle ./initial-review --json
retold video review harbor/2 --approval-file initial-approvals.json --json
retold video continue harbor/2 --spend 0 --yes --json
retold video edit translation harbor/2 --lang hi --file hi-translation.json --json
retold video edit voice harbor/2 --lang hi --provider local-voice --voice narrator --expect-version BASE_ID --expect-hash BASE_HASH --json
retold video review harbor/2 --write-bundle ./revised-review --lang hi --json
retold video review harbor/2 --approval-file revised-approvals.json --json
retold video plan harbor/2 --json
retold video continue harbor/2 --spend 0 --yes --json
retold video edit captions harbor/2 --lang hi --script Latn --file hi-captions.json --json
retold video continue harbor/2 --spend 0 --yes --json
retold video export harbor/2 --edition hi --preset portrait --spend 0 --yes --json
retold video package harbor/2 --edition hi --aspect 9x16 --release EXPORT_ID --output ./handoff --json
```

The configured fixture template includes Hindi and a qualified zero-incremental-USD voice route.
The fixture first explicitly selects a finite usage policy; none of these commands has unlimited
subscription/local authority. That policy is a test input, not bundled runtime consent. Initial preparation
produces script/cues; if any review checkpoint blocks, the fixture explicitly bundles and approves
its current IDs/hashes before continuing. Approval JSON files use the exact bundle version list;
they model recorded reviewer choices, never automatic production approval. Translation/voice
edits revoke affected approval before renewed review. BASE_ID/BASE_HASH and file base preconditions come from the inspected current target; the fixture
never invents static IDs. Caption edits occur only after regenerated
alignment exists; their JSON pins that current alignment. EXPORT_ID comes from the actual export
result. The unreviewed package must be labeled Review copy; §4.5 confirmation is a separate test.
Parity assertions cover zero non-selected exports, exact invalidation and existing asset reuse.

## 6. Filesystem and structured deliverables

### Roots and ownership

`workspace.toml` records `schema_version`, stable workspace ID, root paths and display name.
Library defaults to `~/Retold` for legacy discovery; explicit workspace init proposes
`<workspace>/library` and shows the paths before writing. State defaults to the OS application
support directory scoped by workspace ID; cache uses the OS cache directory. Exact macOS/Linux
paths come from the existing directories abstraction and are exposed by `workspace show`.
Absolute runtime paths stay in local config/state; portable run manifests store relative paths.

```text
<workspace>/
  workspace.toml
  config/
    settings.toml                # references roots, preferences and selected profile
    providers.toml               # credential references only
    guides/<guide-id>/           # user-authored guide plus examples/rules
    channels/<channel-id>/       # user-authored channel and source assets
    profiles/<name>.toml         # preferences/routes/export settings, no permission
    templates/<name>.toml        # production inputs, no authorization or secret
  library/<channel>/<guide-defined folder levels>/
    README.md                    # generated navigation, current result summary
    review.html                  # offline entry point, generated without exporting
    work/
      01-research/ 02-script/ 03-storyboard/ 04-characters/
      05-images/ 06-voice/ 07-captions/ 08-edit/
    deliverables/<language>/<aspect>/releases/<export-id>/
      video/<slug>.<language>.<aspect>.mp4
      captions/  audio/  thumbnails/  metadata/  credits/
      manifest.json              # version, hashes, provenance, QA, actual file list
      CHECKSUMS.sha256
    deliverables/<language>/<aspect>/current.json  # atomic pointer to complete release
    .retold/
      run.json                   # identity/schema, guide/channel snapshots and pins
      state.json                 # committed projection; rebuild policy explicit
      artifacts/                 # authoritative immutable artifact/version manifests
      history/                   # plans, edits, approvals and undo references
      jobs/                      # job records/envelopes linked to accounting request IDs
      logs/events.ndjson         # typed redacted events
      logs/providers/            # bounded redacted diagnostics, private permissions
      guide/  channel/           # snapshots of actual inputs
      lock
```

```text
<state>/<workspace-id>/
  library.sqlite                # disposable search projection
  queue.sqlite                  # durable jobs/idempotency; not a rebuildable search index
  health/  migrations/  locks/  studio.sock
<account-state>/                 # canonical installation account, outside workspace roots
  registry.sqlite               # workspace bindings, account identity, shared profile identities
  accounting.sqlite             # authoritative charges/holds/caps across ALL workspaces
  health/  pool-slots/           # shared profile limits and concurrency ownership
  locks/admission  locks/activity # account-wide maintenance barrier
  backups/                      # financial snapshots/journal, never ordinary cache
<cache>/<workspace-id>/          # safe-to-refetch inputs only, never sole paid output
<series-root>/<series-id>/       # series.json, versioned registry/bible/assets, part refs
```

The new delivery tree is the default for newly created v2 runs. Legacy runs retain their
recorded layout adapter; discovery/export never blindly renames old folders. Every path is
resolved centrally by `RunLayout` plus a versioned layout policy. Guide folder rules still
control the creative hierarchy; users cannot inject paths escaping configured roots.
The series root is recorded explicitly in the workspace/series manifest; part refs are stable
IDs with relative root mappings, not duplicate writable copies or guessed folder names.

Source-of-truth classification must be declared per file type: authoritative, recoverable
projection, temporary or refetchable. Never call a paid output, approval, original source asset,
licence record, request identity or ledger “cache”. Raw audio/frames remain in work; final output
releases contain only verified exports or copies/references listed by their manifest.

### Delivery schema and publication

`manifest.json` carries `schema_version`, workspace/run/edition/export IDs, guide/channel and
source version hashes, creation timestamp, language/caption script/aspect, exact relative file
paths, byte sizes, SHA-256, MIME type, duration/resolution/loudness when applicable, QA outcome, initial human-review status and review-record schema/location,
licence/source references and included credit text. Costs are a redacted summary, never secrets.
Nullable/unmeasured values are explicit; do not invent measurements or mark unchecked QA passed.

A release is staged on the destination filesystem, verified, fsynced and published by atomic
rename, then `current.json` switches atomically only after required automatic QA passes (§4.5). Existing releases are immutable; no overwrite
of the last good export during a failed render/package. File-existence alone never means success.
Package copies an already completed selected release, verifies it and writes a portable manifest;
it never creates FilmCraft projects, generates thumbnails/audio or calls providers. Missing
requested files yield an actionable error. Releases do not claim to contain formats not exported.

Output names derive from sanitized slugs and explicit BCP-47 language plus aspect. Reject empty,
traversal, symlink escapes, reserved/control filenames and case/normalization collisions at
preflight. JSON IDs remain canonical regardless of displayed title or renamed folder label.
Cross-device moves use copy→verify→publish→retire with a journal, never assumed atomic rename.

## 7. Machine and human output contracts

`--json` remains one versioned NDJSON envelope per stdout line; money is integer USD micros,
times are RFC3339 UTC, durations integer milliseconds and refs use immutable IDs plus labels.
Example of a bounded `video show` result (the timestamp is illustrative):

```json
{"schema_version":1,"type":"result","request_id":"req-123","payload":{"outcome":"completed","workspace_id":"ws-1","run_id":"vid-1","ref":"harbor/1","state":"blocked","progress":{"done":14,"total":20},"cost":{"currency":"USD","spent_micros":320000,"held_micros":100000},"blockers":[{"code":"RG-REVIEW-REQUIRED","item":8,"stage":"images"}],"next_action":{"argv":["retold","video","review","harbor/1"],"requires_confirmation":false},"paths":{"root":"/workspace/library/channel/harbor/part-1","review":"/workspace/library/channel/harbor/part-1/review.html"}}}
```

The query succeeded even though the inspected run is blocked. Commands trying to execute blocked
work emit terminal `error` with exit 3; query snapshots use exit 0 for a valid blocked snapshot.
Every event supports optional `job_id` and sequence for replay; result/error carries the request
ID and resulting job ID where admitted. Error payloads have code, human message, structured
context and `suggested_actions` as argv arrays. Copied human shell commands use tested shell
quoting, not string concatenation. An explicit `redacted` flag marks removed sensitive fields.

For streaming, progress precedes exactly one terminal client outcome; disconnect after admission
reports the existing job ID. `queue attach --after-sequence` replays and follows without dispatch.
Schema changes are additive within a supported major version; breaking changes bump version,
with documented support/deprecation windows and golden compatibility fixtures. No provider tokens,
prompts or user source material appear in routine result envelopes by default.

Human output shows a stage summary, reused/new counts, spend versus holds, blocker and next
command. Estimates show a range and “unknown” where history is absent; never present an unknown
quote as free. Plain mode stays line-oriented, without cursor control, terminal width dependence
or prompts. `--quiet` suppresses progress only, preserving final result and actionable errors;
JSON + quiet preserves terminal envelopes. Respect NO_COLOR without changing semantic data.

## 8. Robustness, privacy and maintenance behavior

### Resource and offline controls

Local-only preflight validates required models/tools/fonts/paths without installation or login.
`doctor --fix` previews each repair; auth, model downloads and external programs require explicit
selected actions. Concurrency settings are upper bounds on shared provider pools; reductions
stop new admission without killing in-flight requests. Check free space before large artifacts
and at stage boundaries, retain recoverable partial output on ENOSPC and report exact affected
path plus recovery action. Disk estimates are bounds/unknown, not a promise to fit arbitrary work.

An offline invocation sends no network requests; local provider processes must be declared and
verified offline-capable before routing. If this cannot be guaranteed, block the route. Add
`--offline` across check/inspect/plan and execution. It never suppresses cost recording or grants
export authority. A fully cached completed plan makes zero provider calls in any mode.

### Backups and restore

Backups have a versioned index of selected manifests/assets and checksums, root/path mappings,
workspace/accounting lineage and omissions. Capture a quiescent snapshot under the account-wide maintenance barrier in §8.2 below and take
a consistent SQLite backup; reject rather than copy live partial state.
Do not include secrets, login profiles, sockets/locks, executable hooks or model downloads.
Encrypted backups and cloud transport are deferred; display sensitive content included in preview.

Restore validates size/count limits, paths, hashes, schema and archive expansion before publication
into a new empty destination. Never run archive content. Restored queues are stopped and authority
is revoked. Restored output/history is inspectable without activating execution. Activation requires
exclusive account/workspace ownership, a current authoritative financial journal and imported
unresolved holds; a stale snapshot cannot lower spend/caps or release charges. If newer accounting
cannot be verified, paid work remains blocked pending explicit billing reconciliation. Cloned
workspaces cannot execute concurrently against the same account lineage; display the conflict.

### 8.1 Canonical account and multi-workspace dispatch

For this release one OS-user installation has one canonical local account. Its registry and
accounting live in the OS application-state location outside workspace/library/config/cache
roots. Workspace init registers a stable ID, canonical root and binding there; ordinary commands
validate that binding before mutation. A copied ID at another root is inactive until registered
as an imported clone. No workspace/profile/env root override can select a fresh ledger or reset
account history. Explicit external process environments may use isolated accounts only in tests.

Every dispatch, including free provider requests, validates canonical account/workspace binding
and admission ownership. Shared paid reservations use this one accounting database: account
day/month caps cover all workspaces; workspace/video/series/job ceilings are additional limits.
No missing/corrupt account registry creates a replacement automatically while workspaces exist;
execution blocks with recovery guidance. Offline inspection of inactive workspaces is allowed.

Provider identity is the registered canonical credential/profile reference (never its secret),
not an arbitrary route alias. Rate-limit health and concurrency slots are shared by identity
across foreground processes and all workspace studios. OS-backed claims plus durable physical-attempt capacity records enforce concurrency
(runtime contract §3–§4); parent-lock disappearance alone never proves work stopped;
registry-configured pool changes serialize through the same account gate. No per-studio counter
can grant a slot. Accounting transactions are short and never wait for slot/OS locks or network.

Activation never installs backup accounting over the canonical database. Import original request
IDs/lineage idempotently and reconcile against current charges/holds without lowering counters,
dropping newer records or clearing unknowns. Missing newer financial state blocks paid work until
billing reconciliation establishes it. Only one active registered root per imported workspace
lineage; original and restored clone cannot both execute. Independent new workspaces may execute
concurrently under the shared account. Switching roots changes selection, never active identity.

These are local installation guarantees, not cross-machine quota coordination. An inactive
portable restore on a different machine requires an explicit transfer/reconciliation step and
user attestation that the old installation stopped; default paid execution remains blocked until
that is established. No distributed multi-host execution/automatic sync is promised in this release.

### 8.2 Maintenance barrier and selected backup closure

All account/workspace mutations (execution admission, studio dequeue, shared generation,
artifact/config edits, cap changes, migration, activation and cleanup) enter one account-wide
OS-backed barrier before acquiring any other filesystem locks: take admission mutex, acquire a
shared activity lock, release admission mutex, retain activity lock for the full operation/job.
Mutations within an owner reuse its token and never recursively acquire admission. Read-only
queries do not join and label any live snapshot accordingly.

Backup takes admission mutex then tries exclusive activity ownership **without waiting**. If an
owner is active it releases admission immediately, reports exact busy owners and changes nothing;
the user can pause/stop/drain those jobs and retry. It never holds admission while waiting for an
owner which might need control to finish. Once exclusive ownership succeeds, admission stays
closed through the consistent registry/run/accounting snapshot and staged archive publication.
Backup code reads under that token, never starts nested mutations. Cancellation/crash releases
OS locks automatically; partial archives are not published. There is no persistent busy flag
requiring manual deletion. Paused jobs release activity ownership after checkpointing.

Lock order: admission/activity gate → asset-generation → series registry or run (never run→shared
as parent §19.3.1 specifies) → short accounting transaction. Registry operations never wait for
run locks; no database transaction waits on a filesystem lock. Account registration, cap edits
and restore activation use the same gate, preventing root/limit changes during snapshots.

`backup --video` includes closure over the selected run's pinned sources, immutable shared
versions, licence records, guide/channel snapshots, relevant edit/review/job history and request
identities. Include a coherent financial lineage snapshot covering the account, with preview of
unrelated cost metadata included; unrelated video media is excluded. State the omission list.
This snapshot is recovery evidence, not authority to roll back unrelated charges on activation.
Take manifests and SQLite snapshots under the exclusive barrier, verify every reference/hash,
then publish archive/index together. `--all` covers all registered workspace roots; reject missing
roots rather than silently omit them. Read-only query caches need not be included.

Required proof: race backup against new execution, studio dequeue, series publication and
config/cap edits; it either reports busy or captures one coherent state. Kill during admission,
copy and publication; next execution can enter normally, and all reservations/results persist.
Two-workspace dispatch and restored-clone identity/overrides must pass shared cap and pool tests.

### Cleanup, diagnostics and local privacy

`clean` reports a byte count and exact path list before Trash moves. Temporary cleanup must not
remove active/staged job data; rebuildable cleanup may remove only declared projections/refetchable
artifacts, never version manifests or the only verified copy of an asset. Deleted videos have a
tombstone and retain accounting/audit references. Shared assets referenced by any version remain;
shared version garbage collection is deferred. Failed deletion is reported per path, not hidden.

Local support bundles are opt-in, preview file list and content classes, redact secret patterns,
credential paths, provider headers and source/prompt text by default. Never read provider profile
credentials for a bundle. Include versions, sanitized config references, bounded event/error
summaries and health checks. No automatic telemetry or uploading. Canary-secret tests are required.
Provider output is sanitized for terminal control characters and redacted before log persistence.
Logs are size-bounded/rotated; financial/artifact/review history referenced by active runs is not
silently pruned. Guide/source files are untrusted data, never execution instructions.

## 9. Completion gates and evidence

The parent ten phases still define build order; §3 assigns every added feature to them. A phase
closes only after its full vertical use cases, real engine adapters, CLI/TUI/machine routes,
errors/recovery and relevant acceptance tests pass. Disabled actions are permitted during
development only, not as a substitute for any required feature at cutover.

- Unit/property tests cover path/schema boundaries, state transitions and accounting invariants.
- Integration tests run J1–J5 against fake providers with exact call/charge assertions, plus parent
  §19.6 races/crashes/migrations. Use temporary roots and fake clocks; no paid/video permission inferred.
- PTY tests cover the screen matrix, navigation, resize, terminal restoration and disconnected
  studio. UTF-8/Bidi fixtures cover caption/path readability and exact copied command bytes.
- Filesystem tests include old/v2 layouts, case/Unicode collisions, symlinks, out-of-space,
  interrupted publication, moved roots, archive traversal and backup/restore accounting lineage.
- CLI contract tests cover every legacy inventory entry and new required command, missing inputs,
  dry-run zero effects, JSON parser errors, pagination stability and schema support compatibility.
- Performance evidence records machine/OS, fixture size, warm/cold runs and p50/p95: first usable
  TUI shell ≤150 ms warm (library loads asynchronously); indexed first page over 1,000 videos
  ≤100 ms p95 warm on the recorded reference machine. Slow uncached scans show progress within
  250 ms, remain cancellable and never block terminal input for a full scan.
- Final gate: workspace format/test/Clippy, dependency/security and clean-code checks required by
  the repository; both plan suites; and a manual keyboard/visual walkthrough with fixture assets.
  Record failures and unsupported environments; a pass count with unhandled errors is not green.

Per feature, store an evidence record under `docs/evidence/cli-rewrite/P<phase>/` containing its
feature IDs, build commit, commands/results, fixture/version hashes, schemas, PTY snapshots and
known limitations. No fabricated approval or “beautiful” claim without the visual walkthrough.
The interaction and participant task-quality protocol in §4.7–4.8 is also a cutover gate.
Release docs include install/migrate/downgrade, workspace paths, command/schema reference,
permissions, recovery and delivery manifests; reconcile README/operations examples at cutover.

## 10. Expansion tradeoffs and stop condition

More features increase the required implementation and validation work. Keep the application
layer small by reusing existing layout, store, typed events, provider adapters, guide schemas,
reservation semantics and review output. Extend them at their owning layer; avoid a parallel
pipeline, generic plugin system or separate permission model for the TUI.

The rewrite is finished only when every required feature F01–F24 and journey J1–J5 passes the
mapped gates, all supported old workflows migrate safely, the structured output contracts hold,
and both strict reviewers approve the implemented cutover evidence. Planning approval confirms
that this is a reviewable specification, not that those capabilities already work.


## 11. Efficiency without quality regression

`cost-quality-contract.md` adds normative subscription/resource budgets, every-call durability,
finite attempt trees, payload/evidence bounds, checker reuse, quality-constrained route evaluation
and an auditable <1% preventable-waste target. Its §10 assigns owners/phases and tests. Existing
24 features and UX task paths must show usage limits/unknown outcomes honestly; USD0 is not free
quota. No render/evaluation/sample may bypass explicit current production-media authority.


## 12. Runtime and integration completeness

`runtime-editing-contract.md` refines F01–F24 without new feature families: bounded event/input
paths, index commit recovery/freshness, global physical resource admission, orphan reconciliation
and fair lazy tickets; version-precondition edits, narration→visual/translation dependencies,
measured edition cue timing and live/static review boundaries. Read-only dry-run precedes all
mutation admission and cannot register/migrate/persist job authority or create hidden files.
Its §8 phase/proof matrix joins the required cutover evidence.


## 13. Stage fidelity and economical multi-source research

`production-stage-contract.md` specifies evidence-bearing stage outputs, deterministic API/HTTP
collection and private source snapshots, independent-source lineage/coverage and targeted AI
semantic work. Material event IDs/participants/witnesses/reveals propagate through script/shot/
pixel/audio/caption evidence; readiness joins current gates locally without a final whole-film
model call. Source count, fluent prose, a matched prompt or human Keep stale never waive factual
contradictions. Legacy semantic evidence remains unverified until explicitly adopted.

Reuse F05/F07–F11/F17/F24 inspectors and review bundles for event/source/depiction constraints;
keep primary copy human-readable (Who is present, What is seen, What is still uncertain). Broad
source discovery stays bounded; missing critical evidence offers targeted retrieval/source review,
not unlimited browsing. Phase ownership and adversarial scene fixtures are in that contract §10.


## 14. Explainable algorithmic efficiency

`algorithm-strategy.md` defines the required conservative retrieval/reuse/constraint interfaces and
optional qualified algorithm policies through existing feature/phase owners. Normal UI shows useful
source reuse/gaps/uncertainties and affected artifacts; Inspect records recipe versions, evidence
selection/omissions and actual resource/quality results. Algorithm names/similarity scores do not
replace factual confidence or promised savings. Plan/dry-run never starts embeddings/model downloads,
network collection or new optimization experiments. Baseline workflows remain supported when an
optional fusion/embedding/solver policy is absent, unqualified or worse than the baseline.
