# Native provider setup checkpoint

The Providers destination uses typed shared configuration operations for offline setup,
replacement, removal, task routing and route removal. Guided fields cover the seven
supported adapters; advanced editing uses the same validated TOML definitions as the CLI.
Discovery inspects installed tool/profile presence without spawning tools or reading secrets.

Every save has a separate before/after review. Its private authority binds the absolute
configuration root and exact original bytes, including comments and file absence. Apply
uses the existing CLI writer lock, keeps a backup, checks inactive recovery and account
maintenance, and rejects changed files or retargeted missing ancestors. A stale save keeps
the native form; refresh and a new review are required. Provider forms also participate in
workspace switching and quit guards.

Explicit offline status reads project saved health, retry times and profile/credential
presence. They reject changed configuration and malformed health data without repairing it.
Profile presence and earlier successful requests do not establish current authentication
or model readiness. Saving providers invalidates the native run review and sends no request.

Native fixtures exercise unconfigured setup, a concurrent client invalidating a save,
retained fields, confirmed configuration, speech fallback routing and offline health reads.
Shared regressions cover missing-file reads, CLI lock contention, exact-byte conflicts,
backups, referenced-provider removal, route clearing, symlink changes, invalid health
preservation and unverified profile presence.

The authoritative root `scripts/check.sh` and standalone desktop `check.sh` both passed
for this checkpoint. Verification included the preserved discovery/assets/canon edits;
those edits remain outside the desktop checkpoint.

This is part of G8, not full phase completion. Live provider testing, account/budget
administration, storage, authoring, assets and series workflows remain separate work.
The G0 media, assistive-technology and platform qualifications remain open.
