# 00 — Setup

**Phase covered:** Phase 0–4 (baseline + bootstrap + DB + migrations).

## Test status

✅ Passed on 2026-06-28.

- Redis check returned `PONG`.
- PostgreSQL `pg_extension` contains `vector`.
- App was already running on `127.0.0.1:3007` and answered health/readiness scenarios.

## 1. Environment

Required `.env` keys (defaults work for local dev):

```env
APP_HOST=127.0.0.1
APP_PORT=3007
APP_ENV=local
RUST_LOG=info,sqlx=warn

APP_DATABASE_URL=postgres://root:password@127.0.0.1:5432/ai_reports
APP_DATABASE_MIGRATE_ON_STARTUP=true

FINERACT_DATABASE_URL=postgres://<user>:<password>@<host>:<port>/<fineract_db>

AUTH_BOOTSTRAP_ADMIN_TOKEN=local-admin-token
API_KEY_PREFIX=air_test

REDIS_ENABLED=true
REDIS_URL=redis://127.0.0.1:6380/0

VOYAGEAI_API_KEY=<voyage_key>          # optional; required for vector path
CATALOG_SYNC_ON_STARTUP=true           # optional; populates knowledge_index
```

## 2. Infrastructure

```bash
# Redis on host port 6380 -> container 6379
docker compose up -d redis
docker compose exec -T redis redis-cli ping     # expect: PONG

# App DB + pgvector
PGPASSWORD=password psql -h 127.0.0.1 -p 5432 -U root -d ai_reports \
  -c "SELECT extname, extversion FROM pg_extension WHERE extname = 'vector';"

# Migrations (only if APP_DATABASE_MIGRATE_ON_STARTUP=false)
sqlx migrate run --database-url "postgres://root:password@127.0.0.1:5432/ai_reports"
```

## 3. Run

```bash
cargo run -p app
```

Expected startup log lines:

```text
environment=local listening=127.0.0.1:3007
health URL=...
ready URL=...
app_database=ok fineract_database=ok pgvector=ok redis=ok
```

## 4. Bootstrap admin token

Set the Postman variable `LOCAL_ADMIN_TOKEN` (or shell env) to the value of
`AUTH_BOOTSTRAP_ADMIN_TOKEN`. This token is used **only** to create the first
API key in `02-auth-api-keys.md`; all subsequent requests use the API key.

## Failure modes

| Trigger | Symptom |
| --- | --- |
| Redis not running | `/ready` returns `redis: error`, chat job SSE falls back to one-shot snapshot |
| `pgvector` extension missing | startup fails on readiness; install via `CREATE EXTENSION vector;` |
| Fineract URL wrong | `/ready` returns `fineract_database: error`; report execution fails downstream |
