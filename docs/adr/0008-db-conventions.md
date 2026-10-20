# 0008. Соглашения `dnd-db`

- Статус: Принято. Дополнено 2026-10-21 (покрытие запросов, append-only со ссылками).
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

## Дополнение (2026-10-21): покрытие запросов и append-only со ссылками

- **Каждый runtime-запрос покрыт тестом на реальной SQLite** (файл во временной папке или
  `:memory:`). Тест `query_inventory_matches_adr_0008` (`crates/dnd-db/tests/queries.rs`) считает
  вхождения `sqlx::query` в исходниках: добавили запрос без теста и без строки ниже — CI падает.
- `build.rs` в `dnd-db` пересобирает крейт при изменении `migrations/`: `sqlx::migrate!` встраивает
  миграции при компиляции, а cargo сам эту папку не отслеживает.
- Append-only таблицы со ссылкой `ON DELETE SET NULL` (`chat_messages.sender_user_id`,
  `dice_rolls.user_id`, `audit_log.session_id`): триггер запрета `UPDATE` пропускает ровно одно
  изменение — обнуление этой ссылки при удалении родителя; любые другие правки запрещены
  (миграции campaign 0002 и server 0002). До исправления удалить пользователя с историей чата
  или сессию с аудитом было невозможно — нашёл тест `server_db_opens_migrates_and_enforces_constraints`.

| Запрос                                    | Где                       | Тест                                                                           |
| ----------------------------------------- | ------------------------- | ------------------------------------------------------------------------------ |
| `PRAGMA quick_check`                      | `connect::quick_check`    | `backup.rs`: corrupted / non-database file                                     |
| `sqlite_schema` EXISTS `_sqlx_migrations` | `has_pending_migrations`  | `schema.rs::campaign_schema`, `backup.rs::pending_migrations_trigger_a_backup` |
| `SELECT version FROM _sqlx_migrations`    | `has_pending_migrations`  | `backup.rs::campaign_from_previous_schema_is_backed_up_and_upgraded`           |
| `VACUUM INTO ?`                           | `backup::create_backup`   | `backup.rs::backups_are_rotated`, restore test                                 |
| `INSERT INTO users`                       | `CampaignDb::create`      | `projection.rs::create_and_reopen_keeps_meta_and_local_user`                   |
| `PRAGMA wal_checkpoint(TRUNCATE)`         | `CampaignDb::checkpoint`  | `queries.rs::checkpoint_empties_the_wal`                                       |
| `SELECT … FROM users WHERE id`            | `local_user`              | `projection.rs::create_and_reopen_keeps_meta_and_local_user`                   |
| `SELECT COALESCE(MAX(seq), 0)`            | `last_seq`                | `queries.rs::every_event_kind_is_projected`, projection proptest               |
| `SELECT intent_id, MAX(seq) … GROUP BY`   | `recent_intents`          | `queries.rs::recent_intents_are_ordered_limited_and_grouped`                   |
| `SELECT * FROM maps` / `tokens`           | `load_state`              | `queries.rs::every_event_kind_is_projected`, projection proptest               |
| `SELECT … FROM characters`                | `load_state`              | `projection.rs::character_owners_are_loaded`                                   |
| `INSERT INTO event_log`                   | `project`                 | `queries.rs::every_event_kind_is_projected`, `project_is_atomic`               |
| `DELETE FROM maps` (+ FK cascade)         | `project_event`           | `queries.rs::every_event_kind_is_projected`                                    |
| `UPDATE tokens SET x, y`                  | `project_event`           | `queries.rs::every_event_kind_is_projected` (включая неизвестный токен)        |
| `DELETE FROM tokens`                      | `project_event`           | `queries.rs::every_event_kind_is_projected`                                    |
| `INSERT … ON CONFLICT` `campaign_meta`    | `set_meta`                | `queries.rs::every_event_kind_is_projected`                                    |
| `SELECT value FROM campaign_meta`         | `get_meta`                | `queries.rs::every_event_kind_is_projected`                                    |
| `INSERT … ON CONFLICT` `maps`             | `upsert_map`              | `queries.rs::every_event_kind_is_projected` (insert и update)                  |
| `INSERT … ON CONFLICT` `tokens`           | `upsert_token`            | `queries.rs::every_event_kind_is_projected` (insert и update)                  |
| миграции `server` (`open_server_db`)      | `connect::open_server_db` | `queries.rs::server_db_opens_migrates_and_enforces_constraints`                |
