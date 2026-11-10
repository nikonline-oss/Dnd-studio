#!/usr/bin/env bash
# Release readiness checks (roadmap R4.1/R4.4). Run without arguments locally: it reports what is
# missing without failing on a template repository. The release workflow runs it with --strict and
# refuses to build when anything is unset.
#
#   bash tools/ci/check-release.sh            # report, exit 0 unless a hard error
#   bash tools/ci/check-release.sh --strict   # any missing item fails (used by release.yml)
set -euo pipefail
cd "$(dirname "$0")/../.."

strict=0
[[ "${1:-}" == "--strict" ]] && strict=1
status=0
missing=0
problem() {
  echo "  MISSING: $*"
  missing=$((missing + 1))
  if [[ $strict -eq 1 ]]; then status=1; fi
}
ok() { echo "  ok: $*"; }

CONF=apps/desktop/src-tauri/tauri.release.conf.json
CARGO_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
TAURI_VERSION=$(sed -n 's/^  "version": "\(.*\)",/\1/p' apps/desktop/package.json | head -1)

echo "== updater"
pubkey=$(sed -n 's/.*"pubkey": "\(.*\)".*/\1/p' "$CONF")
case "$pubkey" in
  "" | REPLACE* | *PLACEHOLDER*) problem "updater pubkey in $CONF is not set"
    echo "     Generate a key with: cd apps/desktop && pnpm tauri signer generate -w ~/.tauri/dndstudio.key"
    echo "     Put the public key here and the private key into the TAURI_SIGNING_PRIVATE_KEY secret." ;;
  RW*) ok "updater pubkey is a minisign key" ;;
  *) problem "updater pubkey does not look like a minisign public key (expected it to start with RW)" ;;
esac
if grep -q "OWNER/REPO" "$CONF"; then
  problem "updater endpoint still contains OWNER/REPO"
else
  ok "updater endpoint is concrete"
fi
if grep -qE '"createUpdaterArtifacts": *true' "$CONF"; then
  ok "updater artifacts are enabled"
else
  problem "createUpdaterArtifacts is not enabled in $CONF"
fi

echo "== versions"
if [[ -n "$TAURI_VERSION" && "$CARGO_VERSION" == "$TAURI_VERSION" ]]; then
  ok "workspace Cargo and desktop package.json both say $TAURI_VERSION"
else
  problem "version mismatch: Cargo ${CARGO_VERSION:-?}, desktop package.json ${TAURI_VERSION:-?}"
fi

echo "== changelog"
if [[ -f CHANGELOG.md ]] && grep -q "$TAURI_VERSION" CHANGELOG.md; then
  ok "CHANGELOG.md mentions $TAURI_VERSION"
else
  problem "CHANGELOG.md has no section for $TAURI_VERSION"
fi

echo "== protocol and built-in plugins"
protocol=$(grep -rn "PROTOCOL_VERSION" crates/dnd-protocol/src/lib.rs | head -1 | grep -oE '[0-9]+' | tail -1)
echo "  info: protocol version ${protocol:-unknown}; built-in plugins: $(ls plugins | tr '\n' ' ')"

if [[ $status -ne 0 ]]; then
  echo "release checks: NOT READY ($missing item(s) to set up)"
elif [[ $missing -gt 0 ]]; then
  echo "release checks: OK for local work, $missing item(s) still missing for a release"
else
  echo "release checks: OK"
fi
exit $status
