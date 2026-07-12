# Chat Client Integration

Base URL local: `http://127.0.0.1:3007`.

All JSON responses use:

```json
{ "success": true, "data": {}, "error": null }
```

Errors use `success=false` and a sanitized `error.message`.

## Authentication

Dashboard auth and API-key management endpoints use the user access token:

```http
Authorization: Bearer <ACCESS_TOKEN>
```

All chat endpoints use an API key owned by a user:

```http
X-API-Key: <API_KEY>
```

API keys without a `user_id` are rejected. The client should not send `owner` when creating an API key; ownership is derived from the authenticated user.

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

Lists chat sessions owned by the API key's user. Use this to render the left-side/session history list.

Auth: API key.

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

Auth: API key.

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

Auth: API key.

Response `200`: same session object as above.

### `GET /chat/sessions/{session_id}/messages`

Returns existing chat messages for a session.

Auth: API key.

Response `200`:

```json
{
  "success": true,
  "data": [
    {
      "id": "<message_id>",
      "session_id": "<session_id>",
      "job_id": "<job_id>",
      "role": "user",
      "content": "What is total deposit this month?",
      "metadata_json": {},
      "created_at": "..."
    }
  ],
  "error": null
}
```

### `POST /chat/jobs`

Starts an AI job. The HTTP response returns immediately; progress comes from SSE.

Auth: API key.

Payload:

```json
{
  "session_id": "<session_id>",
  "message": "What is the total deposit this month?"
}
```

Validation:

- `session_id` must be a valid UUID and must belong to the API key.
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

### `GET /chat/jobs/{job_id}/stream`

Streams job progress with Server-Sent Events.

Auth: API key.

Events:

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

Common `step` values:

- `queued`
- `checking_context`
- `embedding`
- `taking_decision`
- `authorizing`
- `executing_query`
- `shaping_result`
- `formatting_response`
- `response`

### `GET /chat/jobs/{job_id}`

Fetches the latest job state, useful after reconnect or page refresh.

Auth: API key.

Response `200` includes `status`, `current_step`, `state_json`, `result_json`, and `error_json`.

### `POST /chat/jobs/{job_id}/responses`

Continues the same job after a clarification. Do not create a new job.

Auth: API key.

Payload:

```json
{ "message": "1" }
```

Accepted values:

- 1-based option number, for example `"1"`.
- Option label.
- Capability id.

Response `201`: inserted user message. Then reconnect/open SSE for the same `job_id`.

## Client flow

1. Login and store `access_token` in memory or secure storage.
2. Create or fetch a user-owned API key from the dashboard flow. Do not send `owner`; the backend uses the logged-in `user_id`.
3. Load session list with `GET /chat/sessions` using `API_KEY`.
4. If no session is selected, create one with `POST /chat/sessions` using `API_KEY`.
5. Load messages with `GET /chat/sessions/{session_id}/messages`.
6. When user sends a prompt:
   - Disable the send button immediately.
   - Append the user message optimistically or after `POST /chat/jobs` returns.
   - Call `POST /chat/jobs`.
   - Open `GET /chat/jobs/{job_id}/stream`.
7. Render AI state from SSE:
   - `status`/`kind=status`: show text like "AI is thinking..." and map `step` to pipeline text.
   - `kind=clarification`: show the options above the input and keep normal send disabled.
   - `kind=final`: refresh messages and re-enable send.
   - `kind=error`: show the sanitized error and re-enable send.
8. If clarification appears, render option buttons above the input. On click:
   - Call `POST /chat/jobs/{job_id}/responses` with the selected option.
   - Keep the same `job_id`.
   - Reopen/continue SSE.
9. If the browser refreshes mid-job, call `GET /chat/jobs/{job_id}` with `API_KEY`. If status is `queued`, `running`, or `waiting_for_user_input`, restore the disabled/clarification UI from job state and SSE.

## Button state

Disable send when the selected session has an active job with status:

- `queued`
- `running`
- `waiting_for_user_input`

For `waiting_for_user_input`, enable only clarification option buttons. Re-enable normal send after:

- `completed`
- `failed`
- `expired`
- `cancelled`

This prevents stacked requests in the same chat while the pipeline is still running.
