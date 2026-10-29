# Native character review and saved-draft execution

The video overview now opens the actual saved character sheets. Select a character, inspect its
image, and approve that image before returning to a fresh run review. The page shows per-sheet
approval and total progress. It does not start generation or export after approval.

`retold-engine::character_approval` owns bounded sheet inspection and exact-content approval.
Records are limited to 4 MiB and 128 sheets; one selected image is read with a 32 MiB ceiling.
Canonical image paths must remain inside the video's channel. Verified image bytes are returned
to the frontend, which decodes them off the UI thread with dimension/allocation limits and a
1024-pixel preview bound. Rendering does not reopen the saved path.

Approval takes the existing generation lock, reads the current record and image again, and
compares the displayed content hash before atomically saving one character's approval. Changed
or missing bytes, malformed records, inactive work and active generation reject the write.
Other approvals are preserved. CLI/TUI character approval now takes the same generation lock.
The typed desktop application also joins the existing account maintenance barrier for mutation.

The native view clears the prior image when selecting or refreshing a sheet. Failed inspection
or approval removes the image and approval action, keeps a readable error, and offers refresh.
Late results cannot restore another workspace or video's preview. Unsaved script text blocks
entry; character operations invalidate run/export reviews. Run confirmation independently
compares its current target and allowance with the reviewed request before submitting.
Refresh reloads the current list, so a removed or renamed selected character cannot trap recovery.

## Acceptance

Five engine regressions cover read-only empty inspection, regenerated sheets, changed/missing
image bytes, preservation of other approvals, malformed approval records, channel boundaries,
and generation-lock ownership.

The native production fixture uses the existing zero-price loopback providers and a CLI-created
saved draft. Native controls review/confirm its run, reach the character gate, reject an approval
after a simulated record change, refresh, approve all three actual generated sheet artifacts,
review/confirm continuation to ready, and separately review/confirm an MP4 export. Double-click
confirmation admits exactly one job per run; the fixture requires exactly three jobs in total,
terminal completion and FFprobe-verified audio/video streams with positive duration. A CLI
approval attempted under a held generation lock must fail without changing approval bytes.

Final validation (2026-10-26): full shared `scripts/check.sh` and standalone desktop
`experiments/retold-desktop/check.sh` exited 0. Both mandatory native production journeys passed,
including renamed-character refresh recovery and the CLI locking regression. Logs:
`.git/retold-character-shared-locked.log` and `.git/retold-character-desktop-locked.log`.
The wide dark, conflict and compact light captures were visually inspected. No live providers
or historical recap jobs were used.

Captures are retained in `target/gpui-g0/evidence/native-production`: sheet selection, stale
approval rejection, compact light presentation, export review and completed export. The provider
uses synthetic images; these prove artifact wiring and interaction, not generated visual quality.

## Remaining boundaries

This proves saved-draft native execution on the development Mac. Source-backed draft creation
for this journey still uses the CLI fixture, and its worker is explicitly started by the fixture.
Native first-launch-to-export and packaged-worker media execution remain separate acceptance.
Scene/cast semantic editing, review notes, localized audio review, delivery browsing/playback,
series, storage, complete administration and remaining G0/platform/release gates stay open.
