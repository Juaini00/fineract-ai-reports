# 006 — Management observability and audit contract

Status: active — requirements defined; design and implementation pending
Severity: high
Area: management | audit | knowledge | LLM | context | API | client integration | database
Created: 2026-07-23
Resolved:

## Problem

The service has durable chat/job state, partial pipeline audit events, and per-call LLM traces, but it has no unified management contract. An admin cannot reliably inspect what knowledge the system offers, why a job reached an outcome, how context was assembled, or aggregate token, cost, latency, and failure behaviour.

Existing audit writes are also best-effort: an in-memory queue can drop events when full, and a failed batch insert is logged but not retried durably. That is acceptable for non-critical telemetry, but it cannot be the only record for material execution and policy decisions.

The management scope for this phase is **observability and inventory**, not a knowledge approval/publishing workflow. The existing catalog's executable capability status remains an execution guard; it is not a new admin approval process.

## Product decisions already made

- Knowledge management is read-only discovery of what the system provides. There is no draft, review, approval, or publish lifecycle in this phase.
- Knowledge includes both executable operational catalog metadata and non-executable reference knowledge such as SOP, FAQ, and internal guidance.
- Reference knowledge may inform an answer but must never authorize access, alter policy/scope, select arbitrary SQL, or become executable.
- One configured LLM provider/model is used at a time. There is no provider routing or silent fallback.
- Token and cost tracking is observational in this phase: reporting and warnings only, no cost-based blocking or quota enforcement.
- Chat history remains until an admin archives/deletes it. No global cross-session summary or implicit global memory is introduced.
- Admin is the only role. All management endpoints require the existing bearer JWT admin authentication.
- Fineract operational/security audit reporting remains deferred. This issue concerns this application's knowledge, LLM, context, policy, and approved-query lifecycle.

## Current behavior and evidence

Implemented durable data includes:

- `chat_job_audit_events` and `GET /chat/jobs/{job_id}/audit` for a job-specific event timeline.
- `assistant_llm_traces` with provider, model, input/output tokens, estimated cost, latency, status, and error kind.
- Durable chat messages, jobs, job memory, graph checkpoints, response projection, and limited retrieval trace data.

Known gaps:

1. Pipeline stages do not have one mandatory event taxonomy or a complete, consistently persisted timeline.
2. Queue overflow and batch-insert failure can lose audit events.
3. There is no management API for knowledge inventory, audit search, LLM-usage aggregation, cost analysis, or warnings.
4. Audit data does not consistently identify catalog version, index version, reference-document/chunk evidence, prompt/context contract version, context budget, or truncation reason.
5. There is no retention, deletion, export, integrity, or access contract for audit records.
6. Error values recorded internally need an explicit sanitization contract; provider error text must not become a source of secrets, prompt content, or PII leakage.
7. Current LLM traces are per call/job records, not a client-ready aggregate ledger or alerting system.

## Goals

- Give an admin a stable, machine-readable management API that a client can integrate without parsing prose or internal database tables.
- Show the available catalog and reference knowledge, including availability and non-executable status.
- Explain each job outcome without exposing raw SQL, secrets, raw provider prompts, or large/raw data results.
- Provide token, cost, latency, and error observability for the single configured provider/model.
- Preserve durable, queryable audit records for material decisions while keeping lossy telemetry explicitly separate.
- Make context/token selection observable without making global memory or automatic summarization.

## Non-goals

- Knowledge authoring, moderation, approval, workflow, or publishing.
- Multi-provider routing, automatic provider fallback, quotas, or cost-based request blocking.
- A global summary built from chat history.
- General Fineract user/security/operations audit reporting.
- Returning raw prompts, raw SQL, credentials, API keys, internal stack traces, or full query result payloads through management APIs.
- Changing the invariant that only catalog-approved, parameterized, read-only SQL may execute.

## Required audit model

### Two durability classes

| Class | Use | Durability requirement |
| --- | --- | --- |
| Decision audit | Authenticated action, job creation, selected catalog/index version, evidence selection, policy decision, query execution decision/outcome, terminal outcome, archive/delete action | Durable PostgreSQL write in the request/job transaction or a durable outbox; it must not depend only on a bounded in-memory queue. |
| Telemetry | Fine-grained timings, repeated progress, token/cost measurements, retry diagnostics | May be asynchronous, but drop/retry status must be measurable. It must not be presented as a complete compliance trail. |

### Required event envelope

Every decision-audit record must carry:

```json
{
  "id": "uuid",
  "occurred_at": "RFC-3339 timestamp",
  "event_type": "stable_enum",
  "actor": { "user_id": "uuid", "role": "admin" },
  "session_id": "uuid or null",
  "job_id": "uuid or null",
  "outcome": "success | blocked | clarification | unsupported | failed",
  "correlation_id": "request/job correlation identifier",
  "summary": {},
  "sanitized_error": { "code": "stable_code" }
}
```

