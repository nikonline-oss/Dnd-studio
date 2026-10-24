# dnd-plugin

Пакеты плагинов `.dndplugin`: безопасное чтение, валидация манифеста, правил системного плагина,
схемы и раскладки листа, компендиев и ассетов; совместимость с приложением и разрешение зависимостей.
Формат — [`docs/plugin-format.md`](../../docs/plugin-format.md).

**Разрешённые зависимости (внутренние):** `dnd-protocol`, `dnd-expr` (сейчас не используются).

**Этап реализации:** 3 — Плагины (3a: формат и валидация; подписи — этап 5).

## API

- `read_package(bytes, PackageLimits)` / `read_package_file(path, PackageLimits)` → `PluginPackage`
  (`sha256` всего файла, `manifest`, `system: Option<SystemPackage>`, `compendia`, `assets`).
- `PluginPackage::to_dto()` → `InstalledPluginDto` (то, что получит ядро; волна 2 перенесёт DTO
  в `dnd-protocol`).
- `check_app_compat(manifest, app)`, `check_install(candidate, installed, app)`,
  `resolve(manifests, app)` → `Resolution { active, deactivated }`.
- `check_data(schema, data)` — эталонная проверка данных листа по подмножеству JSON Schema
  (ядро реализует те же правила у себя: оно не зависит от этого крейта).
- Ошибки — `PluginError`, стабильные коды `code()` для UI.

## Чего крейт не делает

- **Не проверяет синтаксис выражений и костей** (`computed`, `value`/`max` ресурсов, `initiative`,
  `rolls[].dice`): это делает ядро через `dnd-expr`/`dnd-dice` при применении правил (волна 2).
  Здесь только лимиты длины (1024 / 256 символов).
- Не пишет на диск и не знает о кампании: копирование пакета в кампанию по SHA-256 — забота
  вызывающего.
- Не проверяет подписи: на этапе 3a каждый локальный пакет — «неподписанный».

## Зависимости (CLAUDE.md §3, §14)

- `jsonschema` (из §3) — без фич по умолчанию: никакого разрешения удалённых и файловых `$ref`,
  никакого TLS. Мета-схемы встроены в крейт (`schemas/`).
- `semver` — сверх §3. Обоснование: версии плагинов, `app_compat` и диапазоны зависимостей —
  semver по определению (§8); `semver` — де-факто стандарт (на нём работает Cargo, крейт от
  dtolnay, уже есть в `Cargo.lock` транзитивно), писать свой разбор диапазонов — источник ошибок.
- `zip` (deflate), `sha2`, `serde_json`, `thiserror` — уже в workspace.
