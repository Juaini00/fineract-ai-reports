# Chat Data Model: 7. chat_job_events

Source: `docs-old/chat-data-model.md`

## 7. chat_job_events

Stores important stream/live-progress events.

Purpose:

1. Keep durable event history for important user-facing events.
2. Allow replay of final/clarification/error events.
3. Support debugging.

Schema:

```sql
CREATE TABLE chat_job_events (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id),
    event_type TEXT NOT NULL,
    step TEXT NULL,
    payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Event types:

```text
status
clarification
partial_result
final
error
heartbeat
```

Persist only important events:

```text
clarification
final
error
major status changes
```

Heartbeat and frequent live progress events can stay in Redis only.
