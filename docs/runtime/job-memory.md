# Job Memory

Job memory is the durable working state for a single chat job. It preserves the user's original intent across clarification turns so a job cannot drift from "show 10 clients with the most savings accounts" into an unrelated savings summary after the user answers a follow-up.

## Storage Rule

The source of truth is PostgreSQL:

```text
chat_jobs.state_json     -> durable job memory payload
chat_jobs.state_revision -> optimistic-lock version for state updates
```

Redis is only for live progress and SSE coordination. RAM is only temporary data during the current request. Do not store resumable job memory in Redis, global variables, static maps, or process-local caches.

Current implementation:

```text
crates/chat/src/chat/pending_intent.rs
```

The runtime stores typed pending-intent state as JSON inside `chat_jobs.state_json.pending_intent` and exposes `chat_jobs.state_revision` through the `ChatJob` model.

Candidate capabilities are filtered against the original prompt shape before they are stored or shown as clarification options. For example, a client ranking prompt must not store savings deposit/withdrawal options as pending memory.

## Isolation

Job memory is isolated by `chat_jobs.id` and `api_key_id`.

Every clarification response uses:

```text
POST /chat/jobs/{job_id}/responses
```

The repository must load and update only the row matching both `job_id` and the authenticated `api_key_id`. Two users, two API keys, or two jobs cannot share memory unless they share the same row, which policy forbids.

## Concurrency

Use optimistic locking for every memory update:

```sql
UPDATE chat_jobs
SET state_json = $1,
    state_revision = state_revision + 1,
    updated_at = now()
WHERE id = $2
  AND api_key_id = $3
  AND state_revision = $4;
```

If `rows_affected = 0`, another request updated the job first. Reload the job and retry from the latest state or return a safe conflict. Do not overwrite `state_json` from a stale request.

`POST /chat/jobs/{job_id}/responses` also locks the target job row with `FOR UPDATE` while appending the clarification response and requeueing the job. This prevents two simultaneous clarification responses for the same job from both being accepted while the job is still `waiting_for_user_input`.

When no active pending intent remains, `pending_intent` is written as `null`. Resolved pending intents may remain in `state_json` for audit, but runtime readers must ignore entries whose status is `resolved`.

## Pending Intent

When a job requires clarification, store a typed `pending_intent` inside `state_json`:

```json
{
  "schema_version": 1,
  "revision": 1,
  "original_message": "show 10 clients with the most savings accounts",
  "status": "collecting_slots",
  "domain": "client",
  "target_entity": "client",
  "requested_shape": "top_n",
  "metric": "savings_account_count",
  "candidate_capabilities": ["client_top_n_by_savings_account_count"],
  "selected_capability": "client_top_n_by_savings_account_count",
  "params": {
    "limit": 10,
    "office_scope": "authorized_scope"
  },
  "missing_slots": [],
  "last_user_response": null,
  "invalid_attempts": 0
}
```

The Rust code should manipulate this as typed structs, not ad-hoc `serde_json::Value` mutations. JSON is the persistence format, not the domain model.

## Clarification Rules

If `state_json.pending_intent` is active, a clarification response must first be interpreted against that intent.

Allowed clarification actions:

1. Select one of `candidate_capabilities`.
2. Fill one of `missing_slots`, such as `from_date`, `to_date`, or `limit`.
3. Abandon the pending intent and start a new one when the response is a clear new report request.

Disallowed behavior:

1. Reclassify a slot answer or option choice from scratch while `pending_intent` is active.
2. Drop `original_message` or the original extracted metric.
3. Replace a client capability with a savings capability unless the user response is a clear new report request.
4. Execute when the response does not fill the requested slot.

Invalid clarification responses increment `invalid_attempts`, preserve the original pending intent, and ask again. They must not silently select a single candidate just because only one candidate is available. Explicit new-request phrases such as `other_activity`, `actually`, `instead`, `new request`, `forget that`, or `ganti`, plus report-shaped free text such as `show ... savings ...`, abandon the pending intent and allow normal retrieval to start again.

## State Transitions

The minimum job-memory state machine is:

```text
created
  -> collecting_slots
  -> waiting_for_capability_choice
  -> ready_to_execute
  -> resolved
```

Invalid clarification responses increment `invalid_attempts` and keep the prior `pending_intent` intact. Completed, failed, cancelled, or expired jobs must not accept further memory updates.

## Audit

Important memory changes must be written to `chat_job_events` or `chat_job_checkpoints`:

```text
pending_intent_created
slot_filled
capability_selected
invalid_clarification
pending_intent_resolved
```

The audit payload must not include raw API keys, SQL, prompts hidden from the user, or unnecessary PII.

## Relationship To Other Docs

- `docs/chat-data-model.md` defines the durable tables and storage responsibilities.
- `docs/implementation-steps.md` tracks implementation status and sequencing.
- `docs/Modern_RAG_Architecture_Blueprint.md` defines the broader pipeline; job memory is the concrete per-job state layer that keeps semantic parsing, ambiguity resolution, retrieval, and clarification connected.
