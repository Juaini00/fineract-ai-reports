# Audit Trail Design: Event-Driven Writer

Source: `docs-old/audit-trail-design.md`

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
