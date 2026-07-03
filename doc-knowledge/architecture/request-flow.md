---
type: Architecture
title: Request Flow — from chat message to report
description: The lifecycle of one `POST /chat/messages` call — components touched, decisions made, durability contract, and where each guard runs.
tags: [architecture, request-flow, chat]
---

# Request flow

## Overview

```
Client                 axum handler        JobService           Pipeline (tokio::spawn)          Postgres / Redis / Fineract
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
POST /chat/messages ─▶ chat::api::handlers ─▶ JobService::create ─┬─▶ classifier ─▶ planner ─▶ policy ─▶ executor ─▶ formatter ─▶ emit_event ┐
                                                                  │                                                                          │
                                                                  └─▶ chat_jobs INSERT + first chat_job_events (Postgres, source of truth)   │
                                                                                                                                             │
GET /chat/jobs/{id}/stream (SSE) ◀───────────────────────── Redis chat_job:{id}:latest_event (1h TTL, mirror only) ◀──────────── emit_event   ┘
```

## 1 · HTTP entry

- `POST /chat/messages` hits an axum handler under `chat::api::handlers`.
- Body validation uses the shared `ValidatedJson<T>` extractor (`validator` derive) — no per-route hand-rolling.
- Authentication uses the generic `AuthenticatedClient` extractor from `core::auth`. It resolves an API key (either `Authorization: Bearer …` or `X-API-Key: …`) into a `ClientContext { client_id, allowed_office_ids, can_view_pii, … }`.
- All responses go through the `{ success, data, error }` envelope. Any error path renders `ApiError` — no raw serde / sqlx / stack traces leak. See [policies/query-safety](../policies/query-safety.md) principle: **never leak internals**.

## 2 · Session & job creation

- The handler calls `JobService::create(client_ctx, session_id_or_none, user_message)`.
- Inside `ChatRepository::create_job` a single transaction: (a) auto-creates the `chat_session` if missing, (b) inserts the `chat_message`, (c) inserts the `chat_job` in `queued` state, (d) writes the first `chat_job_events` row. Durable Postgres state is authoritative.
- The service returns the `job_id` immediately (200); the pipeline continues **asynchronously** via `tokio::spawn`.

## 3 · Classifier — natural language → structured intent

Location: `crates/chat/src/chat/classifier.rs`.

- Reads the user prompt and calls `KnowledgeRepository::search_context` against the vector index.
- Vector index contains **capabilities, metrics, data areas, domains, and query descriptors** (rebuilt at startup or via `POST /vector-index/rebuild`). Non-capability retrieval rows also land in `chat_jobs.state_json.classification.candidates` for audit and downstream LLM grounding.
- Emits a structured `Classification { intent, candidate_capabilities, extracted_params, output_mode_hint, requires_date_range, ... }`. Bilingual (EN + ID) prompt phrasing is handled here; example vocabulary lives in each capability's `examples` field.

## 4 · Planner — pick a capability, build an execution plan

Location: `crates/chat/src/chat/planner.rs`. Optional LLM fallback in `chat/llm.rs`.

Decisions:

1. **Rule-based match first** — if exactly one approved capability's `examples` / `supported_intents` match the classification, pick it.
2. **LLM fallback** — if ambiguous, call the constrained OpenAI-compatible planner (`chat::llm`) which must return one of a fixed enum: `execute | clarify | unsupported`. Prompts never include SQL — the LLM never authors queries.
3. **Clarify** — if any of the capability's `required_parameters` are missing (e.g. no `from_date` for `savings.deposit_total`), the planner emits a clarification template from [responses/clarification](../responses/clarification.md). The job stays alive — client answers via `POST /chat/jobs/{job_id}/responses`, which resumes the same pipeline.
4. **Unsupported** — deferred domains, out-of-scope areas, and hard-reject cases route to [responses/unsupported](../responses/unsupported.md). Job ends in `failed_unsupported`.

Output: `ExecutionPlan { capability_id, query_id, bound_params, output_mode }`.

## 5 · Policy — the guard before execution

Location: `crates/chat/src/chat/policy/authorization.rs`, called via `chat::chat::planner::evaluate_policy`.

Runs **before** any SQL touches the database:

