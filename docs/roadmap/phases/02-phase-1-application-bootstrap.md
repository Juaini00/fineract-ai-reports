# Implementation Steps: Phase 1: Application Bootstrap

Source: `docs-old/implementation-steps.md`

## Phase 1: Application Bootstrap

Goal: start the HTTP server with clean configuration and tracing.

Tasks:

1. Load `.env` using `dotenvy`.
2. Create typed application config.
3. Initialize tracing/logging.
4. Start Axum HTTP server.
5. Add shared application state.
6. Add graceful shutdown.

Required config groups:

```text
Application config
App database config
Fineract database config
LLM provider config
Auth config
Query/report guard config
Redis config, optional
Vector config
```

Minimum application config:

```text
APP_ENV
APP_HOST
APP_PORT
RUST_LOG
```

Current local port:

```text
APP_PORT=3007
```

Validation:

```bash
cargo run
```

Expected result:

```text
server starts on APP_HOST:APP_PORT
logs are visible
```

Startup logs must show:

```text
application environment
listening address and port
health URL
ready URL
app database readiness
fineract database readiness
pgvector readiness
redis readiness
```

Current status:

```text
DONE
```
