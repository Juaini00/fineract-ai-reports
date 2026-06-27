# 06 — Clarification + Unsupported

**Phase covered:** Phase 12–13 decision policy (`unsupported_threshold`, `clarify_score`, close-candidate margin).
**Precondition:** Same as `05`.

## A. Clarification — ambiguous deposit question

```bash
curl -X POST {{BASE_URL}}/chat/jobs \
  -H "Authorization: Bearer {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{ "session_id": "{{SESSION_ID}}", "message": "deposits this month" }'
```

### Expected job end-state
```json
{
  "status": "waiting_for_user_input",
  "current_step": "taking_decision",
  "state_json": {
    "classification": {
      "outcome": "clarification_required",
      "options": [
        { "label": "...", "capability": "savings_deposit_total" },
        { "label": "...", "capability": "savings_deposit_top_n" }
      ],
      "source": "vector"
    }
  }
}
```

### SSE
```text
event: update
data: {"kind":"clarification","step":"taking_decision","payload":{"options":[...]}}
```

Assistant message inserted with `metadata_json.type = "clarification"` and the options list.

## A.1 Respond to clarification

User picks option 1 (option text, capability id, or 1-based number all accepted):

```bash
curl -X POST {{BASE_URL}}/chat/jobs/{{JOB_ID}}/responses \
  -H "Authorization: Bearer {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{ "message": "1" }'
```

### Expected
- HTTP 200 with the inserted user message.
- Background pipeline re-runs `classify_clarification_response` → builds plan → executes. **Same `JOB_ID` continues** — no new job is created.
- Final SSE `update` event reaches `final` step with `status: completed`.

## B. Unsupported — write intent

```bash
curl -X POST {{BASE_URL}}/chat/jobs \
  -H "Authorization: Bearer {{API_KEY}}" \
  -d '{ "session_id": "{{SESSION_ID}}", "message": "create a new savings account" }'
```

### Expected job end-state
```json
{
  "status": "failed",
  "state_json": {
    "classification": {
      "outcome": "unsupported",
      "source": "write_intent",
      "candidates": []
    }
  },
  "error_json": {
    "code": "unsupported_request",
    "message": "No approved reporting capability matched this request."
  }
}
```

The write-intent guard rejects **before** spending any embedding tokens.

## C. Unsupported — no allowed capability

Repeat with an API key whose `allowed_capabilities` is `[]`. Classification short-circuits with `source: "no_allowed_capabilities"`.

## D. Unsupported — low confidence

A nonsense message such as `"banana"` → embedding runs, but top capability distance falls below `0.40` confidence → job `failed` with `source: "vector_no_match"`.

## Failure modes / edge cases

| Trigger | Expected |
| --- | --- |
| Clarification respond with `"3"` when only 2 options | Re-clarifies (or unsupported), no execution |
| Respond to a completed/failed job | HTTP 409 or 404 depending on path |
| Respond with empty `message` | HTTP 400 validation error |
