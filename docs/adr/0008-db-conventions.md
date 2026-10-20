# 0008. Соглашения `dnd-db`

- Статус: Принято
- Дата: 2026-10-21

## Контекст

ADR-0002 выбрал SQLite. Нужно зафиксировать, как именно пишем схему и запросы, чтобы клиент и сервер
не разошлись и CI не зависел от живой БД.

## Решение

- **Только runtime-запросы sqlx** (`query`, `query_as`, `query_scalar`); макросы `query!` не используем —
  не нужен `DATABASE_URL` и `.sqlx`-кэш в CI. Корректность SQL покрывают интеграционные тесты, форму
  схемы — insta-снапшот `sqlite_schema`.
- Миграции встраиваются `sqlx::migrate!`: `migrations/campaign` (файл кампании = файл сессии) и
  `migrations/server` (`server.sqlite`). Только вперёд. Файл от более новой версии приложения →
  `DbError::SchemaTooNew`.
- Таблицы `STRICT`; id — UUID-строки; время — unix-мс; булевы — `INTEGER 0/1` с `CHECK`; JSON — `TEXT`
  с `CHECK (json_valid(...))`; enum-значения — snake_case-имена из `dnd-protocol` (общий serde-кодек).
- У изменяемых сущностей `version INTEGER NOT NULL DEFAULT 1`. В `installed_plugins` колонка `version` —
  semver плагина (§6), версия строки — `row_version`.
- `chat_messages`, `dice_rolls`, `event_log`, `audit_log` — append-only, защищены триггерами.
- Метаданные кампании — ключи `campaign_meta`: `campaign.name`, `campaign.description`,
  `campaign.version`, `local.user_id`.
- Проекция: `event_log` (строка на событие) и таблицы обновляются в одной транзакции.
- Открытие: `PRAGMA quick_check` (read-only соединение) → бэкап, если есть неприменённые миграции →
  миграции. Бэкапы — `VACUUM INTO` в `<имя>.backups/backup-<unix_ms>-<причина>.sqlite`, хранится 5.
  Восстановление переименовывает повреждённый файл в `*.corrupt` (для диагностики), не удаляет.

## Последствия

- Опечатка в SQL ловится тестами, а не компилятором.
- Время передаётся в `dnd-db` параметром (`now_ms`), чтобы тесты были детерминированными.

## Рассмотренные альтернативы

- `query!` с офлайн-кэшем: проверка на этапе компиляции, но лишний артефакт в репозитории и
  обязательная перегенерация при каждом изменении схемы.
