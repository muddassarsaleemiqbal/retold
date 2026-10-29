# Phase 3B: offline provider discovery

`retold-next providers scan` inventories `claude`, `codex`, `gemini`,
`llama-completion`, `filmcraft` and `ffmpeg` on PATH, plus immediate home folders
matching `.claude*` and `.codex*`. On Unix, tool candidates must be regular files
with an execute bit. Tool symlinks may resolve to regular executable files.
The scan uses at most 256 PATH entries and 4096 immediate home entries; exceeding
either limit fails explicitly rather than returning an apparently complete inventory.
Missing tools are normal inventory results, not execution failures.

JSON reports paths, profile folders and presence booleans for OPENROUTER_API_KEY,
ELEVENLABS_API_KEY, GEMINI_API_KEY and OPENAI_API_KEY. Values are never included.
Plain output gives per-tool presence and a profile-folder count. Both outputs
explicitly say that versions and authentication were not checked.

The scan never executes binaries (including `--version`), reads login files,
loads `.env`, queries Keychain, contacts providers, writes configuration or opens
an editor. Discovery grants no execution authority. Malformed provider configuration
does not prevent offline discovery. Empty PATH entries follow normal PATH semantics.

This is partial discovery evidence, not completion of Phase 3 or the full §7.1
specification: version probing, login identity/health, local model-file inventory,
FilmCraft app-bundle discovery, `.env`/Keychain detection, provider add/test,
backups and migrations remain pending. Backup/restore requires the account-wide
maintenance barrier and cannot be implemented safely as an ordinary file copy.

Tests use temporary homes and tools with invalid program contents, verify no
subprocess is needed, distinguish nonexecutable files/directories, exercise limits
and unreadable roots, and check one terminal JSON envelope, credential redaction
and no created files.

Validation: 460 workspace tests passed (`cargo test --workspace --all-targets`),
including Clean Code with the existing baseline. Strict Clippy passed for engine,
app and CLI, along with architecture, formatting and whitespace checks. A local
plain-output smoke scan completed without launching any discovered tools.