- **[office_scope](../policies/office-scope.md)** — validates the requested `office_ids` (if any) is a subset of `client_ctx.allowed_office_ids`. If omitted, injects all allowed offices. Populates the `office_ids` bound parameter for the SQL.
- **[pii](../policies/pii.md)** — checks capability's PII contract vs. `client_ctx.can_view_pii`. Removes conditional PII fields from the plan when the caller can't view PII.
- **[query_safety](../policies/query-safety.md)** — capability must reference a `query_id` present in the approved catalog. No arbitrary SQL. No AI-authored SQL at runtime.
- **[execution_limits](../policies/execution-limits.md)** — validates `max_date_range_days`, `max_rows`, projected timeout.
- **[unsupported_requests](../policies/unsupported-requests.md)** — final catchall for hard-reject categories missed upstream.

Any guard failure → `PolicyDecision::deny` → formatter emits an unsupported / clarification response.

## 6 · Executor — bind, prepare, run

Location: `crates/chat/src/chat/executor.rs`.

- Resolves `query_id` to a compiled SQL statement from `KnowledgeSyncService`'s in-memory catalog (loaded at startup from `queries/**/*.sql` + `knowledge/queries/**/*.yaml`).
- Runs `validate_runtime`: `PREPARE` the SQL against Fineract, compare returned output columns against the declared `output_fields` order — this is where a drifted SQL/YAML gets caught late.
- Binds parameters positionally: `from_date, to_date, office_ids[], currency_code?, product_ids[]?, limit?`. Only bound parameters — never string interpolation.
- Executes with `timeout_ms` from the query concept. Reads via the **Fineract read-only replica pool** (`FINERACT_DATABASE_URL`), never the app DB.
- Returns rows typed against `output_fields`. PII-sensitive rows already have conditional fields stripped by the policy pass.

## 7 · Formatter — rows → user-facing text

Location: `crates/chat/src/chat/formatter.rs`.

- Selects a template from [responses/reporting](../responses/reporting.md) based on `output_mode` (`summary`, `total`, `top_n`, `monthly_breakdown`, `monthly_top_n`).
- Applies `field_labels` and formatting rules (ISO 4217 currency, `YYYY-MM-DD` dates, preserve DB decimal precision).
- Uses **only** fields declared in the capability's `output_fields.public` / `pii_conditional`; never invents columns.
- Never mentions SQL, prompts, stack traces, or internal ids.

## 8 · Event emission & SSE

- Each pipeline state transition (`queued → running → completed | failed | needs_clarification`) is written to `chat_job_events` in Postgres — **durable source of truth**.
- `JobService::emit_event` also mirrors the latest event to Redis `chat_job:{id}:latest_event` (1 hour TTL) and `chat_job:{id}:live_state` for the SSE consumer.
- `GET /chat/jobs/{id}/stream` polls Redis at 1s, stopping when `live_state ∈ {completed, failed}`. If Redis is unreachable, SSE degrades but nothing is lost — the job continues in Postgres.

## 9 · Clarification loop

- Same job, same `state_json`, same pipeline. Client posts the answer to `POST /chat/jobs/{job_id}/responses`.
- Handler appends a `chat_message`, calls `JobService::respond`, which merges the new params into the plan and re-enters step 5 (policy) → 6 (executor).
- Never spawn a new job for clarification. The `chat_job_id` in the SSE stream stays the same.

## 10 · Durability contract summary

| Where | What lives here | Truth? |
|---|---|---|
| Postgres `chat_sessions`, `chat_messages`, `chat_jobs`, `chat_job_checkpoints`, `chat_job_events` | conversation, job state, event log | ✅ source of truth |
| Redis `chat_job:{id}:live_state / :latest_event / :lock` | SSE coordination, worker leader locks | ❌ ephemeral; ttl 1h |
| Fineract replica (read-only) | business data (offices, clients, accounts, transactions) | ✅ source of truth for business |
| App DB via migrations | schema owner for app-owned tables | ✅ managed by `migrations/*.sql` — never `CREATE TABLE` at startup |

## 11 · Where each concept plugs in

| Step | Concept |
|---|---|
| 2. Auth | [glossary](../glossary.md#auth), API key + `allowed_office_ids` |
| 3. Classifier | vector index over [capabilities](../capabilities/index.md), [metrics](../metrics/index.md), [domains](../domains/index.md), [data-areas](../data-areas/index.md) |
| 4. Planner | [capabilities](../capabilities/index.md) with `examples`, `required_parameters`, `output_mode` |
| 5. Policy | [policies/](../policies/index.md) — all five guards |
| 6. Executor | [queries/](../queries/index.md) — SQL + `output_fields` contract |
| 7. Formatter | [responses/reporting](../responses/reporting.md) |
| 4/5 fall-through | [responses/clarification](../responses/clarification.md) or [responses/unsupported](../responses/unsupported.md) |
