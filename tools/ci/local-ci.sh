#!/usr/bin/env bash
# Runs the same checks as .github/workflows/ci.yml locally.
set -euo pipefail
cd "$(dirname "$0")/../.."

echo "== cargo fmt";    cargo fmt --all --check
echo "== cargo clippy"; cargo clippy --workspace --all-targets --locked -- -D warnings
echo "== cargo test";   cargo test --workspace --locked
echo "== boundaries";   bash tools/ci/check-crate-boundaries.sh
if [ -f pnpm-lock.yaml ]; then
  echo "== pnpm";       pnpm install --frozen-lockfile
  pnpm format:check
  pnpm -r --if-present lint
  pnpm -r --if-present typecheck
  pnpm -r --if-present test
fi
echo "== generated files are committed"; git diff --exit-code -- '*.ts' '*.snap' || {
  echo "generated files changed — commit them"; exit 1; }
echo "local CI: OK"
