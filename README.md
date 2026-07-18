# AI Reporting Service

Rust service for asking natural-language reporting questions against an Apache Fineract PostgreSQL database. The service plans against a project-owned capability catalog and executes only approved, parameterized, read-only SQL; the AI never generates SQL.

## Current State

Implemented:

- A three-crate workspace: `app` (binary and composition root), `core` (shared runtime and auth), and `chat` (reporting workflow).
- Typed configuration, tracing, app and Fineract database pools, migrations, graceful startup, and health/readiness endpoints.
- User login, JWT access tokens, `GET /auth/me`, refresh-cookie rotation, logout, and scoped API keys whose raw value is returned once.
- PostgreSQL-backed chat sessions, messages, jobs, checkpoints, events, audit data, and clarification responses.
- Redis-backed live SSE state with PostgreSQL fallback. Redis is never the durable source of truth.
- Catalog loading and validation, persistent vector indexing, Voyage embeddings when configured, and lexical fallback.
- Semantic graph, router, retrieval, planner, policy enforcement, approved-SQL execution, and structured response rendering.
- Authorization for capability, office, and PII scope. Office restrictions are bound inside approved SQL.

The catalog currently contains **25 capabilities and 25 queries**. Savings reporting is implemented; client has 7 approved executable capabilities and organization has 8. Group/center support is conditional. Loan, accounting/GL, tax, custom datatables, and audit/users/operations remain deferred.

Deterministic parameter extraction is partial. It currently covers common limits, ISO date ranges, currency, domain hints, and selected metric hints; broader provenance, conflict handling, and clarification gates remain pending. If no semantic router is available, routing fails closed. LQR exists behind `LQR_ENABLED` and is not the default supported path yet.

See [current status](docs/current/status.md) and [next work](docs/current/next-work.md) for the maintained implementation state.

## Architecture

```text
ai_report/
  crates/
    app/       # binary entrypoint and composition root
    core/      # config, databases, auth, API primitives, readiness
    chat/      # sessions, jobs, catalog, retrieval, policy, execution
  knowledge/  # approved YAML knowledge and capability definitions
  queries/    # approved SQL
  migrations/ # app database schema
```

Dependencies are intentionally one-way:

```text
app -> core
app -> chat
chat -> core
```

Runtime flow:

```text
HTTP route -> service -> repository -> database
                   |
                   -> retrieval -> planner -> policy -> approved SQL -> response
```

PostgreSQL is authoritative for durable state. Redis stores only live SSE/coordination keys such as `chat_job:{id}:live_state`, `:latest_event`, and `:lock`. Clarification continues the same job through `POST /chat/jobs/{job_id}/responses`.

## Requirements

- Stable Rust with edition 2024 support
- PostgreSQL for the app database, with `pgvector`
- A read-only or replica PostgreSQL connection to Fineract
- Redis when live SSE coordination is enabled
- `sqlx-cli` only when running migrations manually

## Local Setup

Start from the checked-in configuration template:

```bash
cp .env.example .env
```

Review `.env` before starting. Its settings are grouped by:

- application and app database
- LLM planner provider
- Voyage embeddings and vector storage
- login, JWT, refresh cookie, and API-key auth
- read-only Fineract database
- Redis
- query/report guards
- catalog paths and validation
- observability

For the current supported runtime path, leave LQR disabled unless intentionally evaluating it:

```env
LQR_ENABLED=false
```

Do not use development credentials or `APP_DATABASE_MIGRATE_ON_STARTUP=true` in production.

Start Redis if enabled:

```bash
docker compose up -d redis
docker compose exec -T redis redis-cli ping
```

Run migrations manually when startup migration is disabled:

```bash
sqlx migrate run --database-url "postgres://root:password@127.0.0.1:5432/ai_reports"
```

Run the service:

```bash
cargo run -p app
```

The default local address is `http://127.0.0.1:3007`.

## Development Commands

```bash
cargo check
cargo test
cargo fmt
cargo run -p app
```

## Health and Readiness

```bash
curl http://127.0.0.1:3007/health
curl http://127.0.0.1:3007/ready
```

`/ready` checks the app database, Fineract database, `pgvector`, and Redis when `REDIS_ENABLED=true`.

## API

All responses use a common envelope:

