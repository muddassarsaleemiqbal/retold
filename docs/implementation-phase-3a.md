# Phase 3A: read-only configuration validation

`retold-next config validate --root PATH` checks an explicitly selected configuration folder.
It uses the engine's existing typed settings parser, provider schema and route checks, guide
catalog and channel loaders. Unsupported provider schema versions, unknown fields, invalid
routes/concurrency, malformed installed guides/channels and unreadable directories fail with
`RG-INPUT-REQUIRED` (exit 2). Provider configuration is required; absent settings use the existing
defaults, and empty guide/channel catalogs are valid configuration.

Validation never creates defaults, loads secrets, launches binaries, tests logins, dispatches
requests or writes configuration. Errors intentionally omit parser excerpts and source values.
JSON uses the existing terminal envelope; success reports `data.valid = true`. This checks
configuration structure, not provider availability or production readiness.

This is a bounded first slice of Phase 3. Versioned settings, config get/set/edit, schemas,
backups/restore, migration and provider scan/add/test remain unfinished. No Phase 3 completion
or real-provider evidence is claimed.

Regression tests exercise valid and invalid provider files with an empty PATH, unsupported
versions, unknown secret-bearing fields, route errors, one terminal JSON envelope and byte-for-byte preservation of source files.
