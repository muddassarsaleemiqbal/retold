#!/usr/bin/env bash
# The one quality gate. Nothing is committed or pushed unless this passes.
#
#   scripts/check.sh          everything (pre-push)
#   scripts/check.sh --fast   formatting, lints and the Clean Code gate (pre-commit)
#
# Install the hooks once with `scripts/install-hooks.sh`.
set -euo pipefail
cd "$(dirname "$0")/.."

if command -v getconf >/dev/null 2>&1 && darwin_tmp="$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null)" && [[ -d "$darwin_tmp" ]]; then
    export TMPDIR="$darwin_tmp"
fi

fast=false
[[ "${1:-}" == "--fast" ]] && fast=true

step() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }

step "format"
cargo fmt --all --check

step "clippy (pedantic + nursery, warnings are errors)"
cargo clippy --workspace --all-targets --quiet -- -D warnings

step "clean code (strictest preset, ratcheting baseline)"
cargo test --quiet -p retold-cli --test clean_code

$fast && { step "fast checks passed"; exit 0; }

step "clippy on the opt-in acceptance suites"
cargo clippy -p retold-engine -p retold-filmcraft --all-targets --features acceptance --quiet -- -D warnings

step "architecture (one-way crate dependencies)"
python3 scripts/check-architecture.py

step "unused dependencies"
cargo machete

step "licences, advisories and sources"
cargo deny --log-level error check

step "docs (warnings are errors)"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --quiet

step "tests"
cargo test --workspace --quiet

step "trim stale build artifacts (keeps target/ small)"
python3 scripts/trim-target.py

step "all checks passed"
