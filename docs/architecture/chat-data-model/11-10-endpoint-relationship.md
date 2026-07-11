# Chat Data Model: 10. Endpoint Relationship

Source: `docs-old/chat-data-model.md`

## 10. Endpoint Relationship

Recommended endpoints:

```text
POST /chat/sessions
GET  /chat/sessions/{session_id}
GET  /chat/sessions/{session_id}/messages

POST /chat/jobs
GET  /chat/jobs/{job_id}
GET  /chat/jobs/{job_id}/audit
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
