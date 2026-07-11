# Chat Data Model: 8. Redis Live State

Source: `docs-old/chat-data-model.md`

## 8. Redis Live State

Redis is used for live, temporary progress state.

Suggested keys:

```text
chat_job:{job_id}:live_state
chat_job:{job_id}:latest_event
chat_job:{job_id}:lock
```

Example live state:

```json
{
  "job_id": "...",
  "status": "running",
  "current_step": "embedding",
  "message": "Finding relevant reporting knowledge...",
  "updated_at": "2026-06-14T12:00:00Z"
}
```

TTL recommendations:

```text
live_state: 15-60 minutes
latest_event: 15-60 minutes
lock: 30-120 seconds
```

Redis is not source of truth. If Redis data is lost, the job must still be recoverable from PostgreSQL.
