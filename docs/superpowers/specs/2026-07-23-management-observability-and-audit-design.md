# Management observability and audit design

## Status

Proposed design for [issue 006](../../issues/active/006-management-observability-and-audit-contract.md). This document is for review only; it authorizes no implementation until accepted with its companion plan.

## Goal

Provide an admin-facing, stable HTTP contract through which a client can fully integrate management screens for:

1. discovering the knowledge currently offered by the system;
2. explaining an individual chat job safely;
3. searching application audit events;
4. observing configured-model token, cost, latency, and failures; and
5. seeing management runtime health.

The design turns current partial job audit and LLM traces into an explicit operational contract. It does **not** introduce knowledge approval, authoring workflow, provider routing, global conversation memory, quota enforcement, or Fineract operational-audit reporting.

## Confirmed product decisions

- Knowledge management is inventory/discovery only. It displays what is available, deferred, unavailable, or reference-only; it does not submit, approve, publish, or promote knowledge.
- Knowledge has two non-interchangeable kinds: `catalog` (controlled operational metadata) and `reference` (SOP/FAQ/guidance). Only catalog items with `execution_mode: approved_catalog_query` can participate in approved execution.
- A single configured LLM provider/model is active. A failure is reported; it does not silently route to a different provider.
- Token/cost management is monitoring and advisory warning only. Technical context-window protection remains mandatory, but there are no usage quotas or cost blocks.
- Session history remains per session until admin archive/delete. The system will not derive or retrieve global summaries from all chats.
- Every management API is authenticated by the existing bearer JWT admin identity. API keys do not authenticate management requests and must not affect management visibility.
- This application's audit is distinct from Fineract's audit tables and operational/user/security reports, which remain deferred.

## Existing state and gap

`chat_job_audit_events` is durable and append-only by convention, and `assistant_llm_traces` records per-call usage. Job memory, checkpoints, messages, and selected retrieval trace also persist. However, the exposed audit timeline is job-only, event coverage is not contractual, its writer is a bounded in-memory best-effort queue, and no management API provides aggregate or searchable data.

The implementation must preserve current chat APIs during migration. In particular, `GET /chat/jobs/{job_id}/audit` remains supported as a compatibility projection until all supported clients migrate to the new management detail endpoint.

## Invariants

1. PostgreSQL remains the durable source of truth. Redis and SSE are never audit sources.
2. Management access is server-enforced admin bearer authentication. A frontend role check is not sufficient.
3. No LLM-generated or arbitrary SQL executes. Audit references query/capability identifiers only; it does not expose SQL text.
4. Office restrictions remain bound inside approved SQL and are represented only as safe scope metadata in audit.
5. Audit payloads are allowlisted summaries. They never persist or return raw API keys, credentials, authorization headers, raw prompts, raw chat history, raw document chunks, embeddings, raw SQL, stack traces, or large/raw query results.
6. Decision audit is append-only. A correction/late outcome appends an event; it never rewrites an old event.
7. A job's catalog/index version is captured at job creation (or first retrieval) and remains associated with that job even after a catalog refresh.
8. Telemetry loss is measurable. A caller must never interpret telemetry as a complete decision trail.
9. Management API JSON is versioned by endpoint contract and uses stable enums/discriminators. Clients must not infer behavior from English labels or internal graph-stage names.

## Terms

| Term | Meaning |
| --- | --- |
| Decision audit | Durable evidence of a material authenticated action or decision that explains a job outcome. |
| Telemetry | High-volume measurements useful for operations but allowed to be asynchronous; loss is counted. |
| Catalog item | Machine-readable operational knowledge, such as a capability or parameter definition. |
| Reference item | Non-executable SOP, FAQ, or guidance metadata. |
| Catalog version | Immutable checksum/version of loaded knowledge. |
| Index version | The catalog-vector-index snapshot used for retrieval, when relevant. |
| Context summary | Bounded metadata describing categories/identifiers and token budget; never raw context content. |

## Architecture

