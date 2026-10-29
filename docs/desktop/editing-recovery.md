# Retained editing drafts

The production shell checkpoints unsaved script-section, scene-direction and review-note text on
this device. [Scene recovery](scene-editing.md) retains action, location and mood together, including
incomplete text, and restores through explicit comparison with the current sequence.
Typing queues the latest buffer; a single background writer coalesces changes over a 300 ms
interval. Render and input callbacks perform no filesystem work. The editor distinguishes pending
retention, acknowledged retention and errors. A crash can lose text typed after the last acknowledged
checkpoint. Saving to the video remains a separate, version-checked action.

Startup reads retained drafts without opening their workspaces or restarting work. A compact
retained-draft notice expands to Restore, Copy text and confirmed Discard actions. Restoration
checks the canonical library and immutable production identity. Script sections match by name,
including when their order changed; a missing section keeps its checkpoint available to copy.
Recovered scripts open with the current saved text for comparison and require an explicit choice
before saving. Recovery cannot approve a script or authorize generation/export. Notes retain their
append identifiers and expected comment versions, so an acknowledged append whose reply was lost
is not duplicated after restart. Artifact comments also retain their typed subject and reviewed
content hash. Refresh never adopts replacement content; moving a draft to a new version retains
the new checkpoint before clearing the old one. Old video-note checkpoints serialize identically.

The profile's `desktop-drafts.json` has a strict schema and contains only editing text, production
identity and comparison context. It excludes credentials, run plans, financial allowances and worker
restart instructions. Inspection is bounded to 8 MiB and 32 drafts; script text is limited to 128 KiB
and notes to 8 KiB. Capacity, invalid data and oversized pastes never evict or truncate previous
checkpoints. New text stays in the editor and errors explain recovery. Files are published through
the private atomic writer with file/directory synchronization; corrupt files are not repaired.

Writes join the existing account's maintenance barrier and reject inactive restored profiles.
A profile-local writer lock serializes windows; every update and deletion also compares the last
acknowledged checkpoint version. A stale window cannot overwrite newer retained text or clear it
after a production save. Recovery conflicts can refresh the retained list while keeping active
editor text under separate identifiers. The original retained drafts remain available for review.

Production save acknowledgement clears only the matching local checkpoint, after in-flight retention
finishes. Newer typing coalesces behind that write and is kept. When only editing drafts are unsaved,
Quit offers keeping them or explicitly discarding them; keeping waits for durable retention, and
discarding waits for conditional removal. A retention failure keeps the window open. Other unfinished
provider/setup forms retain their existing discard review and are not persisted as editing drafts.

## Evidence

Application tests cover absent side-effect-free inspection, exact restart/retry receipts, private
permissions, concurrent-window updates and cleanup, target identity, corrupt/checksum/duplicate
records, input limits, writer contention, inactive profiles and exclusion of embedded execution
authority. The native suite launches separate GUI processes which type mixed Hindi/Urdu/English,
wait for a durable checkpoint and exit without close handlers. Reopening verifies unchanged
production content, comparison against an intervening external script edit, an explicitly saved
unapproved revision and cleanup. The note journey simulates a lost append response and verifies
no duplicate comment. No queue is created by recovery.

Compact light captures are `shell-760x640-recovery-retained-light.png`,
`shell-760x640-recovery-script-text-light.png` and
`shell-760x640-recovery-script-compare-light.png` in `/tmp/retold-gpui-g0-evidence`.
Integrated gate results are recorded in the completion ledger. Native operating-system quit-dialog
interaction still belongs to the final platform/user qualification; the process-exit fixture proves
restart persistence without treating automated input as an IME or accessibility certification.
