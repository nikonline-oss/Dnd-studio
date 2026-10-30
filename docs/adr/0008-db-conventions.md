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

| Запрос                                                               | Где                                | Тест                                                                                            |
| -------------------------------------------------------------------- | ---------------------------------- | ----------------------------------------------------------------------------------------------- |
| `INSERT pending_ops`                                                 | `insert_pending`                   | `queries.rs::offline_intents_are_ordered_and_conflicts_can_be_rebased_or_discarded`             |
| `SELECT pending_ops ORDER BY id`                                     | `pending_ops`                      | `queries.rs::offline_intents_are_ordered_and_conflicts_can_be_rebased_or_discarded`             |
| `UPDATE pending_ops status/conflict`                                 | `mark_pending_conflict`            | `queries.rs::offline_intents_are_ordered_and_conflicts_can_be_rebased_or_discarded`             |
| `UPDATE pending_ops intent/base_version`                             | `rebase_pending_op`                | `queries.rs::offline_intents_are_ordered_and_conflicts_can_be_rebased_or_discarded`             |
| `DELETE pending_ops`                                                 | `delete_pending_op`                | `queries.rs::offline_intents_are_ordered_and_conflicts_can_be_rebased_or_discarded`             |
| `SELECT event_log WHERE intent_id`                                   | `accepted_intent`                  | `engine.rs::a_durable_retry_after_the_cache_expires_never_rewinds_a_token`                      |
| `UPDATE pending_ops SET projected`                                   | `mark_projected`                   | desktop `remote.rs::host_join_and_play_through_the_server`, `backup.rs` migration test          |
| `PRAGMA quick_check`                                                 | `connect::quick_check`             | `backup.rs`: corrupted / non-database file                                                      |
| `sqlite_schema` EXISTS `_sqlx_migrations`                            | `has_pending_migrations`           | `schema.rs::campaign_schema`, `backup.rs::pending_migrations_trigger_a_backup`                  |
| `SELECT version FROM _sqlx_migrations`                               | `has_pending_migrations`           | `backup.rs::campaign_from_previous_schema_is_backed_up_and_upgraded`                            |
| `VACUUM INTO ?`                                                      | `backup::vacuum_into`              | `backup.rs::backups_are_rotated`, `archive.rs::export_and_import_round_trip`                    |
| `INSERT INTO users`                                                  | `CampaignDb::create`               | `projection.rs::create_and_reopen_keeps_meta_and_local_user`                                    |
| `PRAGMA wal_checkpoint(TRUNCATE)`                                    | `CampaignDb::checkpoint`           | `queries.rs::checkpoint_empties_the_wal`                                                        |
| `SELECT … FROM users WHERE id`                                       | `local_user`                       | `projection.rs::create_and_reopen_keeps_meta_and_local_user`                                    |
| `SELECT COALESCE(MAX(seq), 0)`                                       | `last_seq`                         | `queries.rs::every_event_kind_is_projected`, projection proptest                                |
| `SELECT intent_id, MAX(seq) … GROUP BY`                              | `recent_intents`                   | `queries.rs::recent_intents_are_ordered_limited_and_grouped`                                    |
| `SELECT * FROM maps` / `tokens`                                      | `load_state`                       | `queries.rs::every_event_kind_is_projected`, projection proptest                                |
| `SELECT … FROM characters`                                           | `load_state`                       | `projection.rs::character_owners_are_loaded`                                                    |
| `SELECT … FROM users`                                                | `load_state`                       | `queries.rs::characters_and_users_are_projected_and_loaded`                                     |
| `INSERT … ON CONFLICT` `characters`                                  | `upsert_character`                 | `queries.rs::characters_and_users_are_projected_and_loaded` (insert и update)                   |
| `SELECT … FROM journal_entries`                                      | `load_state`                       | `queries.rs::journal_entries_are_projected_and_loaded`                                          |
| `INSERT … ON CONFLICT` `journal_entries`                             | `upsert_journal_entry`             | `queries.rs::journal_entries_are_projected_and_loaded` (`created_at` не меняется)               |
| `DELETE FROM journal_entries`                                        | `project_event`                    | `queries.rs::journal_entries_are_projected_and_loaded`                                          |
| `INSERT INTO chat_messages`                                          | `insert_chat_message`              | `queries.rs::chat_messages_and_rolls_are_stored_and_paged`                                      |
| `INSERT INTO dice_rolls`                                             | `insert_chat_message`              | `queries.rs::chat_messages_and_rolls_are_stored_and_paged`                                      |
| `SELECT … chat_messages JOIN dice_rolls`                             | `read_chat_history`                | `queries.rs::history_filters_before_limit_for_every_role` (роли, страницы)                      |
| `INSERT INTO assets`                                                 | `insert_asset`                     | `queries.rs::assets_are_registered_and_loaded` (уникальность `type`+`usage`+`sha256`, ADR-0027) |
| `SELECT … FROM assets`                                               | `load_state`                       | `queries.rs::assets_are_registered_and_loaded`                                                  |
| `INSERT INTO event_log`                                              | `project`                          | `queries.rs::every_event_kind_is_projected`, `project_is_atomic`                                |
| `DELETE FROM maps` (+ FK cascade)                                    | `project_event`                    | `queries.rs::every_event_kind_is_projected`                                                     |
| `UPDATE tokens SET x, y`                                             | `project_event`                    | `queries.rs::every_event_kind_is_projected` (включая неизвестный токен)                         |
| `DELETE FROM tokens`                                                 | `project_event`                    | `queries.rs::every_event_kind_is_projected`                                                     |
| `INSERT … ON CONFLICT` `campaign_meta`                               | `set_meta`                         | `queries.rs::every_event_kind_is_projected`                                                     |
| `SELECT value FROM campaign_meta`                                    | `get_meta`                         | `queries.rs::every_event_kind_is_projected`                                                     |
| `INSERT … ON CONFLICT` `maps`                                        | `upsert_map`                       | `queries.rs::every_event_kind_is_projected` (insert и update)                                   |
| `UPDATE maps SET fog = json_insert(…)`                               | `project_event`                    | `queries.rs::fog_is_appended_reset_and_loaded` (порядок, неизвестная карта)                     |
| `UPDATE maps SET fog = ?`                                            | `project_event`                    | `queries.rs::fog_is_appended_reset_and_loaded` (сброс; переименование не трогает)               |
| `INSERT … ON CONFLICT` `combats`                                     | `project_event`                    | `queries.rs::combats_and_combatants_are_projected_and_loaded` (один бой на карту)               |
| `DELETE FROM combats` (+ FK cascade)                                 | `project_event`                    | `queries.rs::combats_and_combatants_are_projected_and_loaded`                                   |
| `INSERT … ON CONFLICT` `combatants`                                  | `project_event`                    | `queries.rs::combats_and_combatants_are_projected_and_loaded` (insert и update)                 |
| `DELETE FROM combatants`                                             | `project_event`                    | `queries.rs::combats_and_combatants_are_projected_and_loaded`                                   |
| `SELECT … FROM combats` / `combatants`                               | `load_state`                       | `queries.rs::combats_and_combatants_are_projected_and_loaded` (SET NULL токена)                 |
| `INSERT … ON CONFLICT` `journal_folders`                             | `project_event`                    | `queries.rs::journal_folders_are_projected_loaded_and_cascade` (переименование)                 |
| `DELETE FROM journal_folders` (+ cascade)                            | `project_event`                    | `queries.rs::journal_folders_are_projected_loaded_and_cascade` (записи внутри)                  |
| `SELECT … FROM journal_folders`                                      | `load_state`                       | `queries.rs::journal_folders_are_projected_loaded_and_cascade`                                  |
| `INSERT … ON CONFLICT` `tokens`                                      | `upsert_token`                     | `queries.rs::every_event_kind_is_projected` (insert и update)                                   |
| `INSERT … ON CONFLICT` `users`                                       | `project_event`                    | `queries.rs::joined_users_are_projected_and_redacted_events_refused`                            |
| `INSERT … ON CONFLICT` `installed_plugins`                           | `project_event`                    | `queries.rs::plugins_and_the_system_are_projected_and_loaded` (замена версии)                   |
| `DELETE FROM installed_plugins`                                      | `project_event`                    | `queries.rs::plugins_and_the_system_are_projected_and_loaded`                                   |
| `SELECT dto FROM installed_plugins`                                  | `load_state`                       | `queries.rs::plugins_and_the_system_are_projected_and_loaded`                                   |
| `INSERT … ON CONFLICT` `boards`                                      | `project_event`                    | `queries.rs::boards_are_projected_loaded_and_cascade` (переименование)                          |
| `DELETE FROM boards` (+ cascade)                                     | `project_event`                    | `queries.rs::boards_are_projected_loaded_and_cascade` (узлы и рёбра)                            |
| `INSERT … ON CONFLICT` `board_nodes`                                 | `upsert_board_node`                | `queries.rs::boards_are_projected_loaded_and_cascade`                                           |
| `UPDATE board_nodes SET dto = json_set`                              | `project_event`                    | `queries.rs::boards_are_projected_loaded_and_cascade` (перемещение)                             |
| `DELETE FROM board_nodes` (+ cascade)                                | `project_event`                    | `queries.rs::boards_are_projected_loaded_and_cascade` (рёбра узла)                              |
| `INSERT … ON CONFLICT` `board_edges`                                 | `project_event`                    | `queries.rs::boards_are_projected_loaded_and_cascade`                                           |
| `DELETE FROM board_edges`                                            | `project_event`                    | `queries.rs::boards_are_projected_loaded_and_cascade`                                           |
| `SELECT dto FROM boards` / `board_nodes` / `board_edges`             | `load_state`                       | `queries.rs::boards_are_projected_loaded_and_cascade`                                           |
| `INSERT … ON CONFLICT` `zones`                                       | `project_event`                    | `queries.rs::zones_are_projected_loaded_and_go_with_their_map`                                  |
| `DELETE FROM zones` (+ cascade с картой)                             | `project_event`                    | `queries.rs::zones_are_projected_loaded_and_go_with_their_map`                                  |
| `SELECT dto FROM zones`                                              | `load_state`                       | `queries.rs::zones_are_projected_loaded_and_go_with_their_map`                                  |
| `INSERT … ON CONFLICT` `triggers`                                    | `project_event`                    | `queries.rs::triggers_are_projected_loaded_and_deleted` (срабатывание `once`)                   |
| `DELETE FROM triggers`                                               | `project_event`                    | `queries.rs::triggers_are_projected_loaded_and_deleted`                                         |
| `SELECT dto FROM triggers`                                           | `load_state`                       | `queries.rs::triggers_are_projected_loaded_and_deleted`                                         |
| `INSERT … ON CONFLICT` `library_folders`                             | `project_event`                    | queries.rs::library_folders_and_sounds_are_projected                                            |
| `DELETE FROM library_folders` (+ cascade, `assets.folder_id` → NULL) | `project_event`                    | queries.rs::library_folders_and_sounds_are_projected                                            |
| `SELECT dto FROM library_folders`                                    | `load_state`                       | queries.rs::library_folders_and_sounds_are_projected                                            |
| `UPDATE assets SET folder_id, original_name`                         | `project_event`                    | queries.rs::library_folders_and_sounds_are_projected                                            |
| миграции `server` (`open_server_db`)                                 | `connect::open_server_db`          | `queries.rs::server_db_opens_migrates_and_enforces_constraints`                                 |
| `INSERT INTO sessions`                                               | `ServerDb::insert_session`         | `server_index.rs::sessions_are_stored_listed_and_change_state` (уникальность id)                |
| `SELECT … FROM sessions WHERE id`                                    | `ServerDb::session`                | `server_index.rs::sessions_are_stored_listed_and_change_state`                                  |
| `SELECT … FROM sessions WHERE state`                                 | `ServerDb::sessions_in`            | `server_index.rs::sessions_are_stored_listed_and_change_state` (порядок)                        |
| `SELECT COUNT(*) FROM sessions`                                      | `ServerDb::count_sessions_in`      | `server_index.rs::sessions_are_stored_listed_and_change_state`                                  |
| `UPDATE sessions SET state`                                          | `ServerDb::set_session_state`      | `server_index.rs::sessions_are_stored_listed_and_change_state`                                  |
| `UPDATE sessions SET last_activity`                                  | `ServerDb::touch_session`          | `server_index.rs::sessions_are_stored_listed_and_change_state` (только вперёд)                  |
| `INSERT INTO participants`                                           | `ServerDb::insert_participant`     | `server_index.rs::participants_are_found_by_token_within_their_session`                         |
| `SELECT … FROM participants WHERE token`                             | `ServerDb::participant_by_token`   | `server_index.rs::participants_are_found_by_token_within_their_session` (изоляция)              |
| `UPDATE participants SET status`                                     | `ServerDb::set_participant_status` | `server_index.rs::participants_are_found_by_token_within_their_session`                         |
| `SELECT COUNT(*) FROM participants`                                  | `ServerDb::count_participants`     | `server_index.rs::participants_are_found_by_token_within_their_session`                         |
| `INSERT INTO audit_log`                                              | `ServerDb::audit`                  | `server_index.rs::audit_is_appended`                                                            |

## Дополнение (2026-10-21): имена авторов в истории

- `chat_messages.sender_name` и `dice_rolls.user_name` хранят имя автора **на момент записи**
  (миграция campaign 0003, существующие строки заполнены из `users`). Поля неизменяемы, как и вся
  строка. После удаления пользователя ссылка становится `NULL`, а имя остаётся.
- UI чата и бросков (этап 2a) показывает это имя, когда `sender_user_id`/`user_id` равен `NULL`;
  DTO сообщений и бросков в протоколе обязаны его нести.
- Имена колонок повторяют пары ссылок: `sender_user_id` → `sender_name`, `user_id` → `user_name`.
