# Chat Client Integration

Base URL local: `http://127.0.0.1:3007`.

All JSON responses use the API envelope:

```json
{ "success": true, "data": {}, "error": null }
```

Errors use `success=false` and a sanitized `error.message`. Do not render raw backend error internals.

## Authentication

Dashboard auth and API-key management endpoints use the user access token:

```http
Authorization: Bearer <ACCESS_TOKEN>
```

Chat endpoints require both the logged-in user token and that user's API key:

```http
Authorization: Bearer <ACCESS_TOKEN>
X-API-Key: <API_KEY>
```

The API key must belong to the same `user_id` as the bearer token. API keys without a `user_id`, missing bearer tokens, and mismatched token/key pairs are rejected. The client should not send `owner` when creating an API key; ownership is derived from the authenticated user.

## Endpoints

### `POST /auth/login`

Payload:

```json
{ "username": "admin", "password": "password123" }
```

Validation:

- `username` is required.
- `password` is required.

Response `200`:

```json
{
  "success": true,
  "data": {
    "access_token": "...",
    "token_type": "Bearer",
    "expires_in": 900,
    "user": { "id": "...", "username": "admin", "role": "admin" }
  },
  "error": null
}
```

### `GET /chat/sessions`

Lists chat sessions owned by the API key's user. Use this to render session history.

Auth: bearer token + API key.

Response `200`:

```json
{
  "success": true,
  "data": [
    {
      "id": "<session_id>",
      "api_key_id": "<api_key_id>",
      "title": "Deposits Q3",
      "status": "active",
      "context_json": {},
      "created_at": "2026-07-12T00:00:00Z",
      "updated_at": "2026-07-12T00:00:00Z",
      "expires_at": null,
      "archived_at": null
    }
  ],
  "error": null
}
```

### `POST /chat/sessions`

Creates a new chat session.

Auth: bearer token + API key.

Payload:

```json
{ "title": "Deposits Q3" }
```

Validation:

- `title` is optional.
- Empty or whitespace-only title is stored as `null`.

Response `201`:

```json
{
  "success": true,
  "data": { "id": "<session_id>", "title": "Deposits Q3", "created_at": "..." },
  "error": null
}
```

### `GET /chat/sessions/{session_id}`

Returns one session for the API-key owner.

Auth: bearer token + API key.

Response `200`: same session object as above.

### `GET /chat/sessions/{session_id}/messages`

Returns existing chat messages for a session.

Auth: bearer token + API key.

Response `200`:

```json
{
  "success": true,
  "data": [
    {
      "id": "<message_id>",
      "session_id": "<session_id>",
      "job_id": "<job_id>",
      "role": "assistant",
      "content": "...",
      "metadata_json": {
        "structured_response": {},
        "rendered_markdown": "..."
      },
      "created_at": "..."
    }
  ],
  "error": null
}
```

### `POST /chat/jobs`

Starts an assistant job. The HTTP response returns immediately; progress comes from SSE.

Auth: bearer token + API key.

Payload:

```json
{
  "session_id": "<session_id>",
  "message": "What is the total deposit this month?"
}
```

Validation:

- `session_id` must be a valid UUID and must belong to the API-key user.
- `message` is required and must not be empty.

Response `201`:

```json
{
  "success": true,
  "data": {
    "job_id": "<job_id>",
    "session_id": "<session_id>",
    "user_message_id": "<message_id>",
    "status": "queued",
    "current_step": "queued"
  },
  "error": null
}
```

Immediately open `GET /chat/jobs/{job_id}/stream` with the same auth headers.

### `GET /chat/jobs/{job_id}/stream`

Streams job progress with Server-Sent Events.

Auth: bearer token + API key.

Event names currently used:

- `status`: snapshot-style live state.
- `update`: job event emitted by the worker.

Example stream:

```text
event: status
data: {"job_id":"...","status":"queued","current_step":"queued"}

event: update
data: {"kind":"status","step":"checking_context","payload":{},"at":"..."}

event: update
data: {"kind":"clarification","step":"taking_decision","payload":{"options":[...]},"at":"..."}

event: update
data: {"kind":"final","step":"response","payload":{"status":"completed"},"at":"..."}
```

`update.data` is JSON shaped as:

```json
{ "kind": "status", "step": "checking_context", "payload": {}, "at": "..." }
```

Treat `payload` as event-specific. Known `kind` values include `status`, `clarification`, `final`, and `error`.

Common job statuses:

- `queued`
- `running`
- `waiting_for_user_input`
- `completed`
- `failed`
- `expired`
- `cancelled`

Common `current_step` / event `step` values and practical UI labels:

| Step | Suggested label |
| --- | --- |
| `queued` | Queued |
| `checking_context` | Checking conversation context |
| `embedding` | Finding relevant reporting knowledge |
| `taking_decision` | Choosing the right report or asking a question |
| `authorizing` | Checking permissions |
| `executing_query` | Running the approved report query |
| `shaping_result` | Shaping report data |
| `formatting_response` | Preparing the answer |
| `response` | Finalizing response |

Terminal live states are `completed`, `failed`, `expired`, and `cancelled`. On terminal state, close SSE, fetch messages, and re-enable normal send.

### `GET /chat/jobs/{job_id}`

Fetches the latest job state, useful after reconnect or page refresh.

Auth: bearer token + API key.

Response `200` includes `status`, `current_step`, `state_json`, `result_json`, and `error_json`.

Use this for recovery:

- `queued` or `running`: restore disabled send, show the step label, reopen SSE.
- `waiting_for_user_input`: restore clarification UI from `state_json`/latest event when present; keep normal send disabled.
- `completed`: fetch messages and render the latest assistant response.
- `failed`, `expired`, `cancelled`: show the sanitized terminal state/error, then allow a new user prompt.

### `POST /chat/jobs/{job_id}/responses`

Continues the same job after a clarification. Do not create a new job.

Auth: bearer token + API key.

Request type: `RespondToChatJobRequest`.

Payload fields:

- `message` string, required.
- `option_id` string, optional by schema but required by the client for option-button selections.

For any returned option except `others`, send both:

- `option_id`: the exact option id returned by the backend.
- `message`: the visible option label or description for audit/display.

The server uses `option_id`, not `message`, to resolve non-`others` selections. Invalid `option_id` is rejected and the job remains waiting.

Non-`others` example:

```json
{
  "option_id": "total_deposits",
  "message": "Total deposits"
}
```

For `others`, send `option_id="others"` and a user-provided message:

```json
{
  "option_id": "others",
  "message": "Show deposits grouped by branch for last quarter"
}
```

If the user clicks Others before entering free text, the client may send:

```json
{ "option_id": "others", "message": "others" }
```

Then keep the same job and prompt for free text if the server response keeps the job in `waiting_for_user_input`.

Response `201`: inserted user message. Reopen or continue SSE for the same `job_id`.

## Assistant response payload

Render `structured_response` as the source of truth. Use `rendered_markdown`/`markdown` only as a fallback if structured data is missing or a renderer has not implemented a shape yet.

Do not assume the requested row count was returned. Always read and display the actual returned rows/cards and any warnings.

Common shape:

```json
{
  "response_type": "table",
  "title": "Total deposits",
  "message": "Here are the matching deposit records.",
  "sections": [],
  "table": {
    "columns": [{ "key": "client_name", "label": "Client" }],
    "rows": [{ "client_name": "Amina" }]
  },
  "cards": [],
  "options": [],
  "warnings": [],
  "actions": [],
  "evidence_refs": [],
  "rendered_markdown": "..."
}
```

Known `response_type` values:

- `summary`: render title/message, sections, cards, warnings, actions, and evidence refs.
- `table`: render table columns/rows exactly as returned; show zero-state if `rows=[]`.
- `metric_cards`: render `cards` as metric tiles; do not invent missing metrics.
- `clarification`: render `message` and `options` as selectable actions.
- `help`: render guidance/help text.
- `unsupported`: explain unsupported request and any suggested actions/options.
- `out_of_domain`: explain that the request is outside approved reporting scope.
- `policy_blocked`: show policy-safe block message and warnings/actions if present.
- `error`: show sanitized error copy.

Field rendering notes:

- `table.columns`: preserve backend order; use `label` for headers and `key` for row lookup.
- `table.rows`: render actual row count returned; never pad or promise additional rows.
- `cards`: render `label`, `value`, and any provided unit/trend/metadata fields defensively.
- `options`: render buttons; submit selected `id` as `option_id`.
- `warnings`: show near the result, not as fatal errors unless response type says so.
- `actions`: render as follow-up suggestions/buttons when present.
- `evidence_refs`: render in a collapsible details area if useful; do not expose hidden prompt/debug text.
- `rendered_markdown`: fallback display only.

## Deterministic extraction debug metadata

Verified extraction may record `deterministic_extraction` and `deterministic_extraction_conflicts` in job state, result, or message metadata. This metadata is for diagnostics; exact shape and location can evolve.

Client rule:

- Production UI must not depend on this metadata.
- Development builds may show it in a debug panel for support and QA.
- If conflicts exist, prefer server clarification/results over client-side guessing.

## Client flow

1. Login and store `access_token` in memory or secure storage.
2. Create or fetch a user-owned API key from the dashboard flow. Do not send `owner`; the backend uses the logged-in `user_id`.
3. Send both `Authorization: Bearer <ACCESS_TOKEN>` and `X-API-Key: <API_KEY>` on every chat request.
4. Load sessions with `GET /chat/sessions`.
5. If no session is selected, create one with `POST /chat/sessions`.
6. Load messages with `GET /chat/sessions/{session_id}/messages`.
7. When the user sends a prompt:
   - Disable normal send immediately for the selected session.
   - Append the user message optimistically or after `POST /chat/jobs` returns.
   - Call `POST /chat/jobs`.
   - Open `GET /chat/jobs/{job_id}/stream`.
8. Render live state from SSE:
   - `status` event or `kind=status`: show the mapped step label.
   - `kind=clarification`: show returned options above the input; keep normal send disabled.
   - `kind=final` or terminal `completed`: fetch messages, render `structured_response`, close SSE, re-enable send.
   - `kind=error` or terminal `failed`: show sanitized error, close SSE, re-enable send.
9. If clarification appears, render option buttons. Always include `Others` when returned. On click:
   - For normal options, call `POST /chat/jobs/{job_id}/responses` with `option_id` and the visible label/description as `message`.
   - For Others, collect free text and send `option_id="others"` with that text as `message`.
   - Keep the same `job_id`.
   - Reopen or continue SSE.
10. If the browser refreshes mid-job, call `GET /chat/jobs/{job_id}` with both auth headers and apply the recovery rules above.

## Button state

Disable normal send when the selected session has an active job with status:

- `queued`
- `running`
- `waiting_for_user_input`

For `waiting_for_user_input`, enable only clarification option buttons and any required Others free-text submit. Re-enable normal send after:

- `completed`
- `failed`
- `expired`
- `cancelled`

This prevents stacked requests in the same chat while the pipeline is still running.
