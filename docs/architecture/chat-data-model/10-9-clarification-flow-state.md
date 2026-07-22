# Chat Data Model: 9. Clarification Flow State

A waiting clarification is authoritative, durable, and **job-scoped**. It is stored with the job's assistant memory/pending clarification, not in session memory. A session-level projection may remain during compatibility cleanup, but it is not authority and must not be used to answer a job.

## Public waiting response

`chat_jobs.result_json.structured_response` contains the client-safe response. For a clarification it includes `response_type: "clarification"` and a versioned `clarification` object. Assistant-message metadata stores the same structured response; an SSE `update` may carry it live but is non-durable.

```json
{
  "status": "waiting_for_user_input",
  "current_step": "complete_or_wait",
  "result_json": {
    "structured_response": {
      "response_type": "clarification",
      "message": "Choose a report and provide its inputs.",
      "clarification": {
        "version": 1,
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "revision": 1,
        "kind": "select_option",
        "question": "Which report would you like?",
        "fields": [{
          "key": "date_range",
          "label": "Report period",
          "field_type": "date_range",
          "required": true,
          "value": null,
          "default_value": null,
          "help_text": "Select an inclusive period.",
          "validation": { "max_range_days": 31 },
          "errors": []
        }],
        "options": [{
          "id": "savings_deposit_total",
          "label": "Total deposits",
          "description": "Total deposit amount for the period.",
          "fields": []
        }],
        "allow_free_text": true
      },
      "options": []
    }
  }
}
```

V1 kinds are `select_option`, `collect_fields`, and `free_text`; field types are `date_range`, `integer`, and `text`. The top-level response `options` is a deprecated compatibility projection. The clarification object is authoritative for rendering. Unknown future kind/type values are not executable client input: clients show safe fallback copy and reconcile the job.

## Lifecycle and compare-and-swap

1. The planner creates the clarification snapshot (id, revision, fields/options and validation) and persists it with the job's waiting state.
2. `POST /chat/jobs/{job_id}/responses` binds a structured answer to `clarification_id` and `clarification_revision`, validates only that active snapshot, then atomically consumes/updates it and resumes the same job.
3. A mismatched revision returns `409 clarification_stale`; no active pending clarification returns `409 clarification_not_active`; invalid safe field values return `400 clarification_validation_error`; inaccessible jobs use `404` resource hiding.
4. Success is `201` containing the inserted user `ChatMessage`; clients recover the resumed job via `GET /chat/jobs/{job_id}`.

This CAS prevents a stale or parallel response from consuming a different job's clarification. Historical assistant messages retain the structured response for read-only rendering; interactive controls require a durable waiting job with the same clarification id/revision.

## Delivery and recovery

`GET /chat/jobs/{job_id}` is the durable recovery source. SSE `update` transmits the same structured response as a lossy, repeated live notification and must be deduplicated; it is not an event log or replay source. Session projections may aid legacy compatibility only and cannot override job-scoped pending state.
