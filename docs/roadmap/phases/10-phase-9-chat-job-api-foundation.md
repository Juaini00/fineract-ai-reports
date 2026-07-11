# Implementation Steps: Phase 9: Chat Job API Foundation

Source: `docs-old/implementation-steps.md`

## Phase 9: Chat Job API Foundation

Goal: create authenticated chat job endpoints before knowledge/planner/report execution.

Crate placement:

```text
crates/chat
```

Rules:

1. Use the crate name `chat`, not `ai_report_chat`.
2. Keep `core` as shared foundation for config, DB pools, API primitives, auth, and response/error types.
3. Keep chat session/job service, repository, and future pipeline orchestration inside `chat`.
4. Do not create `knowledge` or `reporting` crates for this phase.

Endpoints:

```text
POST /chat/sessions
GET  /chat/sessions/{session_id}
GET  /chat/sessions/{session_id}/messages

POST /chat/jobs
GET  /chat/jobs/{job_id}
GET  /chat/jobs/{job_id}/stream
POST /chat/jobs/{job_id}/responses
```

Rules:

1. All endpoints require API key authentication.
2. Job ownership must be checked by `api_key_id`.
3. `POST /chat/jobs` may create a session if no `session_id` is provided.
4. Clarification responses must use `POST /chat/jobs/{job_id}/responses`, not a new job.
5. SSE should stream high-level safe events only.
6. Keep route -> handler -> service -> repository -> database boundaries.

Current status:

```text
PARTIALLY DONE

Implemented:
POST /chat/sessions
GET  /chat/sessions/{session_id}
GET  /chat/sessions/{session_id}/messages
POST /chat/jobs
GET  /chat/jobs/{job_id}
GET  /chat/jobs/{job_id}/stream
POST /chat/jobs/{job_id}/responses

Current module layout:
crates/chat/src/api      = routes, handlers, DTOs
crates/chat/src/chat     = model, repository, service
crates/chat/src/policy   = authorization guard helpers

Background worker:
POST /chat/jobs and POST /chat/jobs/{job_id}/responses now insert + emit the queued event, then spawn the pipeline (clarification / execute / fail) via tokio::spawn, so the HTTP call returns immediately.
JobService::run_pipeline is the shared async worker entry point used by both create and respond.

Redis-backed SSE:
JobService::emit_event writes every event durably to PostgreSQL (chat_job_events) AND publishes a best-effort snapshot to Redis key chat_job:{job_id}:latest_event with a 1h TTL. Terminal events (final/error) also set chat_job:{job_id}:live_state to completed/failed.
GET /chat/jobs/{job_id}/stream now polls Redis every 1s, emits an SSE "update" frame on each tick, and stops when live_state is completed/failed or after a 120s safety window. When Redis is disabled it falls back to the previous single PostgreSQL snapshot frame.

Still pending for this phase:
broader chat_job_checkpoints writes at additional pipeline boundaries (currently queued, clarification_required, response_completed, job_failed)
:lock key for multi-instance fairness (single-process worker is fine for MVP)
PubSub fan-out for sub-second SSE latency (polling at 1s is sufficient for current UX)
```