```text
admin client
  -> bearer JWT / management route
  -> management service
  -> management repositories
  -> PostgreSQL audit + LLM trace + catalog/index tables

chat job lifecycle
  -> transactionally persist job state + decision-audit outbox record
  -> outbox dispatcher -> immutable decision-audit event
  -> asynchronous telemetry writer -> LLM/context telemetry
  -> counters/status expose any telemetry loss
```

Management is a module inside the existing `chat` crate; do not add a crate. It follows `route -> service -> repository -> database`. The `app` crate only composes it through `ChatAppState`; `core` continues to provide authentication, shared API primitives, configuration, and pools.

### Decision audit versus telemetry

| Property | Decision audit | Telemetry |
| --- | --- | --- |
| Examples | job created, policy result, execution authorized/completed/blocked, terminal result, session archive/delete | LLM latency/tokens/cost, context token estimate, progress timing, retry detail |
| Write path | Same database transaction as the material state change, or a transactional outbox | Asynchronous writer allowed |
| Loss | Not silently allowed | Allowed only with a persisted/exposed loss counter |
| API role | Explanation and investigation | Analytics and operational warning |

A transactional outbox is the chosen durability boundary. A state mutation and its outbox row are committed together. A dispatcher writes the immutable final event with idempotency on `outbox_id`; dispatcher retries do not duplicate it. The existing bounded `AuditHandle` may remain only for telemetry after the migration; it cannot be the sole writer for a required decision event.

### Reviewer-blocker decisions

- New outbox and decision-event `job_id`/`session_id` foreign keys are nullable and use `ON DELETE SET NULL`. Each row also stores immutable `aggregate_type` and `aggregate_id` snapshots, so losing a live relation never loses the audited aggregate identity.
- Published `management_audit_events` are database-immutable: migration-installed triggers reject `UPDATE` and `DELETE` for the application role. Corrections and retention actions append events; any future privileged retention exception requires a separately approved migration and policy.
- The dispatcher claims due rows (`published_at IS NULL AND next_attempt_at <= now()`) using `FOR UPDATE SKIP LOCKED`. Its publish insert and outbox completion update occur in one PostgreSQL transaction; `outbox_id` uniqueness makes a retry safe. Failures increment attempts, store only a normalized error code, and schedule bounded exponential backoff; exhausted rows remain visible as failed/delayed, never discarded.
- PostgreSQL cannot atomically include external approved-SQL execution. Persist `execution.authorized` before issuing external SQL, then append `execution.completed` or `execution.blocked`/`chat.job_failed` after its known outcome. The before event records authorization, not a claimed completion.
- Legacy `chat_job_audit_events` currently prevents hard job deletion, and current session deletion is archive-only. This release must not introduce a hard-delete path or claim one exists; its new nullable references prepare independent audit retention only.
- `assistant_llm_traces` must not be cascade-deleted with API keys or another ownership row. Migration review must replace conflicting cascade retention with a nullable/set-null ownership reference plus immutable actor/correlation snapshot (or otherwise decouple the trace), preserving independent telemetry retention.
- Provider, database, and dispatcher failures are classified into a finite normalized code/category allowlist before any audit, trace, outbox, counter, or API write. Raw provider text is neither persisted in these fields nor returned.

## Data design

### New tables

Names are provisional until migration review; they must be additive.

#### `management_audit_outbox`

Durable pending decision events, inserted in the same transaction as the material change.

```text
id UUID PK
aggregate_type TEXT                -- immutable snapshot: chat_job | chat_session | management
aggregate_id UUID                  -- immutable snapshot, never nulled
job_id UUID NULL FK ON DELETE SET NULL
session_id UUID NULL FK ON DELETE SET NULL
actor_user_id UUID NULL
role TEXT NULL
correlation_id UUID
contract_version SMALLINT
payload JSONB                     -- validated safe envelope only
occurred_at TIMESTAMPTZ
published_at TIMESTAMPTZ NULL
next_attempt_at TIMESTAMPTZ
attempt_count INTEGER
last_error_code TEXT NULL          -- normalized allowlisted code only
created_at TIMESTAMPTZ
```

