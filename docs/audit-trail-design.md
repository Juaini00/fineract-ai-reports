# Audit Trail Design

This document defines the durable audit trail for chat/report requests. The audit trail is separate from runtime logs and live SSE events. Its job is to make each request explainable after it completes, especially when analyzing whether the system followed `docs/Modern_RAG_Architecture_Blueprint.md`.

## Goals

1. Track every important chat job stage without making the main pipeline wait on audit writes.
2. Persist enough structured data for management, debugging, and blueprint-compliance analysis.
3. Show which layer handled a request: auth, conversation context, classification, retrieval, policy, SQL execution, formatting, and answer generation.
4. Record non-standard paths such as lexical fallback, skipped strict semantic parsing, policy blocks, unsupported requests, and known hardcode risks.
5. Avoid storing secrets, raw API keys, raw embeddings, full SQL result rows, or hidden prompts.

## Non-Goals

1. Do not replace normal `tracing` logs.
2. Do not replace `chat_job_events`, which remains for live progress/SSE and important user-facing job events.
3. Do not make audit zero-loss in the first implementation.
4. Do not build a dashboard in the first implementation.

## Storage Model

Add a new table named `chat_job_audit_events`.

Recommended schema:

```sql
CREATE TABLE chat_job_audit_events (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id),
    session_id UUID NULL REFERENCES chat_sessions(id),
    api_key_id UUID NULL REFERENCES api_keys(id),
    event_type TEXT NOT NULL,
    stage TEXT NOT NULL,
    layer TEXT NOT NULL,
    blueprint_step TEXT NULL,
    status TEXT NOT NULL,
    duration_ms BIGINT NULL,
    input_summary_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    output_summary_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    decision_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    flags_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    error_json JSONB NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_chat_job_audit_events_job_id ON chat_job_audit_events(job_id, created_at);
CREATE INDEX idx_chat_job_audit_events_stage ON chat_job_audit_events(stage, created_at);
CREATE INDEX idx_chat_job_audit_events_blueprint_step ON chat_job_audit_events(blueprint_step, created_at);
CREATE INDEX idx_chat_job_audit_events_api_key_id ON chat_job_audit_events(api_key_id, created_at);
```

`chat_job_audit_events` is append-only. Do not update old audit rows during normal processing.

## Event-Driven Writer

Audit writes must not block the main chat job pipeline.

Use a bounded in-memory `tokio::sync::mpsc` queue and one background writer task:

```text
Job pipeline
  -> AuditHandle.record(event)
  -> bounded mpsc queue
  -> AuditWorker
  -> batch INSERT chat_job_audit_events
```

`AuditHandle::record` must be non-blocking:

```text
try_send(event)
  success        -> return immediately
  queue full     -> drop event, increment dropped counter, warn with tracing
  channel closed -> warn with tracing, continue main flow
```

The worker should batch events:

```text
flush when batch size reaches 50 events
or when 500ms passes since the previous flush
```

If a batch insert fails, retry a small fixed number of times. If it still fails, log a sanitized warning and drop the batch. The main job must not fail because audit persistence failed.

```rust
// ponytail: in-memory audit queue; upgrade to Redis Stream or DB outbox if zero-loss audit is required.
```

## Durability Trade-Off

The first implementation is near-real-time and non-blocking, but not zero-loss. If the process crashes before the worker flushes, recent audit events in memory can be lost.

Upgrade paths:

1. Redis Stream if audit must survive app process crashes without putting DB writes in the request path.
2. DB outbox if audit must be transactionally attached to job state changes.
3. Kafka/NATS/RabbitMQ only if the system grows beyond this service boundary.

For now, do not add a new third-party dependency. Tokio, SQLx, PostgreSQL, and existing tracing are enough.

## Audit Stages

Recommended initial stages:

```text
request_received
auth_context_loaded
conversation_context_built
semantic_parser
classification_started
classification_completed
lqr_planner_started
lqr_planner_completed
flat_retrieval_fallback
lexical_retrieval_fallback
clarification_required
execution_plan_built
policy_evaluated
sql_selected
sql_executed
response_formatted
llm_answer_generation_started
llm_answer_generation_completed
job_completed
job_failed
```

Not every job emits every stage. Missing stages are useful: they reveal which path the request actually took.

## Blueprint Step Mapping

