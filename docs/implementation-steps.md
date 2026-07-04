# Implementation Steps

This document defines the step-by-step implementation order for the AI Reporting Service.

The goal is to build the system incrementally, with each step producing a testable milestone. Do not jump directly into AI planning or report execution before the application foundation, authentication, and observability are ready.

## Phase 0: Project Baseline

Goal: ensure the project compiles and the local environment is ready.

Tasks:

1. Confirm Rust project builds with installed dependencies.
2. Confirm `.env` contains required application, database, LLM provider, auth, and guard settings.
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
DONE

api_keys migration exists.
chat session/job migration exists.
knowledge catalog/index migration exists.
Local/dev startup migration is controlled by APP_DATABASE_MIGRATE_ON_STARTUP.
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
DONE
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
DONE

API key authentication produces a ClientContext.
Authorization helpers in crates/chat/src/policy/authorization.rs are wired through chat::chat::planner::evaluate_policy and gate chat::chat::executor::execute_plan.
Office filtering is enforced inside approved SQL via office_id = ANY($3::bigint[]) bound from policy_decision.office_ids.
Still pending: PII response masking templates beyond MVP fields (tracked under Phase 16).
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
queued
checking_context
embedding
taking_decision
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
DONE: migration 20260617130000_create_chat_tables.sql creates chat sessions, messages, jobs, checkpoints, events, and indexes.
```

## Phase 9: Chat Job API Foundation

Goal: create authenticated chat job endpoints before knowledge/planner/report execution.

Crate placement:

```text
crates/chat
```

Rules:

1. Use the crate name `chat`, not `ai_report_chat`.
2. Keep `core` as shared foundation for config, DB pools, API primitives, auth, and response/error types.
3. Keep chat session/job service, repository, and future pipeline orchestration inside `chat`.
4. Do not create `knowledge` or `reporting` crates for this phase.

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
6. Keep route -> handler -> service -> repository -> database boundaries.

Current status:

```text
PARTIALLY DONE

Implemented:
POST /chat/sessions
GET  /chat/sessions/{session_id}
GET  /chat/sessions/{session_id}/messages
POST /chat/jobs
GET  /chat/jobs/{job_id}
GET  /chat/jobs/{job_id}/stream
POST /chat/jobs/{job_id}/responses

Current module layout:
crates/chat/src/api      = routes, handlers, DTOs
crates/chat/src/chat     = model, repository, service
crates/chat/src/policy   = authorization guard helpers

Background worker:
POST /chat/jobs and POST /chat/jobs/{job_id}/responses now insert + emit the queued event, then spawn the pipeline (clarification / execute / fail) via tokio::spawn, so the HTTP call returns immediately.
JobService::run_pipeline is the shared async worker entry point used by both create and respond.

Redis-backed SSE:
JobService::emit_event writes every event durably to PostgreSQL (chat_job_events) AND publishes a best-effort snapshot to Redis key chat_job:{job_id}:latest_event with a 1h TTL. Terminal events (final/error) also set chat_job:{job_id}:live_state to completed/failed.
GET /chat/jobs/{job_id}/stream now polls Redis every 1s, emits an SSE "update" frame on each tick, and stops when live_state is completed/failed or after a 120s safety window. When Redis is disabled it falls back to the previous single PostgreSQL snapshot frame.

