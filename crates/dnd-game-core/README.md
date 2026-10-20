# dnd-game-core

Deterministic game core: state, apply(intent) -> events, role permissions. No I/O.

**Разрешённые зависимости (внутренние):** только `dnd-protocol`, `dnd-dice`, `dnd-expr` (ADR-0001). Никакого I/O, async, Tauri, sqlx. Проверяется `tools/ci/check-crate-boundaries.sh`.

**Этап реализации:** 1 — Фундамент (Campaign/Map/Token)
