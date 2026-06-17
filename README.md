# AI Reporting Service

Rust-based AI Reporting Service for Apache Fineract data.

The service reads from an existing Fineract PostgreSQL database through a read-only connection or replica. It does not modify Apache Fineract, does not add Fineract plugins, and does not change the Fineract database schema.

## Goals

- Let clients ask reporting questions in natural language.
- Authenticate clients with application-managed API keys.
- Execute only approved, read-only reporting capabilities.
- Use Rust as the main validator, planner, policy enforcer, and executor.
- Use DeepSeek for AI planning fallback and response formatting, not arbitrary SQL execution.
- Use PostgreSQL and `pgvector` for durable app data and knowledge retrieval.
- Use Redis only for live progress, coordination, and future SSE job state.

## Current Status

Implemented:

- Rust workspace with two crates: `crates/app` and `crates/core`.
- Typed configuration loading from `.env`.
- Tracing and HTTP server bootstrap.
- App PostgreSQL pool.
- Fineract PostgreSQL pool.
- Redis readiness integration.
- `pgvector` readiness check.
- `GET /health`.
- `GET /ready`.
- App DB migrations through `sqlx`.
- API key creation with bootstrap admin token.
- API key hashing and one-time raw key response.
- API key authentication extractor.
- `GET /auth/me`.
- Consistent API response envelope.
- Global validated JSON extractor using `validator`.

Next major work:

- Capability authorization helpers.
- Chat/session/job migrations.
- Chat job API foundation.
- SSE progress streaming.
- Approved reporting capability catalog.
- Query planning, validation, execution, and audit logs.

## Architecture

The workspace intentionally has only two crates for now:

```text
ai_report/
  Cargo.toml
  crates/
    app/      # binary entrypoint
    core/     # application logic
  docs/
  migrations/
  docker-compose.yml
```

Rules:

- Root `Cargo.toml` is workspace-only and must not contain `[package]`.
- `crates/app` is only the binary launcher.
- `crates/core` owns config, tracing, DB pools, HTTP routes, auth, and future reporting logic.
- `crates/app` depends on `crates/core` using the alias `app_core` because `core` conflicts with Rust's built-in `core` crate in macros.
- Keep the route -> service -> repository -> database boundary.
- Do not put `sqlx` calls directly in route handlers or services.

## Requirements

- Rust stable with edition 2024 support.
- PostgreSQL with an app database named `ai_reports`.
- `pgvector` extension enabled in the app database.
- Read-only or replica PostgreSQL connection to the Fineract database.
- Docker Compose for Redis.
- `sqlx-cli` if running migrations manually.

## Local Setup

Create or update `.env` with the required values:

```env
APP_ENV=local
APP_HOST=127.0.0.1
APP_PORT=3007
RUST_LOG=info

APP_DATABASE_URL=postgres://root:password@127.0.0.1:5432/ai_reports
APP_DATABASE_MIGRATE_ON_STARTUP=true

FINERACT_DATABASE_URL=postgres://root:password@127.0.0.1:5432/fineract_default

REDIS_ENABLED=true
REDIS_URL=redis://127.0.0.1:6380/0

AUTH_BOOTSTRAP_ADMIN_TOKEN=local-admin-token
API_KEY_PREFIX=air_test
API_KEY_DEFAULT_EXPIRATION_DAYS=0

QUERY_DEFAULT_TIMEOUT_MS=3000
```

Start Redis:

```bash
docker compose up -d redis
```

Check Redis:

```bash
docker compose exec -T redis redis-cli ping
```

Run migrations manually if startup migrations are disabled:

```bash
sqlx migrate run --database-url "postgres://root:password@127.0.0.1:5432/ai_reports"
```

Run the app:

```bash
cargo run -p app
```

The local service listens on:

```text
http://127.0.0.1:3007
```

## Development Commands

```bash
cargo check
cargo test
cargo fmt
cargo run -p app
```

## Health Checks

```bash
curl http://127.0.0.1:3007/health
curl http://127.0.0.1:3007/ready
```

`/ready` checks:

- App database.
- Fineract database.
- `pgvector` extension.
- Redis when `REDIS_ENABLED=true`.

## API Responses

All API responses use the same envelope:

```json
{
  "success": true,
  "data": {},
  "error": null
}
```

Error responses use:

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "unauthorized",
    "message": "invalid API key"
  }
}
```

Client-facing errors must stay sanitized. Do not return raw parser errors, SQL, stack traces, prompts, or secrets.

## Authentication

### Create API Key

API key creation is protected by the bootstrap admin token:

```bash
curl -X POST http://127.0.0.1:3007/auth/api-keys \
  -H "Authorization: Bearer local-admin-token" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "local-dev-client",
    "owner": "Antun",
    "allowed_office_ids": [1],
    "allowed_capabilities": ["savings_deposit_total"],
    "can_view_pii": true
  }'
```

The raw API key is returned once. Store it securely. The database stores only `key_hash` and `key_prefix`.

### Authenticate API Key

Protected requests can use either header:

```http
Authorization: Bearer <api_key>
```

or:

```http
X-API-Key: <api_key>
```

Check the current authenticated client:

```bash
curl http://127.0.0.1:3007/auth/me \
  -H "Authorization: Bearer <api_key>"
```

## Database

App database:

- Stores API keys and future durable app state.
- Owns migrations under `migrations/*.sql`.
- Hosts the `pgvector` extension.

Fineract database:

- Read-only reporting source.
- Must not be modified by this service.
- Runtime reporting must use approved query definitions, not arbitrary AI-generated SQL.

## Chat And Job Design

Planned durable chat/job state belongs in PostgreSQL:

- `chat_sessions`
- `chat_messages`
- `chat_jobs`
- `chat_job_checkpoints`
- `chat_job_events`

Redis is planned only for live progress and temporary coordination:

- `chat_job:{job_id}:live_state`
- `chat_job:{job_id}:latest_event`
- `chat_job:{job_id}:lock`

Memory is never the source of truth for resumable jobs.

## Documentation

- `docs/project-setup.md`: workspace and crate setup rules.
- `docs/implementation-steps.md`: current implementation roadmap.
- `docs/chat-data-model.md`: planned chat/session/job schema and Redis state.
- `docs/ai-reporting-design.md`: broader reporting service design.
- `AGENTS.md`: operating instructions for AI coding agents working in this repo.

## Safety Rules

- Do not modify Apache Fineract code or schema.
- Do not execute arbitrary SQL generated by AI.
- Do not store raw API keys.
- Do not expose secrets or internal errors in client responses.
- Do not create new workspace crates until the architecture explicitly changes.
- Keep schema changes in migration files.