Still pending for this phase:
broader chat_job_checkpoints writes at additional pipeline boundaries (currently queued, clarification_required, response_completed, job_failed)
:lock key for multi-instance fairness (single-process worker is fine for MVP)
PubSub fan-out for sub-second SSE latency (polling at 1s is sufficient for current UX)
```

## Phase 10: Catalog Foundation

Goal: load and validate YAML knowledge files.

Reference design:

```text
docs/knowledge-catalog.md
```

Initial folders:

- [x] `knowledge/data-scope/`
- [x] `knowledge/domains/`
- [x] `knowledge/schema/`
- [x] `knowledge/metrics/`
- [x] `knowledge/capabilities/`
- [x] `knowledge/queries/`
- [x] `knowledge/policies/`
- [x] `knowledge/responses/`
- [x] `queries/`

Initial files:

- [x] `knowledge/data-scope/reporting-scope.yaml`
- [x] `knowledge/data-scope/areas/*.yaml`
- [x] `knowledge/domains/savings.yaml`
- [x] `knowledge/domains/client.yaml`
- [x] `knowledge/domains/organization.yaml`
- [x] `knowledge/schema/fineract/*.yaml`
- [x] `knowledge/schema/fineract/enums/*.yaml`
- [x] `knowledge/schema/fineract/joins/*.yaml`
- [x] `knowledge/schema/fineract/columns/*.yaml`
- [x] `knowledge/metrics/savings/*.yaml`
- [x] `knowledge/capabilities/savings/deposit_total.yaml`
- [x] `knowledge/capabilities/savings/deposit_top_n.yaml`
- [x] `knowledge/queries/savings/deposit_total.yaml`
- [x] `knowledge/queries/savings/deposit_top_n.yaml`
- [x] `knowledge/policies/*.yaml`
- [x] `knowledge/responses/*.yaml`
- [x] `queries/savings/deposit_total.sql`
- [x] `queries/savings/deposit_top_n.sql`

Catalog validation:

1. [x] Required YAML fields exist for loaded catalog layers.
2. [x] Capability references existing domain.
3. [x] Capability references existing query id.
4. [x] Query YAML references existing SQL file.
5. [x] Required parameters are declared.
6. [x] Output fields are declared.
7. [x] Guards are declared in query/capability YAML.
8. [x] Schema/metric/policy/response files are loaded into the runtime catalog.
9. [ ] Schema/metric/policy/response references are fully validated by typed Rust schemas.

Endpoint:

```text
POST /catalog/validate
```

Current status:

```text
PARTIALLY DONE

Project-level knowledge and query folders exist.
Initial MVP YAML/SQL files exist and are marked complete for data scope, domains, schema, metrics, capabilities, queries, policies, and responses.
Every `knowledge/**/*.yaml` file now declares explicit `checks` metadata.
Knowledge checks metadata covers capability-query contracts, office scope, PII, SQL safety, data scope, domain runtime status, metrics, responses, enums, and schema joins.
Loader and validator are implemented under crates/chat/src/knowledge/catalog.
Current loader coverage includes data areas, domains, schema, metrics, capabilities, queries, policies, and responses.
Current validator coverage includes ids/checks for every loaded layer, data area/domain refs where declared, status values, basic executable capability requirements, parameter types, output sensitivity classes, and static SQL safety checks.
Schema, metric, policy, and response layers currently use GenericKnowledge loading; field-specific typed schemas remain pending.
Retrieval document builder exists under crates/chat/src/knowledge/retrieval.rs.
Catalog/index persistence exists under crates/chat/src/knowledge/index and writes generated retrieval documents.
Voyage embedding sync exists for startup sync when CATALOG_SYNC_ON_STARTUP=true and VOYAGEAI_API_KEY is configured.
POST /catalog/validate is implemented and authenticated.

Still pending for this phase:
reject unknown YAML fields after schemas stabilize
validate guards and policy references more completely
runtime vector retrieval fallback exists for chat job creation
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

Current status:

```text
PARTIALLY DONE

Implemented static checks:
SQL file exists
SQL starts with SELECT
SQL is single-statement
SQL does not contain blocked unsafe command tokens
SQL placeholders match declared parameter count/order
basic SQL casts match declared parameter types
office/date/limit clauses are present when required by metadata

Runtime checks added via crates/chat/src/knowledge/catalog/validator.rs::validate_runtime:
SQL is prepared against the Fineract pool (covers parse / EXPLAIN gate without executing rows)
Returned column names are compared to the declared output_fields contract
Wired into POST /catalog/validate; route fails fast on parse or contract mismatch

