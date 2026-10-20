#!/usr/bin/env bash
# Enforces internal dependency boundaries from CLAUDE.md §4 / ADR-0001.
# For each crate: the set of direct normal dependencies starting with `dnd-` must be a subset of the allow-list.
set -euo pipefail

declare -A ALLOWED=(
  [dnd-protocol]=""
  [dnd-expr]=""
  [dnd-dice]="dnd-expr"
  [dnd-game-core]="dnd-protocol dnd-dice dnd-expr"
  [dnd-db]="dnd-protocol dnd-game-core"
  [dnd-plugin]="dnd-protocol dnd-expr"
  [dnd-assets]="dnd-protocol"
  [dnd-net]="dnd-protocol"
)

# Crates that must stay free of I/O, async runtimes and Tauri.
PURE_CRATES="dnd-protocol dnd-game-core dnd-dice dnd-expr"
FORBIDDEN_IN_PURE="tokio sqlx tauri axum reqwest object_store"

status=0
for crate in "${!ALLOWED[@]}"; do
  deps=$(cargo tree -p "$crate" -e normal --depth 1 --prefix none --format '{p}' 2>/dev/null \
    | tail -n +2 | awk '{print $1}' | sort -u)
  for dep in $deps; do
    if [[ $dep == dnd-* ]] && [[ " ${ALLOWED[$crate]} " != *" $dep "* ]]; then
      echo "boundary violation: $crate -> $dep (allowed: ${ALLOWED[$crate]:-none})"
      status=1
    fi
  done
  if [[ " $PURE_CRATES " == *" $crate "* ]]; then
    all=$(cargo tree -p "$crate" -e normal --prefix none --format '{p}' 2>/dev/null | awk '{print $1}' | sort -u)
    for bad in $FORBIDDEN_IN_PURE; do
      if grep -qx "$bad" <<<"$all"; then
        echo "boundary violation: pure crate $crate transitively depends on $bad"
        status=1
      fi
    done
  fi
done

if [[ $status -eq 0 ]]; then echo "crate boundaries OK"; fi
exit $status
