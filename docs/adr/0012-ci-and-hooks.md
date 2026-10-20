# 0012. CI, релизы и pre-commit

- Статус: Принято
- Дата: 2026-10-21

## Решение

- `ci.yml`: Rust (fmt, clippy `-D warnings`, test) на Ubuntu/Windows/macOS, проверка границ крейтов,
  проверка, что сгенерированные `bindings.ts` закоммичены; фронтенд — prettier, eslint, tsc, vitest и
  продакшн-сборка Vite. Локально то же самое — `tools/ci/local-ci.sh` (`.ps1` на Windows).
- `release.yml`: по тегу `v*` tauri-action собирает установщики для Windows, macOS (arm64 и x64
  отдельно) и Linux (Ubuntu 22.04 ради совместимости glibc) и прикладывает их к **черновику** релиза.
- Pre-commit — **lefthook** (dev-зависимость в корне, ставится скриптом `prepare` при `pnpm install`):
  кроссплатформенный, без Python, один бинарь. Хуки: rustfmt, clippy, prettier, eslint, tsc.
  Postinstall-скрипт самого lefthook запрещён (`allowBuilds: lefthook: false`) — хуки ставит `prepare`.
- Версии actions — последние мажоры на дату решения (checkout@v7, setup-node@v7, pnpm/action-setup@v6,
  rust-cache@v2, tauri-action@v1); тулчейн Rust берётся из `rust-toolchain.toml`.