Each audit event should map to the closest blueprint step when possible:

```text
conversation_context
semantic_parser
intent_router
entity_constraint_resolver
ambiguity_detector
retrieval_planner
hybrid_retrieval
reranker
evidence_evaluator
answer_planner
answer_generator
grounded_response
```

When a blueprint step is intentionally skipped in the current implementation, emit an audit event with `status = 'skipped'` and a structured reason:

```json
{
  "reason": "strict_pipeline_not_used_in_production"
}
```

This makes blueprint gaps observable instead of implicit.

## Status Values

Recommended values:

```text
started
completed
skipped
fallback
blocked
failed
```

## Flags

`flags_json` records important analysis hints:

```json
{
  "used_lqr": true,
  "used_flat_retrieval": false,
  "used_lexical_fallback": false,
  "used_llm": true,
  "policy_blocked": false,
  "blueprint_deviation": false,
  "hardcode_risk": false,
  "pii_output_allowed": false,
  "authorized_scope_only": true
}
```

Use `hardcode_risk` for known places where current behavior depends on Fineract-owned constants or deterministic shortcuts. The audit system should not try to statically scan all SQL/code in the first version.

## Safe Payload Rules

Allowed:

```text
job_id
session_id
api_key_id
key_prefix
owner
allowed_office_ids
allowed_capabilities
domain
capability
query_id
confidence
candidate source ids and scores
SQL file path
row_count
duration_ms
sanitized error code/message
```

Not allowed:

```text
raw API key
authorization header
raw embeddings
hidden prompts
full SQL result rows
raw SQL when not needed
secret config values
unmasked PII unless explicitly required and policy-approved
```

The user message already lives in `chat_messages`; audit events should store a compact input summary rather than duplicating full message content by default.

## Relationship To Existing Tables

`chat_jobs` remains the durable job state.

`chat_job_checkpoints` remains for resumability and major state snapshots.

`chat_job_events` remains for live progress/SSE replay and important user-facing events.

`chat_job_audit_events` is for management, flow analysis, and blueprint compliance.

## Example Timeline

```text
request_received                 completed  conversation_context
auth_context_loaded              completed  conversation_context
classification_started           started    intent_router
lqr_planner_started              started    retrieval_planner
lqr_planner_completed            completed  retrieval_planner
classification_completed         completed  intent_router
execution_plan_built             completed  answer_planner
policy_evaluated                 completed  evidence_evaluator
sql_selected                     completed  hybrid_retrieval
sql_executed                     completed  hybrid_retrieval
response_formatted               completed  grounded_response
llm_answer_generation_completed  completed  answer_generator
job_completed                    completed  grounded_response
```

## Management Queries

The audit table should support questions like:

```text
Which jobs used lexical fallback today?
Which jobs skipped semantic parsing?
Which jobs were blocked by policy?
Which capabilities fail most often?
Which stages are slowest?
Which requests took a non-standard path compared with the blueprint?
Which API keys trigger the most unsupported requests?
```

## API Access

The first read endpoint exposes one job's audit timeline:

```text
GET /chat/jobs/{job_id}/audit
```

The endpoint uses the same API key authentication as `GET /chat/jobs/{job_id}`. It only returns audit events for a job owned by the authenticated API key.

Response data:

```json
{
  "job_id": "...",
  "events": [
    {
      "id": "...",
      "job_id": "...",
      "session_id": "...",
      "api_key_id": "...",
      "event_type": "pipeline",
      "stage": "classification_completed",
      "layer": "classification",
      "blueprint_step": "intent_router",
      "status": "completed",
      "duration_ms": null,
      "input_summary_json": {},
      "output_summary_json": {},
      "decision_json": {},
      "flags_json": {},
      "error_json": null,
      "created_at": "..."
    }
  ]
}
```

## First Implementation Scope

Implement only chat job pipeline auditing first. Do not audit `/health`, `/ready`, or simple catalog/status endpoints in the first pass.

First slice:

1. Migration for `chat_job_audit_events`.
2. `AuditEvent`, `AuditHandle`, and background worker in the chat/app composition path.
3. Non-blocking `record` calls at key job stages.
4. Batch insert with bounded queue and sanitized warnings.
5. Tests for enqueue/drop behavior and SQL insert shape where practical.

Later slices can add admin endpoints and HTTP-level request auditing.
