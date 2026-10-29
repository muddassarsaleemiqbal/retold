# Delivery presets, destinations and history

The export review offers four shapes — landscape 16:9, portrait 9:16, square 1:1, or landscape and
portrait together — each pinned as the exact format list in the plan hash, and **Preset** choices (guide defaults or any saved export preset) and the
preset's supported aspects; choosing one discards any previous review, so the plan hash always
binds the selected preset and aspect.

In **Saved media**, a selected file can be saved with **Save a copy to…**. The file's saved identity
is revalidated, its BLAKE3 digest computed, and the copy written to a hidden staging file in the
chosen folder, re-hashed, then renamed into place. If a file with the same name exists, the app
asks to **Replace existing file** or keep it; nothing is replaced silently. Every copy is appended
to the video's `.retold/deliveries.jsonl`. **Check delivered copies** lists that history, newest
first, re-hashing each destination to show whether it still matches what was delivered.

`tests/desktop_destinations.rs` covers verbatim copy, refusal without consent, explicit
replacement and detection of a later external edit. Upload destinations (platform APIs) are
outside this release.
