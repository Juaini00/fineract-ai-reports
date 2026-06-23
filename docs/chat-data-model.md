# Chat Data Model

This document defines the data model for chat sessions, chat messages, chat jobs, checkpoints, and live progress state.

The system must support long-running report generation, clarification flows, SSE progress updates, and resumable jobs without relying on in-memory state as the source of truth.

## 1. Storage Principles

Use three storage layers with different responsibilities:

```text
PostgreSQL -> durable source of truth
Redis      -> live progress / temporary coordination
Memory     -> temporary data during the current request only
```

Rules:

1. If state is required to resume a job, store it in PostgreSQL.
2. If state is only for live UI progress, store it in Redis.
3. Do not hold DB connections while waiting for user input or streaming SSE.
4. Do not store raw API keys.
5. Do not store raw SQL or hidden prompts in user-visible payloads.
6. Keep large report results out of chat messages; store them as job result payloads or report result records.

## 2. Main Entities

The chat system needs these durable tables:

```text
chat_sessions
chat_messages
chat_jobs
chat_job_checkpoints
chat_job_events
```

Existing auth table:

```text
api_keys
```

## 3. chat_sessions

Represents a conversation context for one API key/client.

Purpose:

1. Group multiple messages together.
2. Preserve lightweight conversation context.
3. Allow follow-up questions.
4. Track session lifecycle.

Schema:

```sql
CREATE TABLE chat_sessions (
    id UUID PRIMARY KEY,
    api_key_id UUID NOT NULL REFERENCES api_keys(id),
    title TEXT NULL,
    status TEXT NOT NULL,
    context_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NULL,
    archived_at TIMESTAMPTZ NULL
);
```

Recommended statuses:

```text
active
archived
expired
```

`context_json` should store compact context only, for example:

```json
{
  "last_domain": "savings",
  "last_capability": "savings_deposit_total",
  "last_params": {
    "from_date": "2026-01-01",
    "to_date": "2026-05-31"
  },
  "last_result_summary": "Total deposit was IDR 920,000,000"
}
```

Do not store full prompt history in `context_json`.

## 4. chat_messages

Stores user and assistant messages.

Purpose:

1. Keep chat transcript.
2. Link messages to sessions.
3. Link assistant responses to jobs.
4. Support audit/debugging.

Schema:

```sql
CREATE TABLE chat_messages (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES chat_sessions(id),
    job_id UUID NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Roles:

```text
user
assistant
system
tool
clarification
```

Example user message:

```json
{
  "role": "user",
  "content": "Show savings data from January to May 2026"
}
```

Example assistant message:

```json
{
  "role": "assistant",
  "content": "Apakah Anda ingin total gabungan atau per bulan?",
  "metadata_json": {
    "type": "clarification",
    "response_key": "output_mode"
  }
}
```

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

## 7. chat_job_events

Stores important stream/audit events.

Purpose:

1. Keep durable event history for important events.
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

## 9. Clarification Flow State

Example ambiguous request:

```text
Show savings data from January to May 2026
```

The system detects ambiguity:

```text
total combined vs monthly breakdown
```

Persist in `chat_jobs.state_json`:

```json
{
  "known": {
    "domain": "savings",
    "period": {
      "from": "2026-01-01",
      "to": "2026-05-31"
    }
  },
  "pending_clarification": {
    "response_key": "output_mode",
    "question": "Do you want a combined total or monthly breakdown?",
    "options": [
      {"label": "Combined total", "value": "total"},
      {"label": "Monthly breakdown", "value": "monthly_breakdown"}
    ]
  },
  "resume_from": "taking_decision"
}
```

Job update:

```text
status = waiting_for_user_input
current_step = checking_context
resume_from_step = taking_decision
```

SSE sends:

```text
event: clarification
```

Then the stream can close.

Client responds with:

```text
POST /chat/jobs/{job_id}/responses
```

Body:

```json
{
  "response_key": "output_mode",
  "value": "monthly_breakdown"
}
```

Server merges response into `state_json`, changes status back to `queued` or `running`, and resumes from `resume_from_step`.

## 10. Endpoint Relationship

Recommended endpoints:

```text
POST /chat/sessions
GET  /chat/sessions/{session_id}
GET  /chat/sessions/{session_id}/messages

POST /chat/jobs
GET  /chat/jobs/{job_id}
GET  /chat/jobs/{job_id}/stream
POST /chat/jobs/{job_id}/responses
```

`POST /chat/jobs` may create a session automatically if no `session_id` is provided.

Request:

```json
{
  "session_id": null,
  "message": "Who made the largest deposit today?"
}
```

Response:

```json
{
  "success": true,
  "data": {
    "session_id": "...",
    "job_id": "..."
  },
  "error": null
}
```

## 11. Indexes

Recommended indexes:

```sql
CREATE INDEX idx_chat_sessions_api_key_id ON chat_sessions(api_key_id);
CREATE INDEX idx_chat_sessions_status ON chat_sessions(status);

CREATE INDEX idx_chat_messages_session_id ON chat_messages(session_id);
CREATE INDEX idx_chat_messages_job_id ON chat_messages(job_id);

CREATE INDEX idx_chat_jobs_session_id ON chat_jobs(session_id);
CREATE INDEX idx_chat_jobs_api_key_id ON chat_jobs(api_key_id);
CREATE INDEX idx_chat_jobs_status ON chat_jobs(status);
CREATE INDEX idx_chat_jobs_expires_at ON chat_jobs(expires_at);

CREATE INDEX idx_chat_job_checkpoints_job_id ON chat_job_checkpoints(job_id);
CREATE INDEX idx_chat_job_checkpoints_job_step ON chat_job_checkpoints(job_id, step);

CREATE INDEX idx_chat_job_events_job_id ON chat_job_events(job_id);
CREATE INDEX idx_chat_job_events_job_type ON chat_job_events(job_id, event_type);
```

## 12. Retention Policy

Suggested initial retention:

```text
active sessions: until expired or archived
chat messages: 30-90 days depending on policy
completed jobs: 7-30 days
failed jobs: 7-30 days
checkpoints: same as job retention
events: same as job retention
redis live state: 15-60 minutes
```

Retention should be configurable later.

## 13. Important Rules

1. PostgreSQL stores durable state only.
2. Redis stores live progress only.
3. Memory is never the source of truth.
4. Save PostgreSQL checkpoints only at important boundaries.
5. SSE events are not the source of truth.
6. A job must be resumable from PostgreSQL after app restart.
7. Clarification must preserve job state and continue the same job.
8. Do not create a new job when answering clarification.
9. Do not hold DB connections during SSE idle time.
10. Do not store raw API keys, raw SQL, or internal prompts in chat-visible payloads.