It must be append-only to application callers. Corrections are new events; existing events are not updated or deleted individually.

### Required job decision events

At minimum, use stable event types for:

- `chat.job_created`
- `knowledge.retrieval_completed`
- `context.assembled`
- `llm.call_completed` and `llm.call_failed` (telemetry-linked)
- `policy.evaluated`
- `execution.authorized`, `execution.blocked`, and `execution.completed`
- `chat.clarification_requested` and `chat.clarification_received`
- `chat.job_completed` and `chat.job_failed`
- `chat.session_archived` and `chat.session_deleted`

The exact internal graph states are not the public event taxonomy. Unknown future events must be safely representable without breaking clients.

### Required evidence and context summary

Audit summaries must use identifiers and bounded metadata, not raw content:

- catalog checksum/version and vector-index version;
- selected domain, capability, query, policy IDs, and authorized office-scope mode;
- reference document IDs and chunk IDs/scores, if reference knowledge was used;
- context contract/prompt-template version;
- token budget, estimated tokens supplied, actual input/output token usage where available;
- omitted context categories and stable truncation reason;
- provider/model, call purpose, latency, cost estimate currency/version, and normalized status;
- final response type and sanitized execution row-count/latency summary.

## Client integration contract

All endpoints use the existing envelope:

```json
{ "success": true, "data": {}, "error": null }
```

All management routes require the existing admin bearer JWT. API keys do not authenticate a management request.

### 1. Knowledge inventory

`GET /management/knowledge?kind=catalog|reference&status=available|deferred|unavailable&cursor=&limit=`

Returns paginated metadata only. A client must use `kind` and `execution_mode`, never infer executability from labels.

```json
{
  "items": [
    {
      "id": "savings_deposit_total",
      "kind": "catalog",
      "title": "Total deposits",
      "description": "...",
      "domain_id": "savings",
      "status": "available",
      "execution_mode": "approved_catalog_query",
      "capability_id": "savings_deposit_total",
      "data_area_ids": ["savings_transactions"],
      "parameter_schema": {},
      "updated_at": "2026-07-23T00:00:00Z"
    },
    {
      "id": "sop-deposit-reconciliation",
      "kind": "reference",
      "title": "Deposit reconciliation SOP",
      "status": "available",
      "execution_mode": "reference_only",
      "source": { "type": "managed_document", "version": "1" },
      "updated_at": "2026-07-23T00:00:00Z"
    }
  ],
  "next_cursor": null,
  "catalog_version": "checksum-or-version",
  "index_version": "uuid-or-null"
}
```

`GET /management/knowledge/{id}` returns the same safe metadata plus declared parameters, output-field sensitivity labels, limitations, and source/version metadata. It never returns SQL, embedding vectors, protected raw document content, or internal policy implementation.

### 2. Job audit detail

`GET /management/audit/jobs/{job_id}`

Returns a stable ordered timeline of decision events and linked, sanitized LLM-call summaries. It is the client recovery/detail source; SSE is not an audit transport.

```json
{
  "job_id": "uuid",
  "catalog_version": "checksum-or-version",
  "outcome": "completed",
  "events": [
    {
      "id": "uuid",
      "occurred_at": "2026-07-23T00:00:00Z",
      "event_type": "policy.evaluated",
      "outcome": "success",
      "summary": {
        "capability_id": "savings_deposit_total",
        "policy_result": "allowed"
      }
    }
  ],
  "llm_calls": [
    {
      "id": "uuid",
      "purpose": "semantic_routing",
      "provider": "configured-provider",
      "model": "configured-model",
      "input_tokens": 0,
      "output_tokens": 0,
      "cost_estimate": { "amount": "0.000000", "currency": "USD", "price_version": "configured-version" },
      "latency_ms": 0,
      "status": "ok"
    }
  ]
}
```

### 3. Audit search

`GET /management/audit?from=&to=&event_type=&outcome=&session_id=&job_id=&cursor=&limit=`

Returns a paginated, newest-first event feed with safe summaries. Time range is required and bounded by server configuration. The response includes `next_cursor`; clients must not use offset pagination.

### 4. LLM usage and cost analytics

`GET /management/llm-usage?from=&to=&group_by=day|model|purpose|status`

Returns aggregate values only:

```json
{
  "range": { "from": "...", "to": "..." },
  "configured_provider": "...",
  "configured_model": "...",
  "totals": {
    "calls": 0,
    "input_tokens": 0,
    "output_tokens": 0,
    "total_tokens": 0,
    "estimated_cost": { "amount": "0.000000", "currency": "USD", "price_version": "configured-version" },
    "errors": 0,
    "p95_latency_ms": 0
  },
  "groups": []
}
```

