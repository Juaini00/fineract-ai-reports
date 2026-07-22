# API Documentation

Use scenario docs for executable request flows. This directory is the lightweight endpoint map.

## Endpoint groups

- Health/readiness: `GET /health`, `GET /ready` — see [`scenarios/01-health-ready.md`](../scenarios/01-health-ready.md)
- Auth/API keys: `POST /auth/api-keys`, `GET /auth/me` — see [`scenarios/02-auth-api-keys.md`](../scenarios/02-auth-api-keys.md)
- Catalog validation: `POST /catalog/validate` — see [`scenarios/03-catalog-validate.md`](../scenarios/03-catalog-validate.md)
- Vector index: `POST /vector-index/rebuild`, `GET /vector-index/status` — see [`scenarios/04-vector-index.md`](../scenarios/04-vector-index.md)
- Chat: sessions, jobs, SSE, clarification responses — see [`scenarios/05-chat-session-and-job.md`](../scenarios/05-chat-session-and-job.md)

## Clarification API contract

Waiting jobs expose `result_json.structured_response.clarification`; assistant-message metadata and SSE `update` use the same structured response. `GET /chat/jobs/{id}` is durable recovery and SSE is only a non-durable dedupe hint. The v1 object is versioned and identified by UUID/revision; its closed kinds are `select_option`, `collect_fields`, and `free_text`, with `date_range`, `integer`, and `text` field types. Legacy top-level response `options` remains a deprecated projection.

`POST /chat/jobs/{id}/responses` supports structured `{ clarification_id, clarification_revision, option_id, answers, message? }`. The legacy path still requires `message` and permits optional `option_id`. Success is `201` with the inserted `ChatMessage`; fetch the job afterwards. Safe contract failures are `400 clarification_validation_error`, `409 clarification_stale`, `409 clarification_not_active`, and `404` resource hiding. See the exact client types, recovery, and render rules in [`../current/chat-client-integration.md`](../current/chat-client-integration.md).
