# Протокол

Источник истины — крейт `crates/dnd-protocol`; форма JSON закреплена insta-снапшотами в
`crates/dnd-protocol/tests/snapshots/`, TS-типы — там же (`snapshots__typescript_bindings.snap`).
Решения о кодировании — [ADR-0007](adr/0007-protocol-encoding.md).

## Конверт

```json
{ "v": 1, "id": "<uuid>", "ts": 1760000000000, "seq": null,
  "session_id": "<uuid>|null", "sender": "<uuid>|null",
  "type": "intent", "payload": { ... } }
```

`v` ≠ версии получателя → отказ `protocol_mismatch` («обновите клиент/сервер»).

## Сообщения

| Направление     | `type`     | `payload`                                     |
| --------------- | ---------- | --------------------------------------------- |
| клиент → сервер | `intent`   | `{ intent_id, expected_version, intent }`     |
| клиент → сервер | `ping`     | —                                             |
| клиент → сервер | `resync`   | `{ last_seq }`                                |
| сервер → клиент | `events`   | `{ seq, caused_by, events[] }`                |
| сервер → клиент | `snapshot` | `{ seq, campaign, maps, tokens, characters }` |
| сервер → клиент | `rejected` | `{ intent_id, reason: { code, ... } }`        |
| сервер → клиент | `pong`     | —                                             |

## Intents (этап 1)

| `kind`                 | Версия         | Кто может                                        |
| ---------------------- | -------------- | ------------------------------------------------ |
| `update_campaign_meta` | проверяется    | GM, Co-GM                                        |
| `create_map`           | —              | GM, Co-GM                                        |
| `update_map`           | проверяется    | GM, Co-GM                                        |
| `delete_map`           | проверяется    | GM, Co-GM (каскадно удаляет токены)              |
| `apply_fog`            | —              | GM, Co-GM (ADR-0019; версию карты не меняет)     |
| `reset_fog`            | LWW            | GM, Co-GM (ADR-0019)                             |
| бой: `start_combat` …  | см. ADR-0020   | GM, Co-GM; игрок — свой бросок и свой ход        |
| `create_token`         | —              | GM, Co-GM                                        |
| `update_token`         | проверяется    | GM, Co-GM                                        |
| `move_token`           | LWW, не растёт | GM, Co-GM; Player — свои токены на видимой карте |
| `delete_token`         | проверяется    | GM, Co-GM                                        |
| журнал: записи, папки  | см. ADR-0021   | GM, Co-GM; игрок — текст разрешённых записей     |

Spectator не может ничего. Коды отказов: `stale_version`, `missing_expected_version`, `forbidden`,
`not_found`, `already_exists`, `invalid`, `limit_exceeded`, `protocol_mismatch`.

## Идемпотентность и повторы (ADR-0015)

- `intent_id` — ключ идемпотентности. Повтор **того же** intent (тот же `intent_id` и те же данные)
  в окне (последние 10 000 intents или 24 ч) возвращает исходный результат и ничего не применяет.
- Сервер хранит SHA-256 данных intent (`expected_version` + `intent`). Тот же `intent_id` с другими
  данными → `rejected { code: intent_id_reused }`.
- **Любой повтор с изменёнными данными — новый `intent_id`.** Например, после `stale_version` клиент
  перечитывает сущность и отправляет правку с новой `expected_version` под новым id.
- Повторно отправлять неподтверждённый intent после реконнекта нужно с прежним `intent_id` и
  прежними данными.
- `intent_id` и id создаваемых сущностей — UUIDv7. Сервер проверяет только версию и формат UUID;
  временная метка внутри id ничего не значит для правил и прав.
