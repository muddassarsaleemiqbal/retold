# Source-backed creation and production media

The native draft form accepts an optional UTF-8 source through a path field or native chooser.
Preview reads at most 2 MB and shows at most 12,000 characters. Changing the path clears the
review; saving rechecks both the selected path and the complete source digest. A private snapshot
holds those exact bytes while the engine atomically publishes the draft. Later changes to the
external source cannot alter the captured source. Errors retain the form. Offline saving neither
initializes a queue nor grants provider authority.

The production overview opens Saved media. It lists images, audio, video, captions and supported
editor projects, with edition filtering and shared material retained. Pages contain at most 48 rows;
traversal examines at most 4096 entries and descends at most eight directories. Symlinks and
non-UTF-8 paths are omitted and disclosed as a partial listing. File existence is never described
as export readiness. Selected captions/projects are limited to 128 KiB; selected image input to
32 MiB, with bounded decoded dimensions/allocation and a 1024-pixel preview.

The production player reuses the bounded FFmpeg adapter, separately from the fictional lab.
It has one active inspection/decoder and one pending replacement. Selection, filter, page and
workspace changes clear the old preview, cancel active work and reject late replies. Audio/video
playback supplies Play/Pause and five-second seeking; preview subprocesses exit before another
decoder starts. Rendering performs no filesystem reads. Show saved file uses the platform reveal
action; supported editor projects also offer explicit system opening.
Preview and handoff recheck canonical path, regular-file status, size, modification time and,
on the qualified Unix backend, device/inode identity. A replaced file requires refresh; handoff
validation runs outside input/render callbacks and cannot open a selection that changed meanwhile.

## Cross-frontend cache integrity

The packaged-worker fixture exposed different `serde_json` feature graphs: GPUI enables
`preserve_order`, while the headless graph uses sorted objects. Hash serialization now sorts maps
recursively while retaining declared struct-field order, numeric map ordering and sequence order.
This preserves the legacy headless fingerprints for those cases, including existing sorted cache
envelopes. Regression tests run under both JSON feature configurations. The Clean Code baseline
is unchanged; its gate narrowly excludes five externally required Serde method signatures from
the two signature-shape rules that those traits cannot satisfy. All other rules still apply.

## Acceptance scope

The native production journey now saves its source-backed draft in the GUI, rejects a changed
source without publishing, checks captured bytes and starts the packaged GUI binary in worker
mode. Its existing character review, ready, export, stream-probe and cross-client locking checks
remain mandatory. Both production journeys additionally select the exported MP4 in Saved media,
play/pause it, seek and clear the selection through a filter refresh. A shorter synthetic-only
native player journey exercises the same interaction without a generation run.

These additions do not close semantic scene/cast/localized editing, review notes, series, complete
administration, installation packaging, human A/V synchronization or the remaining platform and
accessibility gates. Current integrated gate results are recorded in the completion ledger.
