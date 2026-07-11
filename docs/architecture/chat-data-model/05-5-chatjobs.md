# Chat Data Model: 5. chat_jobs

Source: `docs-old/chat-data-model.md`

## 5. chat_jobs

Represents a processing job for one user message.

Purpose:

1. Track long-running chat/report process.
2. Allow SSE streaming by job id.
3. Resume after clarification.
4. Resume after SSE reconnect.
5. Store final result/error references.

Schema:

```sql
CREATE TABLE chat_jobs (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES chat_sessions(id),
    api_key_id UUID NOT NULL REFERENCES api_keys(id),
    user_message_id UUID NULL REFERENCES chat_messages(id),
    status TEXT NOT NULL,
    current_step TEXT NOT NULL,
    resume_from_step TEXT NULL,
    message TEXT NOT NULL,
    state_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    state_revision BIGINT NOT NULL DEFAULT 0,
    result_json JSONB NULL,
    error_json JSONB NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ NULL,
    failed_at TIMESTAMPTZ NULL,
    cancelled_at TIMESTAMPTZ NULL
);
```

`state_json` stores the durable working state for the job. `state_revision` is an optimistic-lock version used when updating that state across clarification turns. See `docs/job-memory.md` for the required per-job memory contract.

Statuses:

```text
queued
running
waiting_for_user_input
completed
failed
expired
cancelled
```

Initial `current_step`:

```text
queued
```

Pipeline steps:

```text
checking_context
embedding
taking_decision
response
```

Later report-specific steps:

```text
authorizing
estimating_cost
executing_query
shaping_result
formatting_response
```
