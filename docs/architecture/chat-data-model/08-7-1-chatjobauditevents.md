# Chat Data Model: 7.1. chat_job_audit_events

Source: `docs-old/chat-data-model.md`

## 7.1. chat_job_audit_events

Stores durable management/audit events for pipeline analysis and blueprint compliance. This table is append-only and separate from `chat_job_events` so live progress does not mix with audit evidence.

Detailed schema and write-path rules are in `docs/audit-trail-design.md`.

Purpose:

1. Track which layer handled a job stage.
2. Persist non-standard paths such as fallback, skipped blueprint steps, policy blocks, or hardcode risks.
3. Enable management queries by job, API key, capability, stage, and blueprint step.
4. Keep audit writes out of the main request path through a bounded background queue.
