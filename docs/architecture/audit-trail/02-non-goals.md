# Audit Trail Design: Non-Goals

Source: `docs-old/audit-trail-design.md`

## Non-Goals

1. Do not replace normal `tracing` logs.
2. Do not replace `chat_job_events`, which remains for live progress/SSE and important user-facing job events.
3. Do not make audit zero-loss in the first implementation.
4. Do not build a dashboard in the first implementation.
