# Phase 3K: account maintenance and recovery snapshots

Delivered: `account backup --output PATH [--dry-run]` and `account verify-backup PATH`.
Backups capture installation accounting only, excluding credentials, login profiles and workspace
media/configuration. Portable workspace backup, restore and activation remain pending.

Mutations take shared OS-backed activity ownership; continue/export and provider-test jobs retain
it through execution and settlement. Backup closes admission and refuses active owners with
`RG-ACCOUNT-BUSY`, including process and purpose. Crashed owners release locks automatically.
SQLite snapshots are staged beside the destination, checksummed and published by rename.
Verification checks format/version, checksum, account identity, timezone and recorded balances/counts
offline. It never replaces canonical accounting or authorizes dispatch.

Evidence: app/CLI backup tests cover live owners, crash release, snapshot admission, unresolved
holds, offline and foreign-account verification, tampering, existing/nested destinations and dry run.
No live provider calls or video exports were performed.
