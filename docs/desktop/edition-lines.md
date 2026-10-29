# Edition lines

The native **Edit edition lines** workspace shows each transcreated language edition beside its
generated original. One line is edited at a time; review reports the rebuilt stages (voice and its
descendants — captions, timelines, deliveries — never pictures) and whether the edit restores the
generated original. Confirmation publishes exactly the reviewed text, clears that edition's
approval and keeps the generated `units.json` unchanged. Approval binds the edition's current
content hash.

The engine stores per-language overrides under `.retold/edits/dialogue/<language>.json`. The
overrides join the editing digest only once one exists, so existing plan hashes are unchanged.
`editing::ready` rejects any edited, unapproved edition before provider dispatch, and the
edition pipeline applies approved overrides after transcreation, failing closed when the stored
approval no longer matches. Language names are restricted to tag characters; empty, over-limit,
stale or changed reviews are rejected without writing.

Acceptance: three engine regressions (`tests/dialogue_editing.rs`) cover read-only impact,
held-until-approved publication, unchanged generated originals, digest change, stale-version
rejection, restoration and input validation. A fourth regression runs Voice, approves an edited line, rebuilds the
context and runs Voice again, proving the speech request carries the approved text. Unsaved line text is checkpointed in the private recovery store (bound to
language and line, never to approval) and can be restored after a restart for comparison with the
saved edition. Not yet covered: a native journey for the lines workspace.
