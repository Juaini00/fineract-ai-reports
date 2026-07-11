# Audit Trail Design: API Access

Source: `docs-old/audit-trail-design.md`

## API Access

The first read endpoint exposes one job's audit timeline:

```text
GET /chat/jobs/{job_id}/audit
```

The endpoint uses the same API key authentication as `GET /chat/jobs/{job_id}`. It only returns audit events for a job owned by the authenticated API key.

Response data:

```json
{
  "job_id": "...",
  "events": [
    {
      "id": "...",
      "job_id": "...",
      "session_id": "...",
      "api_key_id": "...",
      "event_type": "pipeline",
      "stage": "classification_completed",
      "layer": "classification",
      "blueprint_step": "intent_router",
      "status": "completed",
      "duration_ms": null,
      "input_summary_json": {},
      "output_summary_json": {},
      "decision_json": {},
      "flags_json": {},
      "error_json": null,
      "created_at": "..."
    }
  ]
}
```
