# dnd-session-server

Self-hosted сервер сессий: axum + `dnd-game-core` + SQLite (CLAUDE.md §1, §7; ADR-0022).

**Разрешённые зависимости (внутренние):** `dnd-protocol`, `dnd-game-core`, `dnd-db`, `dnd-plugin`, `dnd-assets`.

## Запуск

Docker (из корня репозитория):

```bash
cp .env.example .env
```

Задайте в `.env` длинный `DND_HOST_TOKEN`, затем:

```bash
docker compose up -d
```

Данные лежат в томе `dnd-data` (`/data`). Образ — distroless, процесс работает не от root;
`dnd-session-server healthcheck` проверяет `/api/health` (это и есть HEALTHCHECK образа).

Без Docker:

```bash
DND_HOST_TOKEN=change-me-to-a-long-secret DND_DATA_DIR=./data cargo run -p dnd-session-server
```

| Переменная                                               | По умолчанию        |
| -------------------------------------------------------- | ------------------- |
| `DND_HOST_TOKEN` или `DND_HOST_TOKEN_FILE`               | обязательно, ≥ 16   |
| `DND_DATA_DIR`                                           | `/data`             |
| `DND_BIND`                                               | `0.0.0.0:8080`      |
| `DND_PUBLIC_URL`                                         | из запроса (`Host`) |
| `DND_MAX_SESSIONS` / `DND_MAX_PARTICIPANTS`              | 100 / 20            |
| `DND_SESSION_TTL_SECS`                                   | 3600                |
| `DND_CONNECTIONS_PER_MINUTE` / `DND_MESSAGES_PER_MINUTE` | 10 / 100            |
| `DND_MAX_UPLOAD_MB`                                      | 1024                |
| `RUST_LOG`                                               | `info`              |

Работает и за чужим reverse proxy без TLS. Лимит подключений считается по IP соединения, то есть
за прокси — по IP прокси.

## Данные

- `server.sqlite` — сессии, участники, аудит. Секреты хранятся только как хэши.
- `sessions/{id}/campaign.sqlite` и `assets/` — кампания сессии, та же схема, что у десктопа.
- `tmp/` — загрузки и экспорты в процессе.

## API

| Запрос                                     | Кто                                              |
| ------------------------------------------ | ------------------------------------------------ |
| `GET /api/health`                          | все                                              |
| `POST /api/sessions` (multipart `archive`) | host token                                       |
| `GET /api/sessions/{id}`                   | все (имя и состояние — для экрана подключения)   |
| `POST /api/sessions/{id}/join`             | `{ gm_key }` или `{ access_code, name }` → токен |
| `GET /ws/sessions/{id}`                    | токен участника                                  |
| `GET /api/sessions/{id}/assets/{sha256}`   | токен участника; игроку — только видимое         |
| `POST /api/sessions/{id}/stop`             | GM                                               |
| `GET /api/sessions/{id}/export`            | GM                                               |

Токены передаются как `Authorization: Bearer …`.