Still pending:
column type matching against output_fields (currently name-only; runtime executor try_get catches type drift)
table/column cross-check against loaded schema knowledge (depends on Phase 10 schema typing)
```

## Phase 12: Local Classifier MVP

Goal: classify simple savings deposit questions without AI first.

Supported examples:

```text
Who made the largest deposit today?
Show the largest deposits today.
What is the total deposit this month?
Total deposits from January to September 2026.
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

Current status:

```text
PARTIALLY DONE

Implemented:
crates/chat/src/chat/classifier.rs
Savings-specific local capability rules were removed; runtime capability selection now comes from vector/catalog retrieval plus approved clarification options.
Classifier still owns generic parameter extraction for date ranges and top-N limits after a catalog capability is selected.
Stores the classification result in chat_jobs.state_json.classification when a job is created.

Still pending:
typed parameter extraction from query metadata beyond date range and top-N limit
confidence calibration for broader domains as more approved capabilities are added
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

Current status:

```text
PARTIALLY DONE

Implemented:
crates/chat/src/chat/planner.rs
Matched classifier results are converted into a minimal atomic execution plan.
Execution plan is stored in chat_jobs.state_json.execution_plan when a job is created.
Current plan loads and validates the catalog, then maps the matched capability to its approved query id from catalog metadata.
The validated catalog is cached in ChatAppState and reused by job planning.
Policy decision is stored in chat_jobs.state_json.policy_decision when a job is created.
Current policy decision checks API key capability, effective office scope, and simple PII permission before any execution.

Still pending:
required parameter completeness validation against catalog metadata
date range and limit guard enforcement
output mode lookup from richer typed capability metadata instead of MVP naming heuristic
using policy_decision to block execution once a real executor exists
```

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

Current status:

```text
PARTIALLY DONE

Implemented:
crates/chat/src/chat/executor.rs
Synchronous executor runs approved catalog SQL after policy_decision is allowed.
Approved SQL is selected through static `include_str!` bindings by query id, not runtime dynamic SQL strings.
Parameters are bound from execution_plan and policy_decision; user input is not concatenated into SQL.
Results are stored in chat_jobs.result_json and job status becomes completed.
Execution/policy errors are stored as sanitized chat_jobs.error_json and job status becomes failed.
Completion writes response_completed checkpoint and final event.
Failure writes job_failed checkpoint and error event.
Result/error payloads include latency_ms.

Still pending:
statement timeout enforcement
max row enforcement beyond SQL LIMIT metadata
background worker instead of synchronous create-job execution
```

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
LLM provider later
```

Example:

```text
The largest savings deposit today is IDR 25,000,000 from account SV-001.
```

If PII is not allowed:

```text
The largest savings deposit today is IDR 25,000,000 from account SV-****001.
```

Current status:

```text
PARTIALLY DONE

Implemented:
crates/chat/src/chat/formatter.rs
Successful report execution inserts an assistant chat_messages row with a simple English template response.
GET /chat/sessions/{session_id}/messages now shows user and assistant messages after successful execution.
Savings formatter returns empty-result messages for total/top-N/monthly report shapes and only prefixes amounts with a runtime `currency_code` when one is present in query output or request params.
Response formatting is now catalog-driven by `query_id`, `output_mode`, declared `output_fields`, and response field labels instead of hardcoded capability IDs.
PII/secret output fields are omitted by the generic formatter unless a future explicit PII-aware formatter is added.

Still pending:
LLM formatting fallback for complex responses
```

## Phase 17: LLM Provider Integration

Goal: add AI only after the deterministic pipeline works.

Initial use cases:

1. Planner fallback for ambiguous requests.
2. Clarification question generation.
3. Natural-language response formatting for complex results.

Do not use the LLM provider for:

