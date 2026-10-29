# Saved scene direction

The native Scenes workspace reads the actual saved storyboard and recorded scene frame. It shows
narration, duration, cast and visual direction for the selected scene. Previous/next controls and
a searchable keyboard picker navigate the sequence; compact windows stack the preview and editor.
Missing images have a small explicit state. A recorded frame can precede newly saved direction,
so the workspace labels it as the last recorded frame rather than a rebuilt result.

Action, location and mood are local text drafts. Review computes changed scenes, dependent recuts,
neighbors whose shared image-sheet grouping changes, and visual descendant stages. Confirmation
publishes only the reviewed values. Narration and casting are preserved; changing a recut scene
detaches it so its new action receives its own image. Saving revokes sequence approval. Approval
binds the complete saved board hash and remains separate from a reviewed production run.

The application contract binds canonical library, immutable video identity, scene, expected version
and exact field values. Reusing a stale review, changing reviewed text or racing another writer
cannot publish. A frame redo changes the edit version because image grouping may have changed;
it does not revoke approval for unchanged direction. Generation ownership, account maintenance
and inactive-root barriers remain in force. Impact review is read-only and creates no jobs.

The engine stores a pointer and immutable content-addressed scene baselines/revisions under
`.retold/edits`. The generated storyboard mirror and cache remain original. Only a verified engine
storyboard run establishes the trusted baseline; older productions stay inspectable until an
explicit reviewed run through Scenes. Manual direction is checked before any provider dispatch and
consumed by both review HTML and the media workflow. Source, script, guide, research or storyboard
changes block obsolete manual content. Undo remains available to unwind it. Undo/redo restore real
direction, invalidate affected images and require fresh approval; history pages contain at most 64
entries and reject stale cursors.

Unsaved scene drafts use the existing private, version-checked recovery store. They retain all three
fields and comparison context, including incomplete text. Publication still requires nonempty text
within action 4 KiB, location 1 KiB and mood 256 byte limits. Restart restoration requires comparison
with the current saved sequence. Copy includes every field. Recovery cannot approve or run work.
Over-limit edits preserve the previous checkpoint and keep the editor open with a recovery message.

Frame lookup uses `images.json` and the unit's recorded asset, including panels and recuts. It
validates production containment, regular-file identity, current sequence version and recorded
content hash. A newer selection clears the old preview immediately. Lookup and decoding each use
one active request and one latest pending selection; old replies cannot populate a different scene,
workspace or replacement editor. No filesystem work or decoding occurs in rendering/input paths.

## Semantic casting

The Scenes workspace lists who appears in the selected scene and the expression each person shows.
Characters can be added from the saved character profiles, removed, or given another of their first
five profile expressions (those with drawn reference-sheet poses). At most six people appear in one
scene. Review reports the frames to redraw; confirmation publishes exactly the reviewed cast through
the same versioned scene journal, detaches a recut, revokes sequence approval and leaves narration,
voices and visual text unchanged. Names outside the saved profiles, duplicates, incomplete entries,
stale versions and a changed review are rejected without touching the journal. Videos without
character profiles show that they cannot be recast. An unsaved cast is checkpointed with the scene's recovery draft (older
checkpoints without a cast still load) and restored after a restart; leaving the scene with an
unsaved cast is blocked like other unsaved direction.

## Acceptance

Engine regressions cover authoritative content, approval before dispatch, unchanged generated
originals, source/cache integrity, generation-lock conflicts, read-only impact, legacy baselines,
history, undo/redo and image-revision scope. A real zero-price Images workflow verifies that an
eligible one-scene edit issues exactly one new image request containing the saved action while
reusing unaffected provider results. Shared-panel grouping has separate impact coverage; that
single-scene result is not a promise that all guides rebuild only one frame.

Application regressions exercise recorded frame resolution, production containment and hash
tampering that preserves size/time/inode. Recovery tests cover all fields and incomplete text.
The native production journey types and reviews real direction, publishes an unapproved revision,
handles an external conflict, rejects a changed review and stale retry, and separately approves.
A GUI child exits without close handlers; a compact light window restores, compares, saves and
approves all three mixed-language fields. Editing/recovery creates no additional queue jobs.
The subsequent reviewed Ready run produces a real recorded frame; native tests search/select with
Enter, dismiss with Escape and rapidly change scene selection before capturing the loaded frame.

Captures are under `target/gpui-g0/evidence/native-production` with suffixes
`native-scene-impact-dark`, `native-scene-conflict-dark`, `native-scene-picker-dark`,
`native-scene-frame-dark` and `native-scene-recovered-light`. Integrated gate outcomes are recorded
in the [completion ledger](completion-ledger.md). This slice does not implement semantic casting,
edition-specific narration edits, waveform authoring or targeted reject/redo planning, and does not
close G5 or certify operating-system accessibility/IME behavior.
