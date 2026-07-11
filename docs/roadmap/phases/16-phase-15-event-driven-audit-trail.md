# Implementation Steps: Phase 15: Event-Driven Audit Trail

Source: `docs-old/implementation-steps.md`

## Phase 15: Event-Driven Audit Trail

Goal: make every chat job traceable without putting audit DB writes in the critical path. See `docs/audit-trail-design.md`.

Current status:

```text
DESIGNED (2026-07-09)
```

Audit storage:

```text
chat_job_audit_events
```

Core fields:

```text
job_id
session_id
api_key_id
event_type
stage
layer
blueprint_step
status
latency_ms
input_summary_json
output_summary_json
decision_json
flags_json
error_json
created_at
```

Do not log raw API keys.

Avoid logging sensitive result data unless explicitly needed and policy-approved.

Write path:

```text
main job pipeline -> non-blocking AuditHandle::record -> bounded tokio mpsc queue -> background batch writer -> PostgreSQL
```

Important rule:

```text
audit persistence failure must not fail or delay the main chat job
```

First implementation scope:

```text
chat job pipeline only; do not audit health/readiness/simple status endpoints yet
```
