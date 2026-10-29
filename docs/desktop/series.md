# Native series workspace

**Series** is a top-level destination. It lists registered series, opens one as a part matrix and
shows, for every part, its linked video and how many stages are saved (observed from the linked
run, never from a queue receipt). **Link video** offers workspace videos; the shared registry
rejects a video whose guide or channel differs from the series. Selecting parts builds the exact
part selector; **Review plan** compiles every selected part's own plan through the shared series
planner, which divides the total allowance, blocks unlinked parts and parts that share a video,
and returns one plan hash. **Submit reviewed parts** passes only that hash; any change to the
manifest, selection, allowance or part plans makes the run refuse before queueing.

`retold_app::desktop::series` is the typed projection the native client uses. Two regressions
(`tests/desktop_series.rs`) cover the three-part registry/matrix, unlinked-part blockers, forged
and blocked plan hashes queueing nothing, and duplicate selectors. The native shell suite renders
the destination.

**New series…** names the work, chooses a guide (from the workspace's guides) and 2–6 parts; the
shared registry creates unlinked parts and nothing is queued. The contract rejects empty titles
and more than 24 parts.

**Add a part / Add three parts** previews the new unlinked part IDs; confirmation adds them only
while the series is still at the reviewed version, so a concurrent change forces a new review and
a cancelled review changes nothing.

Not yet implemented natively: asset version/usage/take inspection, update-impact
review, music/SFX provenance and the three-linked-part generation acceptance journey.

## Materials and audio provenance

**Inspect materials** on a video lists its saved files (at most 200) from the shared asset
browser. Selecting one shows its provider/source/stage provenance, take number and selection, and
every usage with the stage it would rebuild — the update-impact view for that material. **Verify
file integrity** re-hashes the saved file against its recorded checksum. The same page lists
imported music and sound effects with licence type, commercial-use permission and verification
state. `retold_app::desktop::materials` is the typed projection; `tests/desktop_materials.rs`
covers listing, detail and the empty audio library. **Import licensed audio** chooses music or sound effect, records the declared licence and
commercial-use permission (a licence name is required), then copies the chosen file into the
library. Reviewed version updates remain CLI-only.

## Three linked parts reuse one cast

`crates/retold-engine/tests/series_reuse.rs` creates three parts of one series in a shared
library (`series/nosferatu/part-1`, `part-2`, `part-3`), runs each through Characters with the same
zero-price fakes, and proves all three resolve the same shared cast folder while only the first
part sends character-sheet image requests; parts two and three reuse those sheets with no new
requests.
