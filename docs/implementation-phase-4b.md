# Phase 4B: video screen (stages, blockers, plan panel) and read-only preservation

Date: 2026-10-19. Status: implemented and verified.

Building on the Phase 4A Home screen foundation, Phase 4B implements the interactive Video screen
(plan §6.2) for `retold-next`.

## Capabilities & Visual Layout

When a video run is opened from Home (by pressing Enter on a video entry in CONTINUE):
1. **Top Identity & Status Bar**:
   - Displays video reference, title, channel, guide, configured editions, and run ID.
   - Distinctly presents run state (e.g. active tasks from prior runs, failure markers) and plan state (blockers or ready planned stages).
2. **Stages Panel (Left Column)**:
   - Stages ordered strictly following their pipeline dependencies (e.g., research → script → voice).
   - Shows recorded steps, envelope-verified steps, unresolved steps, and human-readable decisions (`saved`, `not run yet`, `needs repair`).
   - Clearly marks stages scheduled to run under the active plan.
3. **Blockers Panel (Top Right)**:
   - Explains each blocker under the product spec §4.4 contract: what happened, what it affects, and honest remediation actions (e.g., specific `account limit` commands).
   - Shows "Nothing blocks this plan." when clean.
4. **Plan Panel (Bottom Right)**:
   - Details authority bounds (target stage, spending ceiling, paid requests, active routes, video budget).
   - Shows cost estimates and the exact CLI continuation command (`retold-next video continue <ref> --plan-hash <hash>`).
5. **Key Controls**:
   - `$` opens the detailed costs projection.
   - `p` opens the full plain plan text.
   - `c` displays an informational notice reminding users that continuation runs from the CLI in this phase; no execution or side effects are dispatched.
   - `Esc` returns cleanly to the Home screen.

## Read-Only Invariant & Store Fix

- Opening a video and inspecting its costs/plan in the TUI is strictly read-only.
- Fixed `crates/retold-app/src/video.rs` (`stage_costs`) to use `retold_engine::store::read_state` instead of `Store::open`, eliminating unintended `.retold/lock` overwrites during read-only queries.
- Verified via an interactive pseudo-terminal test (`script` / `pty`) asserting that all files and MD5 checksums across the library remain 100% identical before and after navigating through Home, Video, Costs, and exit.

## Verification

- `crates/retold-cli/src/tui/tests.rs`: in-memory terminal tests covering stage ordering, status separation, blocker rendering, related query navigation, and non-mutating key handling.
- `./scripts/check.sh --fast`: format, clippy, and clean code gate pass cleanly with 0 warnings and 0 new findings.