```json
{
  "success": true,
  "data": {},
  "error": null
}
```

Errors use the same shape with `success: false`, `data: null`, and a sanitized `error` object. Responses must not expose raw SQL, parser errors, prompts, stack traces, secrets, or internal database details.

Implemented route groups:

- Auth: login, refresh, logout, current user, and API-key management
- Sessions: list, create, get, and messages
- Jobs: create, get, audit, SSE stream, and clarification responses
- Catalog: capabilities and validation
- Vector index: rebuild and status
- Runtime: health and readiness

See the [API endpoint map](docs/api/README.md) and [runtime documentation](docs/runtime/README.md).

### Authentication

Log in as an application user to obtain an access token:

```bash
curl -X POST http://127.0.0.1:3007/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"password123"}'
```

The refresh token is managed through an HTTP cookie. Use the access token for protected routes:

```bash
curl http://127.0.0.1:3007/auth/me \
  -H "Authorization: Bearer <access-token>"
```

API keys are created by an authenticated admin at `POST /auth/api-keys`. Store the returned raw key immediately; the database stores only its hash and prefix.

Chat routes require an admin JWT access token in `Authorization`; an API key never replaces it. `X-API-Key` is optional: only a valid, scoped key contributes `allowed_office_ids` and narrows office scope. Absent, invalid, revoked, expired, or ownerless keys are ignored; API-key capability and PII scopes are not currently applied by this extractor.

### Chat Example

Create a session:

```bash
curl -X POST http://127.0.0.1:3007/chat/sessions \
  -H "Authorization: Bearer <access-token>" \
  -H "X-API-Key: <optional-scoped-api-key>" \
  -H "Content-Type: application/json" \
  -d '{"title":"Savings report"}'
```

Create a job for that session:

```bash
curl -X POST http://127.0.0.1:3007/chat/jobs \
  -H "Authorization: Bearer <access-token>" \
  -H "X-API-Key: <optional-scoped-api-key>" \
  -H "Content-Type: application/json" \
  -d '{"session_id":"<session-id>","message":"What is the total deposit this month?"}'
```

Read or stream the job:

```bash
curl http://127.0.0.1:3007/chat/jobs/<job-id> \
  -H "Authorization: Bearer <access-token>"

curl -N http://127.0.0.1:3007/chat/jobs/<job-id>/stream \
  -H "Authorization: Bearer <access-token>"
```

If the job needs clarification, send the answer to `POST /chat/jobs/{job_id}/responses`; do not create a replacement job.

## Safety Rules

- Never modify Fineract code or schema; use a read-only or replica connection.
- Never execute AI-generated or arbitrary SQL. Only catalog-approved SQL under `queries/` may run.
- Apply authorized office IDs as bound SQL parameters, never by post-filtering rows in Rust.
- Never store raw API keys or expose secrets and internal errors.
- Keep user-facing language English until multilingual extraction, classification, and templates are implemented and tested.
- Keep schema changes in `migrations/`; startup schema creation is not allowed.
- Keep the workspace at exactly `app`, `core`, and `chat` unless the architecture is deliberately changed.

## Current Limitations and Next Work

- Move session ownership to users and select API-key scope per job.
- Complete verified parameter extraction, provenance, conflict detection, required-parameter validation, and clarification gates.
- Expand semantic scenario/golden coverage and safe real-row fixtures.
- Replace remaining generic knowledge layers with typed catalog schemas.
- Promote deferred domains only after their data scope and approved SQL are ready.
- Evaluate enabling LQR by default only after its scenario gates pass.

## Documentation

- [Current status](docs/current/status.md)
- [Active editing context](docs/current/active-context.md)
- [Next work](docs/current/next-work.md)
- [Architecture overview](docs/architecture/overview.md)
- [Chat data model](docs/architecture/chat-data-model/index.md)
- [Runtime guide](docs/runtime/README.md)
- [API endpoint map](docs/api/README.md)
- [Reporting capabilities](docs/product/reporting-capabilities/index.md)
- [Reporting data scope](docs/product/reporting-data-scope/index.md)
- [PII policy](docs/product/pii-policy/index.md)
- [Implementation roadmap](docs/roadmap/implementation-roadmap.md)
- [Agent operating rules](AGENTS.md)