Indexes: a partial due-row index on `(next_attempt_at, created_at) WHERE published_at IS NULL`, plus job timeline, session timeline, and correlation-id indexes. The dispatcher uses the partial index with `FOR UPDATE SKIP LOCKED`; it does not scan or lock all unpublished rows.

#### `management_audit_events`

Immutable published decision events. The public API reads this table, not the outbox.

```text
id UUID PK
outbox_id UUID UNIQUE NULL
job_id UUID NULL FK ON DELETE SET NULL
session_id UUID NULL FK ON DELETE SET NULL
aggregate_type TEXT                -- immutable snapshot
aggregate_id UUID                  -- immutable snapshot, never nulled
actor_user_id UUID NULL
role TEXT NULL
event_type TEXT
outcome TEXT
correlation_id UUID
contract_version SMALLINT
catalog_version_id UUID NULL
catalog_content_hash TEXT NULL
index_version_id UUID NULL
summary_json JSONB
sanitized_error_json JSONB NULL
occurred_at TIMESTAMPTZ
created_at TIMESTAMPTZ
```

The existing `chat_job_audit_events` remains readable during migration and currently blocks hard job deletion. The implementation must not bulk-copy unsafe historical JSON into the new table without sanitizing/validating it. A database trigger makes published `management_audit_events` reject application-role updates and deletes.

#### `management_telemetry_counters`

Daily or process-flush counters for `telemetry_enqueued`, `telemetry_persisted`, `telemetry_dropped_queue_full`, `telemetry_persist_failed`, and `telemetry_retry_exhausted`, keyed by counter date and kind. This makes loss observable without inventing a complete event trail.

### Existing table extensions

Additive columns may be added to `assistant_llm_traces` for:

- `correlation_id`;
- `context_contract_version`;
- `price_version` and `cost_currency`;
- normalized `error_code` (replace public use of arbitrary provider error text);
- optional `catalog_version_id` and `index_version_id`.

Actual token counts remain provider-reported where available. Missing usage is represented explicitly, not as a false zero in analytics.

### Safe event schema

All events are validated before persistence against an internal tagged enum. JSONB is used for extensible allowlisted summary fields, not arbitrary untyped diagnostics.

```json
{
  "contract_version": 1,
  "event_type": "policy.evaluated",
  "outcome": "success",
  "actor": { "user_id": "uuid", "role": "admin" },
  "correlation_id": "uuid",
  "knowledge": {
    "catalog_content_hash": "sha256...",
    "index_version_id": "uuid-or-null",
    "domain_id": "savings",
    "capability_id": "savings_deposit_total",
    "query_id": "savings_deposit_total"
  },
  "summary": { "policy_result": "allowed" }
}
```

Public stable `event_type` values in v1:

```text
chat.job_created
knowledge.retrieval_completed
context.assembled
policy.evaluated
execution.authorized
execution.blocked
execution.completed
chat.clarification_requested
chat.clarification_received
chat.job_completed
chat.job_failed
chat.session_archived
chat.session_deleted
```

Public `outcome` values are `success`, `blocked`, `clarification`, `unsupported`, and `failed`. Internal graph steps may be present only in a non-contract diagnostic field and must never be required by a client.

### Context and knowledge summary

`context.assembled` contains only:

```json
{
  "context_contract_version": 1,
  "budget_tokens": 0,
  "estimated_input_tokens": 0,
  "included_categories": ["policy", "scope", "job_state", "catalog_evidence"],
  "omitted_categories": ["session_history"],
  "truncation_reason": "budget_exceeded | none",
  "evidence": [
    { "kind": "catalog_capability", "id": "...", "score": 0.0 },
    { "kind": "reference_chunk", "document_id": "...", "chunk_id": "...", "score": 0.0 }
  ]
}
```

Reference document and chunk IDs are metadata only. Whether a title/description is safe to return is governed by its reference-knowledge classification; raw chunk text never appears in audit.

## Public management API

