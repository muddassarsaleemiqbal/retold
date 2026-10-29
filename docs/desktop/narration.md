# Narration workspace

**Listen to narration** lists every recorded edition from `editions.json`. Choosing one decodes
its narration (WAV or MP3, at most 192 MiB) in the background into 240 peak columns, after
checking that the file is contained by the production, keeps its file identity and matches the
recorded content hash. The waveform, the native player for the same audio item and the aligned
line start times are shown together; only the latest edition request can populate the page.
The workspace is read-only: text changes go through [edition lines](edition-lines.md) and a
reviewed run records them again.

Acceptance: provider unit tests cover a synthetic quiet-then-loud WAV envelope/duration and
rejection of zero, excessive and undecodable requests; an engine workflow regression proves a
captioned fixture exposes timed lines and a bounded envelope, and that a byte-altered copy is
rejected. Each line's start time is a button that plays the narration from that point, replacing any
playback in progress. The native production journey opens Narration on a generated video, waits
for the loaded player, reveals the retake control and starts playback from the first line.
