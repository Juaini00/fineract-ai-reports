# Chat Data Model: 10. Endpoint Relationship

Source: `docs-old/chat-data-model.md`

## 10. Endpoint Relationship

Recommended endpoints:

```text
POST /chat/sessions
GET  /chat/sessions
GET  /chat/sessions/{session_id}
PATCH /chat/sessions/{session_id}
DELETE /chat/sessions/{session_id}
GET  /chat/sessions/{session_id}/messages

POST /chat/jobs
GET  /chat/jobs/{job_id}
GET  /chat/jobs/{job_id}/audit
GET  /chat/jobs/{job_id}/stream
POST /chat/jobs/{job_id}/responses
```

`PATCH /chat/sessions/{session_id}` requires `{ "title": "..." }`, trims the
title, validates 1-120 Unicode characters, and returns the updated session.
`DELETE /chat/sessions/{session_id}` soft-archives immediately and returns
`{ "session_id": "...", "deleted": true }`. Missing, foreign, and already
archived sessions are indistinguishable (`404`).

Archived sessions are excluded from client-visible session, message, and job
access, including new SSE connections and clarification responses. A synchronous
job already in flight may finish internally; deletion does not force cancellation
or physically remove persisted history.

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
