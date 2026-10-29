# Native review notes

Video overview opens Video notes. Existing comments are shown first; Write a note opens a focused
multiline editor. An empty thread opens the editor immediately. Saving is explicit. Back and other
pages keep the draft in its window, with a Return to note notice. A dirty note blocks switching to
another video/workspace or starting another draft. Quit includes the note in its discard review.
Save errors and refreshes retain the text. A successful reply clears only the matching submitted
draft; text changed while a request was pending remains intact.

The shared `retold_app::notes` contract reads legacy `meta/reviews.json` comments together with
the new `.retold/review-notes.json` journal. New comments leave the legacy file and its approvals
and edit history unchanged. Upgraded CLI note writes use the same contract; CLI history and review
counts include both sources. There is no silent migration, no corrupt-file reset and no approval
or generation side effect.

Each append has a stable identifier and expected version. Versions bind the production identity,
canonical location, legacy bytes and journal bytes. The manifest is rechecked during observation
and publication. New writes join the account maintenance barrier and acquire the engine's generation
lock. Stale versions, inactive roots, active generation and conflicting identifier reuse reject
the write. An identical already saved append can be acknowledged read-only even while generation
is active; retrying it cannot add a second comment or renew production authority.

Both files are read with a 2 MiB ceiling. The new journal also limits entry count to 2000, comment
text to 8 KiB and target identifiers to 512 bytes. Invalid schema, identifiers, duplicate entries
and timestamps fail closed. Publication uses the engine's atomic writer. Pages contain at most 64
comments; chronological sorting uses parsed timestamps, including fractional seconds.

## Evidence

Eight application regressions cover read-only empty inspection, cross-workspace version rejection,
idempotent receipts, stale/conflicting requests, legacy-byte preservation and CLI history/count
parity, malformed journals, generation/inactive guards, timestamp order, duplicate records and
oversized input. The native journey types mixed Hindi/Urdu/English, navigates away and back, rejects
a stale append after an external write, keeps its draft through refresh, saves once under a double
click, and reopens saved comments in a compact light window. It creates no production queue.

Captures: `/tmp/retold-gpui-g0-evidence/shell-1024x700-notes-conflict-dark.png` and
`shell-760x640-notes-reopened-light.png`. Both were visually reviewed; final integrated gate results
are recorded in the completion ledger.

Process-crash persistence is implemented by [retained editing drafts](editing-recovery.md), including
append identifiers for lost-response retries. Artifact/version-specific entry points are described below; semantic cast/localized editing remains open.

## Notes on reviewed artifact versions

Script sections, scene direction, recorded frames and character sheets have separate native comment
entry points. The draft holds a typed subject and immutable content hash. Script targets hash the
saved section; direction targets hash the selected unit and shot; frame and sheet targets hash the
verified pixels. Approval bookkeeping and unrelated artifact changes do not rebind a comment.
Frame comments are available only after the saved image has loaded. Selecting another scene clears
its frame target immediately, and stale replies cannot populate a replacement note editor.

The contextual append contract verifies the journal version and current artifact under the same
production writer lock used to publish the comment. The generic append cannot bypass this check
with a reserved artifact key. A stale or unavailable artifact rejects new writes while retaining
historical comments and draft text. An already saved identical identifier is acknowledged before
current-artifact validation, preserving safe retries after replacement or a lost response.

Refresh keeps the original target. Compare current version shows both hashes; Move draft to current
version is a separate explicit action that preserves text, starts a new append/checkpoint identity
and leaves earlier comments on their original version. The new checkpoint is acknowledged before
conditional cleanup of the old checkpoint. Restoring a draft reopens its original historical context,
including when the artifact changed while Studio was closed. Old video-note checkpoints omit the
optional target field on serialization, preserving their existing checksums.

Focused application tests cover version separation, stale publication, pixel tampering, missing
artifacts, generation ownership, reserved keys, malformed targets, recovery identity and legacy
serialization. The native production journey exercises scene-note conflicts and explicit rebinding,
a process exit followed by compact-window recovery after an intervening scene edit, and a comment
on the loaded recorded frame without changing media bytes. Integrated outcomes and capture review
are recorded in the completion ledger. These additions do not implement semantic cast/localized
editing or close G5.