```text
raw SQL generation at runtime
unbounded schema exploration
large result computation
```

Current status:

```text
PARTIALLY DONE

Implemented:
LLM config is loaded from LLM_* environment variables, with legacy DEEPSEEK_* fallback for local compatibility.
crates/chat/src/chat/llm.rs provides a constrained OpenAI-compatible planner fallback client.
Current/default provider is DeepSeek (`LLM_PROVIDER=deepseek`, `LLM_MODEL=deepseek-chat`).
Other OpenAI-compatible providers can be used by changing `LLM_CHAT_COMPLETIONS_URL`, `LLM_MODEL`, and `LLM_API_KEY`.
JobService invokes the LLM only after deterministic/vector classification returns clarification with approved options.
The LLM may return only: one provided capability id, a clarification question, or unsupported.
Returned capability ids are checked against the provided approved options before planning.
Rust still extracts parameters, runs policy checks, and executes only static approved SQL bindings.

Verified 2026-07-02:
Ambiguous prompt "Show customer savings activity this week" returned clarification_required with source=llm_planner and did not execute SQL.

Still pending:
response formatting fallback for complex results
broader prompt context consumption beyond clarification options
```

## Phase 18: Vector Indexing

Goal: add semantic knowledge retrieval after catalog is stable.

Reference design for the full RAG pipeline (indexing + runtime retrieval):

```text
docs/rag-architecture.md
```

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

Current status:

```text
PARTIALLY STARTED

Database tables exist for knowledge_catalog_versions and knowledge_index.
Retrieval document hashes and index persistence exist.
Voyage document embeddings are generated when catalog startup sync is enabled.
Runtime query embedding and capability vector search are wired into chat job creation.
Catalog lexical retrieval is used as a fallback when embedding/vector search is unavailable.
Vector search is restricted to rows that can map back to the caller's allowed_capabilities.
Capability rows and query rows can both select approved capabilities; query candidates are mapped back to their owning capability before planning.
Vector search uses the latest indexed/embedded catalog version and collapses duplicate capability ids.
Current confidence policy: <0.40 unsupported, 0.40-0.55 clarify, close candidates within 0.05 clarify, clear >=0.55 can execute after policy checks.
Classification state records source (`local_rule`, `vector`, or clarification source) and vector candidates for manual verification.
POST /vector-index/rebuild and GET /vector-index/status are implemented (authenticated; rebuild runs KnowledgeSyncService::with_embeddings, status returns the latest knowledge_catalog_versions row).
Broader retrieval: KnowledgeRepository::search_context queries non-capability rows from the latest indexed catalog version; results are appended to classification.candidates with their source_type for audit and future LLM planner consumption — they do not directly execute SQL.

Important sequencing rule:
Vector retrieval only selects knowledge candidates that resolve to approved capabilities. SQL execution still goes through catalog validation, policy guard, and static approved SQL bindings.
```

## Phase 19: Reporting Expansion

Goal: add more reporting capabilities after MVP.

Current status:

```text
PARTIALLY DONE (savings matrix + organization/client foundation summaries)

Slice 1 — withdrawal capabilities:
savings_withdrawal_total + savings_withdrawal_top_n capability + query YAML.
queries/savings/withdrawal_total.sql + withdrawal_top_n.sql (mirror deposit, transaction_type_enum=2).
savings.withdrawal_amount metric flipped to approved_mvp; savings.withdrawal_count added.

Slice 2 — monthly breakdown:
savings_deposit_monthly_breakdown capability + query YAML.
queries/savings/deposit_monthly_breakdown.sql (GROUP BY date_trunc('month', transaction_date)).
OUTPUT_MODES extended with "monthly_breakdown".
Generic catalog-driven formatter renders declared output fields and labels from response catalog.
Executor resolves SQL from QueryKnowledge.sql_file under queries/; no query-id match arms are required.

Routing: vector retrieval picks the right capability via embedding distance (no classifier
change needed). classify_retrieved_capability is generic on output_mode — top_n adds limit,
total and monthly_breakdown only need from_date/to_date. PII gate in planner is derived from
the selected query output_fields sensitivity, not output_mode naming.

Local savings keyword classifier was removed; runtime capability selection comes from vector/catalog retrieval plus approved clarification options.

Slice 3 — date-range parser upgrade (classifier.rs::date_range):
Added: yesterday/kemarin, this year / tahun ini / ytd / year-to-date, last year / tahun lalu,
last month / bulan lalu, last week / minggu lalu, relative counts ("last 7 days", "past 30 days",
"3 months ago", "3 bulan terakhir", "5 hari lalu"), bare year ("deposits in 2026"), and
month-range with default-current-year ("from January to September" → 2026-01-01 .. 2026-09-30).
date_range now lowercases internally so callers don't have to.
13 new unit tests cover each pattern, including January wraparound for "last month".

Slice 4 — monthly top-N capability:
savings_deposit_monthly_top_n capability + query YAML.
queries/savings/deposit_monthly_top_n.sql uses a CTE + ROW_NUMBER() OVER (PARTITION BY month
ORDER BY amount DESC) to pick top-N per month.
OUTPUT_MODES extended with "monthly_top_n".
Validator: SQL safety check now accepts queries that start with WITH (CTE) in addition to SELECT.
Validator: limit-bound check now accepts ROW_NUMBER() / RANK() as alternative to trailing LIMIT.
Classifier classify_retrieved_capability now treats any output_mode ending in "top_n" as the
top_n shape (adds `limit` param); monthly_top_n default limit is 1, atomic top_n stays at 10.
Planner PII gate checks selected query output field sensitivity, so monthly_top_n requires can_view_pii when client identity is included.
Generic formatter renders monthly_top_n rows from output contract fields.

Slice 5 — snapshot balance summary:
savings_balance_summary capability + query YAML.
queries/savings/balance_summary.sql aggregates m_savings_account.account_balance_derived over
active client-owned accounts, filtered by m_client.office_id ∈ allowed_office_ids.
OUTPUT_MODES extended with "summary".
Validator: approved capability with output_mode == "summary" may declare empty required_parameters
(no time/limit/etc. user inputs needed; office scope is implicit from API key).
Classifier classify_retrieved_capability skips date_range for output_mode == "summary".
savings.account_balance metric flipped to approved_mvp.
Generic formatter renders the summary from output contract fields and response labels.

Withdrawal monthly mirrors:
savings_withdrawal_monthly_breakdown capability + query YAML + SQL file + formatter support.
savings_withdrawal_monthly_top_n capability + query YAML + SQL file + formatter support.
queries/savings/withdrawal_monthly_breakdown.sql and withdrawal_monthly_top_n.sql mirror deposit monthly slices with transaction_type_enum=2.
Retrieval classification now maps query source rows back to owning capability ids before planning.
Postman-derived runtime matrix passed all 9 approved savings capabilities on 2026-07-02.

Organization/client foundation summaries:
organization_office_summary capability + query YAML + queries/organization/office_summary.sql.
client_lifecycle_summary capability + query YAML + queries/client/lifecycle_summary.sql.
Metrics added: organization.office_count, organization.active_staff_count, client.lifecycle_count.
Both SQL files were prepared/executed against FINERACT_DATABASE_URL locally and return non-PII aggregate output only.

Still pending:
group-owned savings balance summary (requires promoting group_center_foundation out of conditional).
loan_* and accounting_* capabilities — blocked until those domains move out of deferred.
```

Next capabilities (in priority order):

```text
loan_disbursement_total (requires loan domain promotion)
loan_repayment_total
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
Phase 17 -> LLM Provider Integration
Phase 18 -> Vector Indexing
Phase 19 -> Reporting Expansion
```
