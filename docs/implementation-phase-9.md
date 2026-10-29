# Phase 9: Series Registry & Multi-Part Commands

Date: 2026-10-19. Status: implemented and verified across full test suite and quality gates.

Closing out Phase 9 capabilities (`docs/plan-cli.md` §17 & §19.3.1, `plan-series-and-quality.md` §3 & §5), Phase 9 delivers:
1. Canonical multi-part series registry in `retold-app::series`.
2. Immutable part definitions and shared character sheets with stable cross-part pinning.
3. Series management CLI commands (`series new|list|show|plan|run|update`).
4. Top-level `new` shortcut for swift project creation.

---

## Capabilities & Architecture

### 1. Series Manifest & Invariants
- Series state lives in `series/<series-id>/series.toml`.
- Part admission locks immutable asset versions; subsequent part additions never invalidate or mutate earlier completed parts (preventing W1–W3 cascades).
- Character sheets are pinned per series; look changes produce versioned updates without modifying completed video runs.

### 2. Series CLI Subcommands
- `series new <guide> --work <title> [--parts N] [--budget USD]`: creates a new series entry.
- `series list`: enumerates all registered series in the workspace.
- `series show <series-id>`: displays part completion status, budget utilization, and character references.
- `series plan <series-id> [--parts P01,P02]`: dry-run execution plan for uncompleted parts.
- `series run <series-id> [--parts P01,P02] [--spend USD]`: schedules execution across selected parts.
- `series update <series-id> [--add-parts N]`: appends parts or updates series configuration safely.

---

## Quality Gate Verification
- Passed `cargo fmt --all --check`
- Passed `cargo clippy --workspace --all-targets -- -D warnings`
- Passed `cargo test -p retold --test clean_code` with zero baseline growth under `preset = "strictest"`
- Passed integration tests in `crates/retold-cli/tests/series.rs`