No hard quota is applied. The server may include `warnings` such as `cost_estimate_unavailable`, `usage_missing`, `telemetry_dropped`, or `unusual_usage_detected`; warnings are advisory and never block a job in this phase.

### 5. Management health/status

`GET /management/status`

Returns safe operational metadata required by a client: configured provider/model identity, catalog/index version and validation status, audit/telemetry health including dropped-event counters, and the enabled management feature flags. It must not return secrets or raw configuration.

## Security, privacy, and retention requirements

- Admin-only access is enforced server-side for every route; the frontend is not an authorization boundary.
- Every response remains enveloped and errors remain sanitized.
- Never expose raw SQL, raw API keys, authorization headers, provider credentials, full prompts, user message bodies, document chunk bodies, embedding vectors, stack traces, or large/raw report results.
- Store only bounded, classified audit summaries. If an error can include provider input or a secret, store a normalized stable error code instead of the original error text.
- Session archive/delete must not silently erase audit evidence. The retention policy must explicitly state whether audit records survive a session deletion and for how long.
- Define configurable retention separately for chat content, decision audit, and telemetry. Until a deletion policy is implemented, do not claim automatic purge.
- Audit queries must be indexed and paginated; unbounded audit exports are not part of the first client contract.

## Proposed implementation order

1. Write a stable API schema/spec for the five management surfaces and fixture responses for client integration.
2. Define event taxonomy, summary allowlist, error normalization, and durability/outbox strategy.
3. Add migrations/repositories for audit versioning, correlation IDs, context summaries, catalog/index references, and telemetry drop/retry counters.
4. Instrument the job/assistant lifecycle at the required decision boundaries; preserve existing job audit endpoint as a compatibility surface or map it to the new contract.
5. Implement management API routes, admin authorization, cursor pagination, filters, and aggregate LLM usage queries.
6. Add contract tests for every endpoint, authorization, sanitization, pagination, no-raw-SQL/no-secret guarantees, and known job lifecycle scenarios.
7. Add retention/deletion behavior and document operational runbooks before declaring audit complete.

## Acceptance criteria

- An admin client can render inventory, job audit detail, audit search, LLM usage analytics, and management status using only documented JSON fields.
- A catalog item declares `execution_mode`; a reference item can never be presented or invoked as executable.
- Every completed, clarified, blocked, unsupported, failed, or executed job has an ordered durable decision timeline with catalog/version and outcome information.
- Material audit decisions are not silently lost due to the in-memory audit queue; any telemetry loss is counted and exposed by `/management/status`.
- Each LLM call is associated with its job/session when applicable and records provider, model, purpose, tokens, latency, normalized status, and a cost estimate or explicit `cost_estimate_unavailable` warning.
- Context observability records only bounded identifiers/counts/budgets and truncation reasons, never raw prompt/history/document content.
- Management API responses and persisted audit error summaries contain no raw SQL, secrets, API keys, provider credentials, stack traces, or raw prompt/document text.
- All management endpoints reject non-admin callers and use cursor pagination plus bounded time ranges where applicable.
- Archive/delete and retention behavior is documented and covered by tests; no audit record is silently removed as a side effect.
- Existing reporting safety invariants remain unchanged: LLM-generated SQL is never executable, and office filtering remains bound inside approved SQL.

## Dependencies and risks

- Issue 003 (verified payload extraction) defines field-level provenance. This issue must link its final verified payload summary rather than duplicate its internal evidence model.
- Issue 005 (unified clarification contract) defines structured clarification events and safe public summaries.
- The current user-owned chat/auth model must remain the authority; API keys are not management authentication.
- Price estimates require a versioned configured pricing source. Missing or stale pricing must be reported as unavailable/stale, never fabricated.
- A durable outbox may be necessary if audit writes cannot be atomically committed with the material job state transition.

## Links

- `docs/current/status.md`
- `docs/architecture/overview.md`
- `docs/architecture/audit-trail/index.md`
- `docs/knowledge/catalog/09-9-storage-and-refresh-policy.md`
- `docs/knowledge/catalog/10-10-audit-requirements.md`
- `docs/issues/active/003-verified-payload-extraction.md`
- `docs/issues/active/005-unified-agentic-clarification-contract.md`
- `docs/superpowers/specs/2026-07-23-management-observability-and-audit-design.md`
- `docs/superpowers/plans/2026-07-23-management-observability-and-audit.md`
- `migrations/20260709040000_create_chat_job_audit_events.sql`
- `migrations/20260712120000_create_assistant_tables.sql`
- `crates/chat/src/audit/`
