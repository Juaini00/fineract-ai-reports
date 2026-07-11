# Audit Trail Design: Relationship To Existing Tables

Source: `docs-old/audit-trail-design.md`

## Relationship To Existing Tables

`chat_jobs` remains the durable job state.

`chat_job_checkpoints` remains for resumability and major state snapshots.

`chat_job_events` remains for live progress/SSE replay and important user-facing events.

`chat_job_audit_events` is for management, flow analysis, and blueprint compliance.
