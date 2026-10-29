# Phase 7: Review, Editing, Undo/Redo & Locking Semantics

Date: 2026-10-19. Status: implemented and verified across full test suite and quality gates.

Closing out Phase 7 capabilities (`docs/plan-cli.md` §17 & §19.4), Phase 7 delivers:
1. Video review inspection and note recording.
2. Atomic `approve` (alias `lock`) recording reviewed content hash and pinned input versions.
3. Explicit `unlock` revoking approval protection without implicit regeneration.
4. Targeted shot and line editing (`video edit shot`, `video edit line`) that invalidates only true descendants.
5. Linear undo and redo history for non-destructive creative adjustments.

---

## Capabilities & Architecture

### 1. Atomic Approval and Protection (`video approve`, `video unlock`)
- `video approve` (`lock`):
  - Captures and stores an approved artifact snapshot with cryptographic verification.
  - Pinned input versions and reviewed hashes prevent upstream drift from silently mutating approved results.
- `video unlock`:
  - Revokes protection on specific targets or runs.
  - Does not trigger unrequested generation; downstream work requires explicit plans and fresh authorization.

### 2. Fine-Grained Editing (`video edit shot`, `video edit line`)
- Editing one shot invalidates only that frame and assembly; unrelated frames are preserved.
- Editing a narration line invalidates only its sentence group and audio alignment.
- Edits persist new versioned intent without performing paid calls or background generation.

### 3. Review Notes & Undo/Redo
- `video review`: provides an aggregated summary of run status, blockers, and review notes.
- `video note`: appends reviewer observations and directions to durable run metadata.
- `video undo` / `video redo`: navigates versioned state history safely without data loss.

---

## Quality Gate Verification
- Passed `cargo fmt --all --check`
- Passed `cargo clippy --workspace --all-targets -- -D warnings`
- Passed `cargo test -p retold --test clean_code` with zero baseline growth under `preset = "strictest"`
- Passed integration tests in `crates/retold-cli/tests/`
