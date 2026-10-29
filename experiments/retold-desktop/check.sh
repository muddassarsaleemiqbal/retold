#!/usr/bin/env bash
# Desktop dependency policy stays separate; native acceptance reuses the CLI fixture.
set -euo pipefail
task_root="$(cd "$(dirname "$0")/../.." && pwd)"
task_manifest="$task_root/experiments/retold-desktop/Cargo.toml"
task_target="${CARGO_TARGET_DIR:-$task_root/target}"

cargo fmt --manifest-path "$task_manifest" --check
cargo clippy --manifest-path "$task_manifest" --target-dir "$task_target" --locked --all-targets -- -D warnings
SDL_AUDIODRIVER=dummy cargo test --manifest-path "$task_manifest" --target-dir "$task_target" --locked --lib --test contracts --test native --test shell_native --test worker
cargo build --manifest-path "$task_manifest" --target-dir "$task_target" --locked --bin retold-studio
python3.11 "$task_root/experiments/retold-desktop/native-production.py"
python3.11 "$task_root/experiments/retold-desktop/audit-policy.py"
python3.11 -m unittest discover -s "$task_root/experiments/retold-desktop/tests" -p 'test_*.py'
cargo deny --manifest-path "$task_manifest" --config "$task_root/experiments/retold-desktop/deny.toml" --locked check
printf 'Native desktop checks passed\n'
