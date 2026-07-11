# Chat Data Model: 9. Clarification Flow State

Source: `docs-old/chat-data-model.md`

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
