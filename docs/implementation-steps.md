# Implementation Steps

This document defines the step-by-step implementation order for the AI Reporting Service.

The goal is to build the system incrementally, with each step producing a testable milestone. Do not jump directly into AI planning or report execution before the application foundation, authentication, and observability are ready.

## Phase 0: Project Baseline

Goal: ensure the project compiles and the local environment is ready.

Tasks:

1. Confirm Rust project builds with installed dependencies.
2. Confirm `.env` contains required application, database, DeepSeek, auth, and guard settings.
3. Confirm PostgreSQL database `ai_reports` exists.
4. Confirm `pgvector` extension is enabled in `ai_reports`.
5. Confirm Fineract database connection values are present in `.env`.
6. Confirm Redis runs through Docker Compose, not Homebrew/local service.

Validation:

```bash
cargo check
cargo test
```

Database validation:

```bash
PGPASSWORD=password psql -h 127.0.0.1 -p 5432 -U root -d ai_reports -c "SELECT extname, extversion FROM pg_extension WHERE extname = 'vector';"
```

Expected result:

```text
vector extension is installed and active
```

Current status:

```text
DONE
```

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
DeepSeek config
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

## Phase 2: Database Connections

Goal: initialize database pools and verify connectivity.

Tasks:

1. Create App PostgreSQL pool using `APP_DATABASE_URL`.
2. Create Fineract PostgreSQL pool using `FINERACT_DATABASE_URL`.
3. Configure max connections.
4. Configure connection timeout.
5. Add database ping helpers.
6. Add pgvector readiness check.
7. Add Redis readiness check when `REDIS_ENABLED=true`.

App database is used for:

```text
api keys
audit logs
execution logs
token usage
catalog snapshots
jobs
vector embeddings
```

Fineract database is used for:

```text
read-only business reporting queries
```

Validation queries:

```sql
SELECT 1;
```

pgvector validation query:

```sql
SELECT extname, extversion FROM pg_extension WHERE extname = 'vector';
```

Expected result:

```text
both database pools connect successfully
pgvector extension is active in app database
```

Current status:

```text
DONE
```

## Phase 3: Health And Readiness Endpoints

Goal: expose basic service status before implementing business features.

Endpoints:

```text
GET /health
GET /ready
```

`/health` checks:

```text
server process is alive
```

`/ready` checks:

```text
app database connection
fineract database connection
pgvector extension availability
optional redis connection if REDIS_ENABLED=true
```

Example `/health` response:

```json
{
  "status": "ok"
}
```

Example `/ready` response:

```json
{
  "status": "ready",
  "checks": {
    "app_database": "ok",
    "fineract_database": "ok",
    "pgvector": "ok",
    "redis": "disabled"
  }
}
```

Validation:

```bash
curl http://127.0.0.1:3007/health
curl http://127.0.0.1:3007/ready
```

Current status:

```text
DONE
```

## Phase 4: App Database Migrations

Goal: create the minimum database schema needed before auth and audit.

Important rule:

```text
schema changes must live in migration files, not in application startup code
```

Migration behavior:

```text
APP_DATABASE_MIGRATE_ON_STARTUP=false by default
APP_DATABASE_MIGRATE_ON_STARTUP=true allows local/dev startup migrations
```

Current local value:

```text
APP_DATABASE_MIGRATE_ON_STARTUP=true
```

Initial tables:

```text
api_keys
chat_sessions
chat_messages
chat_jobs
chat_job_checkpoints
chat_job_events
audit_logs, later
execution_logs, later
token_usage_logs, later
```

MVP can start with:

```text
api_keys
```

`api_keys` table fields:

```text
id
name
owner
key_prefix
key_hash
allowed_office_ids
allowed_capabilities
can_view_pii
expires_at
revoked_at
created_at
last_used_at
```

Important rule:

```text
never store raw API keys
```

Chat data model reference:

```text
docs/chat-data-model.md
```

Validation:

```bash
sqlx migrate run
```

Expected result:

```text
migrations run successfully
api_keys table exists
```

Current status:

```text
PARTIALLY DONE

api_keys migration exists and has been applied.
chat session/job migrations still need to be created.
```

## Phase 5: API Key Generation

Goal: allow creating API keys for clients.

Endpoint:

```text
POST /auth/api-keys
```

Protection:

```text
Authorization: Bearer <AUTH_BOOTSTRAP_ADMIN_TOKEN>
```

Request:

```json
{
  "name": "local-dev-client",
  "owner": "Antun",
  "expires_at": null,
  "allowed_office_ids": [1, 2, 3],
  "allowed_capabilities": [
    "savings_deposit_total",
    "savings_deposit_top_n"
  ],
  "can_view_pii": true
}
```

Response:

```json
{
  "success": true,
  "data": {
    "id": "...",
    "api_key": "air_test_...",
    "message": "Store this API key securely. It will not be shown again."
  },
  "error": null
}
```

Implementation rules:

1. Generate a cryptographically secure random secret.
2. Prefix key using `API_KEY_PREFIX`.
3. Hash the full raw key.
4. Store only the hash and metadata.
5. Return raw key only once.
6. Store visible prefix for debugging.