All responses use `{ success, data, error }`. List endpoints use opaque cursor pagination, a server-bounded `limit`, and `next_cursor`. Timestamps are RFC 3339 UTC. All enum values serialize as `snake_case`.

Errors use the existing sanitized `ApiError` envelope. Required stable validation codes include `invalid_time_range`, `invalid_cursor`, `invalid_filter`, `management_feature_unavailable`, and `resource_not_found`; authorization uses the existing role error behavior.

### `GET /management/status`

Returns safe runtime state for the management landing page.

```json
{
  "provider": { "name": "...", "model": "..." },
  "catalog": { "content_hash": "...", "validation_status": "valid" },
  "index": { "status": "ready", "version_id": "uuid-or-null" },
  "audit": {
    "decision_audit_status": "healthy | delayed | unavailable",
    "telemetry": { "dropped_events": 0, "last_persisted_at": "..." }
  },
  "features": {
    "reference_knowledge": false,
    "cost_warnings": true
  }
}
```

### `GET /management/knowledge`

Query: `kind`, `status`, `domain_id`, `cursor`, `limit`.

Returns inventory metadata. `status` is a presentation availability status (`available`, `deferred`, `unavailable`), while `execution_mode` is authoritative (`approved_catalog_query`, `catalog_metadata_only`, `reference_only`). Both are mandatory in every item.

The first implementation builds catalog inventory from the in-memory validated `KnowledgeCatalog`. Reference rows return an empty list until a reference-knowledge source exists; status must expose that the feature is disabled rather than fabricating records.

### `GET /management/knowledge/{id}`

Returns a safe detailed projection of a single inventory item: description, domain/data-area IDs, declared parameter contract, output-field sensitivity labels, limitations, source/version metadata, availability, and execution mode. It does not expose SQL or internal document text.

IDs are globally unique in this API by a prefixed public ID (`catalog:<id>`, `reference:<id>`) to avoid future collisions.

### `GET /management/audit/jobs/{job_id}`

Returns the ordered decision event timeline and sanitized linked LLM call summaries for one job. It returns `404` if absent/not visible. It is the full-recovery source for an audit detail screen.

### `GET /management/audit`

Query: required `from`, `to`; optional `event_type`, `outcome`, `job_id`, `session_id`, `cursor`, `limit`.

The maximum date span and limit are configuration values. It returns newest-first event rows ordered by the stable tuple `(occurred_at DESC, id DESC)`; its opaque cursor encodes both tuple values and continuation uses the matching lexicographic predicate. A client uses the cursor exactly as supplied. Aggregate analytics do not belong in this feed.

### `GET /management/llm-usage`

Query: required `from`, `to`; required `group_by` one of `day`, `model`, `purpose`, `status`.

Returns aggregate call counts, token counts separated into known/unknown usage, estimated cost with `{ amount, currency, price_version }` or `null`, errors, and percentile latency. Costs with different price versions/currencies are not summed into a misleading total: the response either groups them separately or returns `cost_estimate_unavailable`/`price_version_mismatch` warnings.

Warnings are advisory: `usage_missing`, `cost_estimate_unavailable`, `price_version_mismatch`, `telemetry_dropped`, and `unusual_usage_detected`. The first release can implement deterministic threshold warnings based on configuration; it must not claim anomaly detection without an explicit algorithm.

## Authentication and visibility

Create a dedicated `AuthenticatedManagementAdmin` extractor in `core` or reuse a generic role-checked bearer extractor if one exists. It must authenticate only the bearer user session and require `role == "admin"`; unlike `AuthenticatedChatClient`, it must not parse or project `X-API-Key` office scope.

Although all current users are admins, authorization is still checked so later RBAC can narrow access without changing the endpoint contract. The user ID is persisted as the actor on management-affecting actions.

## Catalog and reference knowledge behavior

The validated in-memory catalog is the source for catalog inventory. Existing executable statuses remain execution safeguards. The management API translates them into safe availability and execution-mode fields; it does not add another approval state.

