# Phase 3L: portable backup index verification foundation

`retold-next storage verify <directory>` checks a version 1 directory-backup index and
all listed file bytes offline. This is the first portable storage slice; workspace backup
creation, selected-run closure, semantic manifest/account validation, inactive restore and
activation are still pending. It does not claim that indexed content is a valid runnable workspace.

The strict `index.json` contains `schema_version: 1`, `format: "retold-workspace-backup"`,
nonempty `workspace_id` and `account_id`, `files` and `omissions`. Each file entry contains a
relative portable `path`, exact `bytes` and a lowercase BLAKE3 digest. Unknown index fields fail.
The verifier rejects traversal, absolute/Windows paths, duplicate/reserved paths, missing or
unindexed files, symlinks and special files. It streams checksums, caps index input at 4 MiB,
indexed bytes at 1 GiB and traversal entries at 10,000. These conservative limits apply to this
initial verifier; compressed archives are not supported.

Success reports `execution_authorized: false`. Verification writes nothing, selects no account,
contacts no provider, and never runs indexed content. Integrity is not authenticity or financial
reconciliation. Future restore must reverify the bytes it copies before atomic publication;
a prior verification result is not a durable admission token for concurrently mutable input.

Evidence: isolated CLI tests cover success/unchanged files, platform/traversal paths, oversized
entries, checksum mismatch, duplicate files, unsupported version, missing/unindexed files and
symlink files/parents. Repository fast gate, architecture check and focused CLI tests pass.