Validation:

```bash
curl -X POST http://127.0.0.1:3007/auth/api-keys \
  -H "Authorization: Bearer local-admin-token" \
  -H "Content-Type: application/json" \
  -d '{"name":"local-dev-client","owner":"Antun","allowed_office_ids":[1],"allowed_capabilities":["savings_deposit_total"],"can_view_pii":true}'
```

Use local port `3007`:

```bash
curl -X POST http://127.0.0.1:3007/auth/api-keys \
  -H "Authorization: Bearer local-admin-token" \
  -H "Content-Type: application/json" \
  -d '{"name":"local-dev-client","owner":"Antun","allowed_office_ids":[1],"allowed_capabilities":["savings_deposit_total"],"can_view_pii":true}'
```

Expected result:

```text
raw API key is returned once
hashed key is stored in database
```

Current implementation notes:

```text
route -> AuthService -> ApiKeyRepository -> PostgreSQL
request validation uses validator crate + global ValidatedJson extractor
responses use a consistent success/data/error envelope
```

Current status:

```text
DONE
```

## Phase 6: API Key Authentication Middleware

Goal: protect all reporting and admin endpoints except health/readiness and key creation.

Supported headers:

```text
Authorization: Bearer <api_key>
X-API-Key: <api_key>
```

Runtime flow:

```text
extract API key
hash API key
find matching key_hash
check revoked_at is null
check expires_at is valid
load scopes
build ClientContext
attach ClientContext to request
```

Client context:

```json
{
  "api_key_id": "key_...",
  "owner": "Antun",
  "allowed_office_ids": [1],
  "allowed_capabilities": ["savings_deposit_total"],
  "can_view_pii": true
}
```

Validation endpoint for middleware:

```text
GET /auth/me
```

Expected response:

```json
{
  "api_key_id": "key_...",
  "owner": "Antun",
  "allowed_capabilities": ["savings_deposit_total"]
}
```

Validation:

```bash
curl http://127.0.0.1:3007/auth/me \
  -H "Authorization: Bearer <generated_api_key>"
```

Current status:

```text
TODO NEXT
```

## Phase 7: Authorization Guards

Goal: enforce API key scopes before report execution.

Guard checks:

1. Selected capability is allowed by API key.
2. Requested office filter is inside `allowed_office_ids`.
3. PII fields are removed or masked if `can_view_pii=false`.
4. Async job result access belongs to the same API key.
5. Query parameters cannot bypass scopes.

This phase depends on Phase 6 because all report/chat endpoints must receive a validated `ClientContext`.

Failure examples:

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "forbidden",
    "message": "This API key is not allowed to run the selected capability."
  }
}
```

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "forbidden",
    "message": "Requested office is outside this API key scope."
  }
}
```

Current status:

```text
TODO
```

## Phase 8: Chat Session And Job Data Model

Goal: create durable chat/session/job state before implementing chatbot pipeline.

Reference design:

```text
docs/chat-data-model.md
```

Tables:

```text
chat_sessions
chat_messages
chat_jobs
chat_job_checkpoints
chat_job_events
```

Storage rule:

```text
PostgreSQL = durable checkpoints and chat history
Redis = live progress state and temporary SSE coordination
Memory = transient only
```

Required job statuses:

```text
queued
running
waiting_for_user_input
completed
failed
expired
cancelled
```

Initial pipeline steps:

```text
checking_context
embedding
response
```

Redis live keys:

```text
chat_job:{job_id}:live_state
chat_job:{job_id}:latest_event
chat_job:{job_id}:lock
```

Checkpoint policy:

```text
save PostgreSQL checkpoints only at important boundaries
do not save every progress/heartbeat update to PostgreSQL
```

Validation:

```bash
sqlx migrate run
```

Expected result:

```text
chat session/job tables exist
indexes exist
```

Current status:

```text
TODO
```

## Phase 9: Chat Job API Foundation

Goal: create authenticated chat job endpoints before knowledge/planner/report execution.

Endpoints:

```text
POST /chat/sessions
GET  /chat/sessions/{session_id}
GET  /chat/sessions/{session_id}/messages

POST /chat/jobs
GET  /chat/jobs/{job_id}
GET  /chat/jobs/{job_id}/stream
POST /chat/jobs/{job_id}/responses
```

Rules:

1. All endpoints require API key authentication.
2. Job ownership must be checked by `api_key_id`.
3. `POST /chat/jobs` may create a session if no `session_id` is provided.
4. Clarification responses must use `POST /chat/jobs/{job_id}/responses`, not a new job.
5. SSE should stream high-level safe events only.

Current status:

```text
TODO
```

## Phase 10: Catalog Foundation

Goal: load and validate YAML knowledge files.

Initial folders:

```text
knowledge/domains/
knowledge/capabilities/
knowledge/queries/
queries/
```

Initial files:

```text
knowledge/domains/savings.yaml
knowledge/capabilities/savings/deposit_total.yaml
knowledge/capabilities/savings/deposit_top_n.yaml
knowledge/queries/savings/deposit_total.yaml
knowledge/queries/savings/deposit_top_n.yaml
queries/savings/deposit_total.sql
queries/savings/deposit_top_n.sql
```

Catalog validation:

1. Required YAML fields exist.
2. Capability references existing domain.
3. Capability references existing query id.
4. Query YAML references existing SQL file.
5. Required parameters are declared.
6. Output fields are declared.
7. Guards are declared.

Endpoint:

```text
POST /catalog/validate
```

## Phase 11: Query Validation

Goal: ensure SQL files are safe before runtime execution.

Validation checks:

1. SQL file exists.
2. SQL is SELECT-only.
3. SQL is not multi-statement.
4. SQL does not contain unsafe commands.
5. Placeholder count matches query metadata.
6. `EXPLAIN` succeeds with sample params.
7. Output columns match output contract when possible.

Unsafe commands include:

```text
INSERT
UPDATE
DELETE
TRUNCATE
DROP
ALTER
CREATE
GRANT
REVOKE
COPY
VACUUM
ANALYZE
```

## Phase 12: Local Classifier MVP

Goal: classify simple savings deposit questions without AI first.

Supported examples:

```text
Deposit terbesar hari ini siapa?
Setoran paling besar hari ini.
Total deposit bulan ini berapa?
Total setoran Januari sampai September 2026.
```

Classifier output:

```json
{
  "domain": "savings",
  "capability": "savings_deposit_total",
  "output_mode": "total",
  "params": {
    "from_date": "2026-01-01",
    "to_date": "2026-09-30"
  },
  "confidence": 0.86
}
```

If confidence is low:

```text
return unsupported or clarification
```

## Phase 13: Execution Plan And Policy Guard

Goal: convert classifier result into validated execution plan.

Plan types:

```text
atomic
composite
iterative
```

MVP only needs:

```text
atomic
```

Policy checks:

1. Capability exists.
2. Query exists.
3. Required params are complete.
4. Date range is within max range.
5. Limit is within max limit.
6. API key can run capability.
7. API key can access requested office scope.

## Phase 14: Query Executor MVP

Goal: execute approved SQL safely against Fineract read-only database.

Executor requirements:

1. Use parameter binding only.
2. Set statement timeout.
3. Enforce max rows.
4. Use read-only pool.
5. Return structured result.
6. Record latency and status.
7. Never concatenate user input into SQL.

## Phase 15: Audit Logging

Goal: make every request traceable.

Audit fields:

```text
request_id
api_key_id
message
decision
domain
capability
query_id
params
status
error_code
latency_ms
created_at
```

Do not log raw API keys.

Avoid logging sensitive result data unless explicitly needed.

## Phase 16: Response Formatting

Goal: return user-friendly answers.

MVP response strategy:

```text
template first
DeepSeek later
```

Example:

```text
The largest savings deposit today is IDR 25,000,000 from account SV-001.
```

If PII is not allowed:

```text
The largest savings deposit today is IDR 25,000,000 from account SV-****001.
```

## Phase 17: DeepSeek Integration

Goal: add AI only after the deterministic pipeline works.

Initial use cases:

1. Planner fallback for ambiguous requests.
2. Clarification question generation.
3. Natural-language response formatting for complex results.

Do not use DeepSeek for:

```text
raw SQL generation at runtime
unbounded schema exploration
large result computation
```

## Phase 18: Vector Indexing

Goal: add semantic knowledge retrieval after catalog is stable.

Initial vector content:

```text
domain knowledge
capability descriptions
example questions
synonyms
unsupported intents
schema summaries
```

Do not vectorize transactional Fineract rows.

Endpoint:

```text
POST /vector-index/rebuild
GET  /vector-index/status
```

## Phase 19: Reporting Expansion

Goal: add more reporting capabilities after MVP.

Next capabilities:

```text
savings_deposit_monthly_breakdown
savings_deposit_monthly_top_n
savings_withdrawal_total
savings_withdrawal_top_n
loan_repayment_total
loan_disbursement_total
```

Each new capability requires:

1. Capability YAML.
2. Query YAML.
3. Approved SQL file.
4. Query validation.
5. Test cases.
6. Permission scope definition.

## Recommended Implementation Order

```text
Phase 0  -> Project Baseline
Phase 1  -> Application Bootstrap
Phase 2  -> Database Connections
Phase 3  -> Health And Readiness
Phase 4  -> App Database Migrations
Phase 5  -> API Key Generation
Phase 6  -> API Key Authentication Middleware
Phase 7  -> Authorization Guards
Phase 8  -> Chat Session And Job Data Model
Phase 9  -> Chat Job API Foundation
Phase 10 -> Catalog Foundation
Phase 11 -> Query Validation
Phase 12 -> Local Classifier MVP
Phase 13 -> Execution Plan And Policy Guard
Phase 14 -> Query Executor MVP
Phase 15 -> Audit Logging
Phase 16 -> Response Formatting
Phase 17 -> DeepSeek Integration
Phase 18 -> Vector Indexing
Phase 19 -> Reporting Expansion
```
