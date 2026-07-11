# Chat Data Model: 6. chat_job_checkpoints

Source: `docs-old/chat-data-model.md`

## 6. chat_job_checkpoints

Stores durable checkpoints for resumability and audit.

Purpose:

1. Track important job state transitions.
2. Resume from last stable checkpoint.
3. Keep history of major pipeline decisions.
4. Avoid overwriting all state without trace.

Schema:

```sql
CREATE TABLE chat_job_checkpoints (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id),
    step TEXT NOT NULL,
    checkpoint_type TEXT NOT NULL,
    state_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Checkpoint types:

```text
job_created
step_started
step_completed
clarification_required
user_response_received
decision_completed
query_completed
response_completed
job_failed
job_cancelled
```

Do not checkpoint every heartbeat or minor progress update.
