# Phase 3M: managed workspace backup creation

`retold-next --library PATH storage backup --output NEW_DIRECTORY [--dry-run]` captures the
selected, already registered workspace's engine-owned run folders and canonical accounting.
No account or workspace is created implicitly. Plain and JSON commands use the same application
request. Run `storage verify NEW_DIRECTORY` to verify the resulting directory offline.

Scope: all discovered runs in one workspace, their `.retold` metadata/cache/guide/channel
snapshots, work, deliverables, review pages and shared cast folders. Nested cast folders are
copied once. Missing/corrupt manifests or recorded cache envelopes block publication. Symlinks,
special files, executable files and unsupported file types inside managed content are rejected.
Known credential/profile/lock/hook/model paths are omitted and recorded; arbitrary files outside
managed run/shared-cast folders are excluded. Scripts/media may contain private user content.
Preview states that the full account snapshot includes unrelated workspace cost metadata.

Creation closes canonical account admission, obtains exclusive activity, locks each discovered
run, validates cache evidence, then inventories/copies under that ownership. SQLite is snapshotted
under the same maintenance token without nested admission. The account's caps, job/request IDs,
charges and unresolved holds are recovery evidence, never installed over canonical accounting.
A busy account reports its owners; a busy run blocks before staging. Preview takes no ownership,
creates no lock files or staging and is explicitly a live read-only observation.

Output contains `workspace/`, `workspace.json` (original root/ID and archive mapping), `account/`
(consistent SQLite plus account manifest), and a strict checksummed `index.json`. Copying is
bounded, checks source and staged hashes, syncs files/directories and verifies the entire index
before publication. Private sibling staging is cleaned on ordinary failures. A killed process
may leave an unreferenced staging directory; it never publishes a partial destination or leaves
persistent account ownership. Atomic no-replace rename is used on Linux/macOS; Windows uses
its no-replace directory rename. Other platforms fail publication. A failed final parent sync
may leave a complete backup and reports that possibility rather than claiming nothing was written.

Validation: isolated CLI tests cover preview/no writes, round-trip managed files/account limits,
busy account/run refusal, secret canaries, symlinks, invalid/unregistered roots, nested/shared cast
and missing cache evidence. A publication regression creates the destination after staging and
proves even an empty existing directory is preserved. No live provider calls or media exports.

Remaining F18 work: `--all` across registered roots, `--video` with explicit dependency/reference
closure, validation of external asset/library references and licences, broader semantic backup
schema validation, inactive restore, authority revocation/stopped queues and safe activation/
financial reconciliation. This managed snapshot is not yet the complete portable recovery journey.

Verification completed on 2026-10-19: the original full `./scripts/check.sh` run
finished with all checks passed. An overlapping resumed run failed two local-text
subprocess assertions; the isolated `cargo test -p retold-providers --lib` rerun
passed all 51 tests after the original gate's artifact cleanup finished. No source
changes were needed. All eight workspace-backup scenarios passed.
