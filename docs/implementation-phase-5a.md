# Phase 5A: Background Studio Daemon & Durable Queue Foundation

Date: 2026-10-19. Status: implemented and verified across full test suite and quality gates.

Delivering the foundational infrastructure for Phase 5 (`docs/plan-cli.md` §9, §17 row 5, §19.3, and `docs/cli-product-spec.md` §5), Phase 5A implements:
1. Durable job queue backed by SQLite (`queue.sqlite`), enforcing valid state machine transitions, idempotency keys, and priority ordering.
2. Background studio daemon lifecycle with singleton process lock (`locks/studio.lock`), Unix domain socket IPC (`studio.sock`), and client protocol.
3. CLI commands for `studio start|stop|status` and `queue list|show|pause|unpause|stop|move|attach`.
4. Fail-closed blocked diagnostics (`RG-STUDIO-REQUIRED`, exit code 3) when queue mutation actions are requested without a running studio.

---

## Capabilities & Architecture

### 1. Durable Queue Store (`retold-app::queue`)
- **Storage Layer (`queue.sqlite`)**:
  - Pure SQLite database with external SQL definitions loaded at compile time (`include_str!`).
  - Indexed by `(status, priority DESC, created_at ASC)`.
  - Supports persistent files on disk and in-memory databases for tests.
- **State Machine Transitions**:
  - `JobStatus`: `Queued`, `Running`, `Pausing`, `Paused`, `Stopped`, `Completed`, `Failed`, `Blocked`.
  - Enforces strict transition rules via `can_transition(from, to)`.
  - Terminal statuses (`Stopped`, `Completed`, `Failed`, `Blocked`) cannot transition to any active state.
- **Idempotency & Reordering**:
  - Enqueuing with an existing idempotency key returns the original job record without duplicating entries.
  - `move_before` adjusts target priority (`target.priority + 1`) to place jobs earlier in the execution schedule.

### 2. Studio Daemon & IPC Protocol (`retold-app::studio`)
- **Singleton Process Lock**:
  - Uses OS-level file locking (`locks/studio.lock` via `File::try_lock`) to guarantee exactly one studio daemon runs per workspace.
  - Returns `StudioError::AlreadyRunning` (`ErrorCode::AccountBusy`) if another process holds the lock.
- **Unix Domain Socket IPC**:
  - Listens on `studio.sock` with strict `0700` POSIX permissions.
  - Nonblocking socket with graceful request handling and shutdown.
  - Protocol requests: `Ping`, `Shutdown`, `ListQueue`, `GetJob`, `PauseJob`, `UnpauseJob`, `StopJob`.
  - Automatic cleanup of socket and lock on server drop.
- **Client Helpers & Status Queries**:
  - `query_status`: Non-blocking, offline-capable query checking socket and fallback queue database depth.
  - `send_request` / `stop_studio`: Fast JSON IPC with timeouts.

### 3. CLI Commands (`retold-cli`)
- **`studio start [--foreground]`**:
  - Spawns studio daemon in the background with detached pipes (`Stdio::null()`) or runs foreground loop.
- **`studio stop`**:
  - Gracefully stops the background studio daemon via IPC.
- **`studio status`**:
  - Reports whether studio is running, daemon PID, active job count, queued depth, and uptime.
- **`queue list` / `queue show <id>`**:
  - Offline-safe inspection of queued, running, and completed jobs.
- **`queue pause|unpause|stop|move|attach <id>`**:
  - Communicates with running studio over IPC.
  - Fails closed with `RG-STUDIO-REQUIRED` (`EXIT_BLOCKED` = 3) if studio daemon is not running.

---

## Quality & Contract Verification
- Added `ErrorCode::StudioRequired` (`RG-STUDIO-REQUIRED`, exit code 3) and updated golden contract `docs/schemas/retold-output-v1.json`.
- Zero clean-code violations against `clean-code.toml` preset `strictest`.
- Passed all checks in `./scripts/check.sh` (format, clippy, architecture, deny, docs, and workspace unit/integration test suites).
