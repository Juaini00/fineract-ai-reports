# Implementation Steps: Phase 8: Chat Session And Job Data Model

Source: `docs-old/implementation-steps.md`

## Phase 8: Chat Session And Job Data Model

Goal: create durable chat/session/job state before implementing chatbot pipeline.

Reference design:

```text
docs/chat-data-model.md
```

Tables:

```text
chat_sessions
chat_messages
chat_jobs
chat_job_checkpoints
chat_job_events
```

Storage rule:

```text
PostgreSQL = durable checkpoints and chat history
Redis = live progress state and temporary SSE coordination
Memory = transient only
```

Required job statuses:

```text
queued
running
waiting_for_user_input
completed
failed
expired
cancelled
```

Initial pipeline steps:

```text
queued
checking_context
embedding
taking_decision
response
```

Redis live keys:

```text
chat_job:{job_id}:live_state
chat_job:{job_id}:latest_event
chat_job:{job_id}:lock
```

Checkpoint policy:

```text
save PostgreSQL checkpoints only at important boundaries
do not save every progress/heartbeat update to PostgreSQL
```

Validation:

```bash
sqlx migrate run
```

Expected result:

```text
chat session/job tables exist
indexes exist
```

Current status:

```text
DONE: migration 20260617130000_create_chat_tables.sql creates chat sessions, messages, jobs, checkpoints, events, and indexes.
DONE: migration 20260709030000_add_chat_job_state_revision.sql adds chat_jobs.state_revision for optimistic locking of per-job memory.
```
