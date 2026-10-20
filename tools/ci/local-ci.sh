#!/usr/bin/env bash
# Runs the same checks as .github/workflows/ci.yml locally.
set -euo pipefail
cd "$(dirname "$0")/../.."

BINDINGS=apps/desktop/src/shared/ipc/bindings.ts
bindings_before=$(git hash-object "$BINDINGS" 2>/dev/null || echo none)

echo "== cargo fmt";    cargo fmt --all --check
echo "== cargo clippy"; cargo clippy --workspace --all-targets --locked -- -D warnings
echo "== cargo test";   cargo test --workspace --locked
echo "== boundaries";   bash tools/ci/check-crate-boundaries.sh

# `cargo test` regenerates the TS bindings; they must already match the Rust side.
# (insta snapshots fail their own tests on mismatch.)
echo "== generated bindings are up to date"
if [ "$(git hash-object "$BINDINGS")" != "$bindings_before" ]; then
  echo "$BINDINGS was out of date and has been regenerated — review and commit it"; exit 1
fi

if [ -f pnpm-lock.yaml ]; then
  echo "== pnpm";       pnpm install --frozen-lockfile
  pnpm format:check
  pnpm -r --if-present lint
  pnpm -r --if-present typecheck
  pnpm -r --if-present test
fi
echo "local CI: OK"