Reference knowledge is intentionally modeled as a provider boundary with a disabled implementation in v1. The inventory/API contracts support it, but no document upload, editor, approval, global summary, or free-form runtime indexing is introduced by this feature. When reference knowledge is later enabled, it must supply stable IDs, source/version metadata, sensitivity classification, and index version before it can enter retrieval or audit evidence.

## Retention and deletion

This design intentionally separates data lifecycles:

| Data | Initial behavior |
| --- | --- |
| Chat session/message | Existing archive/delete behavior; retained until admin action under current policy |
| Decision audit | Retained independently of session archive/delete; no automatic purge in this implementation |
| Telemetry/LLM trace | Retained independently; no automatic purge in this implementation |

Current session deletion is archive-only; it emits a decision event before/with that state change and this release adds no hard-delete behavior. The referenced session may no longer be visible, but audit event IDs, immutable aggregate snapshot, actor/time, and outcome remain. `assistant_llm_traces` retention is independent of API-key deletion and must not cascade through that relationship. A future retention job must use explicit configured periods, append a retention-run audit event, and never silently cascade-delete decision audit.

## Rollout and compatibility

1. Ship additive migrations and repositories first.
2. Begin writing new outbox/event records while retaining old audit writes.
3. Expose management endpoints behind a feature flag that defaults off until migrations and contract tests pass.
4. Keep `/chat/jobs/{id}/audit` unchanged. It may later project the new timeline, but its existing response shape cannot silently change.
5. Provide fixtures and an OpenAPI/schema artifact before frontend integration begins.
6. Enable the management feature in non-production, verify audit/outbox health and data sanitization, then enable production.

## Test strategy

- Unit: event enum/summary validation, error normalization, cursor encoding/decoding, time-range validation, cost aggregation by price version, telemetry counter increments.
- Repository integration: transaction writes state plus outbox atomically; due-row claim uses `FOR UPDATE SKIP LOCKED`; publish/completion is idempotent in one transaction; failed publish retry/backoff/exhaustion; indexed `(occurred_at, id)` cursor ordering; nullable FK deletion preserves aggregate snapshot; archive-only session deletion retains audit.
- API contract: admin success, unauthenticated/non-admin rejection, envelopes, schemas, pagination, disabled reference-knowledge behavior, no unsafe fields, date bounds, and known error codes.
- Assistant/job scenarios: each terminal job outcome writes required decision events; blocked execution writes no SQL text; clarification events bind to the same job; catalog/index version stays stable across catalog reload.
- Regression: current chat audit endpoint, existing chat authentication, policy/office-scope tests, and all catalog validation tests remain green.

## Acceptance gates

The feature is ready for client integration only when every endpoint has versioned fixture JSON and contract tests, migrations are applied successfully, and all of the following hold:

- An admin can use only documented APIs to list/detail knowledge, inspect a job, search audit, view LLM aggregates, and view status.
- A non-admin cannot access any management surface; an API key alone cannot authenticate it.
- Every material terminal job path has durable ordered decision events, with event loss not dependent on the in-memory queue.
- Every LLM trace exposes normalized provider/model/purpose/status and safe token/cost/latency semantics; unknown data stays unknown.
- Catalog and index versions, selected IDs, and bounded context metadata are visible without raw source content.
- Archive-only session deletion does not silently erase decision audit; nullable relations retain immutable aggregate identity, and API-key deletion cannot cascade-remove LLM traces.
- Published decision events reject application-role update/delete attempts; dispatcher retries are due-indexed, locked safely, idempotent, and expose exhausted failures.
- External approved-SQL attempts have a durable authorization event before the call and a distinct terminal outcome event after it; no event falsely claims atomic external execution.
- Persisted and returned provider/dispatcher/database errors contain only normalized allowlisted codes/categories, never raw provider text.
- API payloads and persisted public audit summaries pass explicit redaction tests for SQL, secrets, raw prompts, document chunks, result rows, and stack traces.
- Cost warnings are advisory only and context-window failure remains a technical safety outcome.
