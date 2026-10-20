# 0009. Десктоп: локальный сервер, IPC и файловый доступ

- Статус: Принято
- Дата: 2026-10-21

## Контекст

ADR-0001 требует, чтобы локальная игра шла через тот же пайплайн intents, что и удалённая. CLAUDE.md §9
требует типизированный IPC, единый `IpcError` и доступ к ФС только к явно выбранным файлам.

## Решение

- `LocalServer` (крейт `dnd-desktop`) — in-process «сервер»: `apply` → запись batch в SQLite → `evolve`.
  Состояние в памяти меняется только после успешной записи. Не зависит от Tauri и тестируется напрямую.
- IPC — tauri-specta: команды `campaign_create/open/close/current`, `get_snapshot`, `submit_intent`,
  `app_info`; событие `GameEventsEmitted(EventBatch)`. `bindings.ts` генерирует тест
  `export_bindings`, CI проверяет, что закоммиченный файл актуален.
- `submit_intent` возвращает `IntentOutcome { accepted { seq } | rejected { reason } }`: отказ ядра — штатный
  исход, а не ошибка. Ошибки инфраструктуры — `IpcError { kind, code, message }`; UI локализует по `code`.
- Фронтенд держит снапшот в TanStack Query и применяет батчи событий (`applyBatch`, зеркало `evolve`);
  при разрыве `seq` — перезапрос снапшота.
- **Кампания = папка** `<Имя>/campaign.sqlite` + `assets/` (так же, как сессия на сервере, ADR-0002).
- **Диалоги выбора папок вызываются из Rust** (`tauri-plugin-dialog`), фронтенд никогда не передаёт пути.
  Поэтому вебвью не получает разрешений `fs`/`dialog` (capability — только `core:default`), а
  `tauri-plugin-fs` из §3 не подключён: ему нечего делать на стороне вебвью. Подключим, если появится
  сценарий, которому он действительно нужен.
- Строгий CSP без `unsafe-inline` в релизе; `devCsp` разрешает inline-стили и HMR Vite только в dev.
- Windows: манифест Common Controls v6 встраивается во все бинарники через `build.rs`, иначе тестовые
  бинарники, линкующие tauri, падают с `STATUS_ENTRYPOINT_NOT_FOUND`.
- Версии фронтенда: React 19.3 (последний в мажоре 19), **TypeScript 6.0** — TypeScript 7 вышел, но
  typescript-eslint пока поддерживает только `<6.1`; переход — когда появится поддержка.
  React Compiler — через `babel-plugin-react-compiler` (нативный вариант в plugin-react 6 — experimental).

## Последствия

- Любой новый сценарий работы с файлами (импорт ассетов, экспорт) — новая команда с диалогом в Rust.
- Для удалённой игры UI не меняется: события придут из `dnd-net` вместо `LocalServer`.
