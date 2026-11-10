# DndStudio

Open-source экосистема для настольных ролевых игр, не привязанная к конкретной системе правил:
десктоп-клиент (Tauri), self-hosted сервер сессий и каталог плагинов.

Статус: **исправление архитектурных рисков перед завершением MVP**. Первые задачи —
приватность персонажей и рабочий RNG; затем исполнение правил, контракты, синхронизация
и таймеры. Каталог и выпуск следуют после обязательных архитектурных исправлений.
Актуальная очередь — [дорожная карта](docs/roadmap.md), ревизия 2026-11-08.

- Задачи, этапы и критерии готовности: [docs/roadmap.md](docs/roadmap.md)
- Доска работ и просмотр документации: `pnpm workboard` → http://127.0.0.1:8090
  ([tools/workboard](tools/workboard/README.md))
- Порядок работы и проверок: [AGENTS.md](AGENTS.md)
- Продукт и архитектура: [CLAUDE.md](CLAUDE.md)
- Архитектурные решения: [docs/adr](docs/adr/README.md)
- Протокол: [docs/protocol.md](docs/protocol.md)
- Релизы и автообновление: [docs/releasing.md](docs/releasing.md)

## Разработка

Требования: Rust (версия из `rust-toolchain.toml`), Node ≥ 24, pnpm (через `corepack enable pnpm`),
системные зависимости Tauri 2 (https://v2.tauri.app/start/prerequisites/). На Windows нужен
**WebView2 Runtime** — в редакциях LTSC/IoT он не предустановлен (установщик приложения ставит его сам).

Запуск десктопа: `pnpm --filter @dndstudio/desktop tauri dev`.

```bash
bash tools/ci/local-ci.sh
```

## Лицензия

MIT OR Apache-2.0 — на ваш выбор, см. [LICENSE-MIT](LICENSE-MIT) и [LICENSE-APACHE](LICENSE-APACHE).
