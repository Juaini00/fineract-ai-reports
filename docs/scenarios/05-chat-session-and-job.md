# 05 — Chat Session, Happy-Path Job, SSE

**Phase covered:** Phase 8 (durable state) + Phase 9 (background worker + Redis SSE) + Phase 12–14 (classifier + plan + executor) + Phase 16 (template formatter).
**Precondition:** `API_KEY` from `02`. Knowledge catalog synced (`04`). Fineract DB has `m_savings_account_transaction` rows.

## Test status

✅ Passed on 2026-06-28 rerun via Postman MCP runner.

- Session creation returned HTTP 201.
- Job creation returned HTTP 201 with `data.job_id` and `status=queued`.
- SSE stream returned HTTP 200 and included a final event.
- Final job state returned `status=completed` and `result_json.row_count > 0`.
- Session messages included both `user` and `assistant` roles.
- Top-N month variant returned `client_id` and `client_display_name` in result rows.

## 1. Create a session

```bash
curl -X POST {{BASE_URL}}/chat/sessions \
  -H "Authorization: Bearer {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{ "title": "deposits Q3" }'
```

### Expected (HTTP 201)
```json
{
  "success": true,
  "data": { "id": "<uuid>", "title": "deposits Q3", "created_at": "<rfc3339>" },
  "error": null
}
```

Copy `data.id` into `{{SESSION_ID}}`.

## 2. Create a job (total)

```bash
curl -X POST {{BASE_URL}}/chat/jobs \
  -H "Authorization: Bearer {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{
    "session_id": "{{SESSION_ID}}",
    "message": "What is the total deposit this month?"
  }'
```

### Expected (HTTP 201)
```json
{
  "success": true,
  "data": {
    "job_id": "<uuid>",
    "session_id": "{{SESSION_ID}}",
    "user_message_id": "<uuid>",
    "status": "queued",
    "current_step": "queued"
  },
  "error": null
}
```

Copy `data.job_id` into `{{JOB_ID}}`. **HTTP returns immediately** — the background `tokio::spawn` worker runs classification → planning → policy → execute and emits progress events as it goes.

## 3. Stream progress (SSE)

```bash
curl -N {{BASE_URL}}/chat/jobs/{{JOB_ID}}/stream \
  -H "Authorization: Bearer {{API_KEY}}"
```

### Expected event sequence
```text
event: status
data: {"job_id":"<uuid>","status":"queued","current_step":"queued"}

event: update
data: {"kind":"status","step":"queued","payload":{...},"at":"<rfc3339>"}

event: update
data: {"kind":"final","step":"response","payload":{"status":"completed","row_count":1,"latency_ms":<n>},"at":"<rfc3339>"}
```

Stream stops when `chat_job:{job_id}:live_state` is `completed` or `failed`, or after a 120s safety window. With Redis disabled it falls back to a single PG snapshot frame.

## 4. Read final job state

```bash
curl {{BASE_URL}}/chat/jobs/{{JOB_ID}} -H "Authorization: Bearer {{API_KEY}}"
```

### Expected (HTTP 200)
```json
{
  "success": true,
  "data": {
    "id": "{{JOB_ID}}",
    "status": "completed",
    "current_step": "response",
    "result_json": {
      "query_id": "savings.deposit_total",
      "row_count": 1,
      "rows": [{ "from_date": "...", "to_date": "...", "total_deposit_amount": "...", "deposit_count": <n> }],
      "latency_ms": <n>
    },
    "state_json": {
      "classification": { "outcome": "matched", "capability": "savings_deposit_total", "source": "vector", "candidates": [...] },
      "execution_plan": {...},
      "policy_decision": { "status": "allowed", "office_ids": [1,2,3] }
    }
  },
  "error": null
}
```

`state_json.classification.candidates` now contains BOTH capability rows (decision input) AND non-capability context rows (data_area, domain, query) tagged with `source_type` (Phase 18 broader retrieval).

## 5. Read messages

```bash
curl {{BASE_URL}}/chat/sessions/{{SESSION_ID}}/messages \
  -H "Authorization: Bearer {{API_KEY}}"
```

### Expected
- `role: "user"` row with the original question.
- `role: "assistant"` row with the template response (Phase 16 formatter).

## Top-N variant

Same flow with `message: "Show the largest deposits today"` → classifier picks `savings_deposit_top_n`, plan binds `limit`, executor returns rows ordered by amount.

✅ Passed on 2026-06-28 rerun with `message: "Show the largest deposits this month"`. The `today` variant completed but returned `row_count=0` in the local dataset, so the month variant was used to verify row fields.

After the knowledge expansion the `top_n` output contract includes client identity (`LEFT JOIN m_client`):

```json
{
  "transaction_id": <n>,
  "transaction_date": "...",
  "amount": "...",
  "currency_code": "...",
  "office_id": <n>,
  "office_name": "...",
  "product_id": <n>,
  "product_name": "...",
  "client_id": <n>,
  "client_display_name": "..."   // PII — sensitivity: pii in output_fields
}
```

`client_id` is `NULL` for group-owned accounts (LEFT JOIN). `client_display_name` is a PII field per the YAML output contract; Phase 16 expansion will mask it when `client.can_view_pii=false`. Current formatter template does not surface it yet.

## Side effects
- DB `chat_jobs`: status `queued → running → completed`. `state_json` populated; `result_json` written on success.
- DB `chat_job_checkpoints`: `job_created` (queued), `response_completed` (response).
- DB `chat_job_events`: `status` (queued), `final` (response).
- DB `chat_messages`: user + assistant rows.
- Redis `chat_job:{id}:latest_event`: JSON snapshot of the most recent event, TTL 1h.
- Redis `chat_job:{id}:live_state`: `"completed"` after success, `"failed"` after error.

## Failure modes

| Trigger | Expected end-state |
| --- | --- |
| Fineract DB down mid-job | job `failed`, error event `code=execution_failed` |
| Wrong API key | HTTP 401 |
| Job belongs to a different API key | HTTP 404 on `/chat/jobs/{id}` |
| Redis down | SSE falls back to single PG snapshot; PG events still recorded |
