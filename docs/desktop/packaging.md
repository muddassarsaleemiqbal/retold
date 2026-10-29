# Installable package

`python3.11 experiments/retold-desktop/package.py build` makes a release build and writes, under
`target/gpui-g0/dist`:

- `Retold Studio.app` — `Contents/MacOS/retold-studio` (the same binary serves the GUI and the
  packaged `--worker` mode), `Info.plist` (`dev.retold.studio`, macOS 13+) and
  `Contents/inventory.json` listing every file with size and SHA-256, plus version, commit,
  platform and `"signed": false`;
- `retold-studio-<version>-macos-<arch>.zip` and its `.sha256`.

The build then runs `package.py verify` on the archive: the digest must match, the extracted
bundle must contain exactly the inventoried files with matching checksums, and the packaged
binary must pass its `--worker-probe` smoke test in an empty scratch directory (no queue job is
ticked or dispatched). `tests/test_package.py` proves modified, extra and missing files fail.

Upgrade and data preservation: the bundle holds only the executable, `Info.plist` and the
inventory. Workspaces, the installation account, provider settings and preferences live outside
the bundle, so replacing it with a newer verified bundle cannot modify user data; existing
schema-versioned stores are read by the new binary under their own compatibility rules.

Recorded locally on 2026-10-28: `retold-studio-0.1.0-macos-arm64.zip` (2 files, unsigned) built
and verified. `package.py upgrade OLD.zip NEW.zip` initializes queue state with the old build, starts the new
build in a fresh directory, proves it refuses to reinitialize the existing state, and checks every
existing file is byte-identical; run locally with the 0.1.0 archive as both sides. **Not done:**
code signing and notarization (need a Developer ID and Apple credentials), a clean-profile install
on a second machine, an upgrade across two genuinely different released versions, and
Linux/Windows packages.
