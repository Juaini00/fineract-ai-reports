# Audit Trail Design: First Implementation Scope

Source: `docs-old/audit-trail-design.md`

## First Implementation Scope

Implement only chat job pipeline auditing first. Do not audit `/health`, `/ready`, or simple catalog/status endpoints in the first pass.

First slice:

1. Migration for `chat_job_audit_events`.
2. `AuditEvent`, `AuditHandle`, and background worker in the chat/app composition path.
3. Non-blocking `record` calls at key job stages.
4. Batch insert with bounded queue and sanitized warnings.
5. Tests for enqueue/drop behavior and SQL insert shape where practical.

Later slices can add admin endpoints and HTTP-level request auditing.
