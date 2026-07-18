# Chat Client Integration

This is the current client contract for authentication, chat sessions, same-job clarification, and live updates. Local base URL: `http://127.0.0.1:3007`.

## Response envelope and errors

Every JSON response uses the same envelope:

```json
{ "success": true, "data": {}, "error": null }
```

An error has `success: false`, `data: null`, and an `error`. Until backend sanitization is fixed, never render `error.message` for HTTP `500`/`internal_error`; use fixed generic copy. Clients must never expose backend internals, stack traces, SQL, or prompt text.

| HTTP | Common code | Client action |
| --- | --- | --- |
| `400` | `invalid_request_body`, `validation_error` | Correct the submitted fields. |
| `401` | `unauthorized` | Refresh once, then require login if retry fails. |
| `403` | `role_not_authorized` | Show that administrator access is required. |
| `404` | `not_found` | Treat the resource or current operation as unavailable. |
| `500` | `internal_error` | Show fixed generic copy. Do not automatically retry a mutating request; reconcile durable state first. |

## Authentication model

All `/chat/**` endpoints require an active administrator access token:

```http
Authorization: Bearer <ACCESS_TOKEN>
```

A missing or invalid token returns `401`. A valid non-admin token returns `403 role_not_authorized`.

`X-API-Key` is optional. A valid key narrows the administrator's permitted office scope. An invalid, expired, or revoked key is ignored; it does not authenticate a request and never replaces the bearer token.

```http
X-API-Key: <OPTIONAL_API_KEY>
```

### Refresh cookie

Login and refresh use an HTTP-only refresh cookie. Defaults are:

- name: `refresh_token`
- `Secure`
- `SameSite=Strict`
- path `/`
- lifetime 7 days

Browser requests to login, refresh, and logout must use `credentials: "include"`. Local plain HTTP normally requires `AUTH_REFRESH_COOKIE_SECURE=false`; keep secure cookies enabled in production.

The server does not refresh access tokens automatically. Use one coordinated refresh attempt for concurrent `401` responses, retry each failed request once, and then clear local auth state. Never create an unlimited refresh loop.

## Endpoint matrix

| Method | Path | Auth | Success | Purpose |
| --- | --- | --- | --- | --- |
| `POST` | `/auth/login` | None | `200` | Login and set refresh cookie. |
| `POST` | `/auth/refresh` | Refresh cookie | `200` | Issue a new access token. |
| `POST` | `/auth/logout` | Cookie optional | `200` | End the refresh session and clear cookie. |
| `GET` | `/auth/me` | Bearer | `200` | Get the current user. |
| `POST` | `/auth/api-keys` | Admin bearer | `201` | Create a scoped API key; raw key is returned once. |
| `GET` | `/chat/sessions` | Admin bearer, optional key | `200` | List sessions. |
| `POST` | `/chat/sessions` | Admin bearer, optional key | `201` | Create a session. |
| `GET` | `/chat/sessions/{id}` | Admin bearer, optional key | `200` | Get one session. |
| `GET` | `/chat/sessions/{id}/messages` | Admin bearer, optional key | `200` | List session messages. |
| `POST` | `/chat/jobs` | Admin bearer, optional key | `201` | Submit a prompt, optionally creating a session. |
| `GET` | `/chat/jobs/{id}` | Admin bearer, optional key | `200` | Read durable job state. |
| `GET` | `/chat/jobs/{id}/stream` | Admin bearer, optional key | `200` SSE | Stream the latest live state/update. |
| `POST` | `/chat/jobs/{id}/responses` | Admin bearer, optional key | `201` | Continue the same waiting job. |
| `GET` | `/chat/jobs/{id}/audit` | Admin bearer, optional key | `200` | Optional job diagnostics. |

There is currently no client endpoint here for API-key listing or job cancellation.

## Authentication endpoints

### Login

```http
POST /auth/login
Content-Type: application/json

{"username":"admin","password":"password123"}
```

Response `200` sets the refresh cookie and returns:

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

### Refresh, logout, and current user

`POST /auth/refresh` reads the refresh cookie and returns the same access-token fields needed to replace the in-memory token. `POST /auth/logout` succeeds even when the cookie is absent. `GET /auth/me` uses the bearer token and returns the current user.

### Create an API key

```json
{
  "name": "Reporting dashboard",
  "expires_at": null,
  "allowed_office_ids": [1, 2],
  "allowed_capabilities": ["savings_deposit_total"],
  "allow_all_offices": false,
  "allow_all_capabilities": false,
  "can_view_pii": false
}
```

Response `201`:

```json
{
  "success": true,
  "data": {
    "id": "<api_key_id>",
    "api_key": "<raw_key_returned_once>",
    "message": "Store this API key securely. It will not be shown again."
  },
  "error": null
}
```

Store the raw key immediately in an appropriate secret store. It cannot be fetched again.

## Sessions and messages

`POST /chat/sessions` accepts an optional `title` with a maximum of 120 characters. A blank or whitespace-only title is stored as `null`.

```json
{ "title": "Deposits Q3" }
```

Both list and detail responses use the full session shape:

```json
{
  "id": "<session_id>",
  "user_id": "<user_id-or-null-for-legacy-rows>",
  "api_key_id": "<api_key_id-or-null>",
  "title": "Deposits Q3",
  "status": "active",
  "context_json": {},
  "created_at": "2026-07-18T00:00:00Z",
  "updated_at": "2026-07-18T00:00:00Z",
  "expires_at": null,
  "archived_at": null
}
```

`GET /chat/sessions/{id}/messages` returns messages in this shape:

```json
{
  "id": "<message_id>",
  "session_id": "<session_id>",
  "job_id": "<job_id-or-null>",
  "role": "assistant",
  "metadata_json": {
    "type": "assistant_response",
    "assistant_response": {}
  },
  "content": "# Rendered assistant markdown\n...",
  "created_at": "2026-07-18T00:00:00Z"
}
```

## Submit and inspect a job

`POST /chat/jobs` accepts a nullable `session_id` and a `message` of 1-1000 characters. If `session_id` is absent or `null`, the server creates a session.

```json
{
  "session_id": null,
  "message": "What is the total deposit this month?"
}
```

Response `201`:

```json
{
  "success": true,
  "data": {
    "session_id": "<session_id>",
    "job_id": "<job_id>",
    "user_message_id": "<message_id>",
    "status": "completed",
    "current_step": "response"
  },
  "error": null
}
```

### Important current behavior

The handler currently awaits the assistant graph before returning. The `201` response is therefore usually already `completed`, `waiting_for_user_input`, or `failed`, rather than an immediate `queued` background acknowledgement. Clients must branch on the returned status before deciding whether to open SSE.

`GET /chat/jobs/{id}` returns the full persisted job model, including its identifiers, `status`, `current_step`, state/result/error JSON, and timestamps. Treat it as the durable recovery source.

`GET /chat/jobs/{id}/audit` returns `{ "job_id": "...", "events": [...] }`. This is optional diagnostics, not a required rendering or recovery dependency.

## Same-job clarification

A clarification is indicated by:

```text
status = waiting_for_user_input
current_step = taking_decision
```

Continue it with `POST /chat/jobs/{job_id}/responses`. Never create a new job for a clarification. The request is:

- `message`: required, 1-1000 characters
- `option_id`: optional, maximum 200 characters

For a returned choice, send its exact `id` and useful visible text:

```json
{
  "option_id": "total_deposits",
  "message": "Total deposits"
}
```

For missing parameters or ordinary free-text clarification, omit `option_id`:

```json
{ "message": "Use 2026-07-01 through 2026-07-31" }
```

This distinction matters: sending a capability option for parameter/free-text input can cause repeated capability-choice clarification.

The handler currently awaits the same job's rerun, but its `201` data is the inserted clarification `ChatMessage` (`id`, `session_id`, `job_id`, `role`, `metadata_json`, `content`, `created_at`), not a job result. After `201`, fetch `GET /chat/jobs/{job_id}` and branch on that durable status; open SSE only if it is `queued` or `running`. A job that is inaccessible or not waiting currently returns `404`, not `409`.

## SSE live updates

Request:

```http
GET /chat/jobs/{job_id}/stream
Authorization: Bearer <ACCESS_TOKEN>
Accept: text/event-stream
```

Include `X-API-Key` only when using a valid scoped key. There are exactly two SSE event names:

```text
event: status
data: {"job_id":"...","status":"running","current_step":"executing_query"}

event: update
data: {"kind":"clarification","step":"complete_or_wait","payload":{"response_type":"clarification","structured_response":{},"markdown":"..."},"at":"..."}
```

- `status` data is `{ job_id, status, current_step }`.
- `update` data is `{ kind, step, payload, at }`.
- `clarification`, `final`, and `error` are `update.kind` values, not SSE event names.
- Other update kinds, such as `status`, should be handled defensively.

### Current delivery limits

Redis stores only the latest event for about one hour. The stream polls it, so the same update can arrive each second. Deduplicate identical updates (for example by a stable serialization of `kind`, `step`, `payload`, and `at`) and make rendering idempotent.

This is not an ordered event log: there is no SSE `id`, `retry`, `Last-Event-ID`, or replay guarantee. Do not calculate durable state by replaying updates.

- If Redis is not configured, the endpoint emits one `status` event and closes.
- If Redis is configured but unreachable, the stream closes.
- It stops after observing `completed` or `failed`, or after roughly 120 polling ticks.
- `expired` and `cancelled` are not current stream termination checks.

Always recover from `GET /chat/jobs/{id}`, not from assumptions about missed SSE events.

### Browser transport

Native `EventSource` cannot set an `Authorization` header. Use `fetch` streaming or a header-capable SSE library. Never put an access token or API key in the URL.

```ts
type ApiEnvelope<T> = { success: boolean; data: T | null; error: null | { code?: string; message: string } };

let accessToken = "";
let refreshPromise: Promise<boolean> | null = null;

async function refreshAccessToken(): Promise<boolean> {
  refreshPromise ??= fetch("/auth/refresh", {
    method: "POST",
    credentials: "include",
  }).then(async (response) => {
    if (!response.ok) return false;
    const body: ApiEnvelope<{ access_token: string }> = await response.json();
    if (!body.success || !body.data) return false;
    accessToken = body.data.access_token;
    return true;
  }).finally(() => { refreshPromise = null; });
  return refreshPromise;
}

async function apiFetch(path: string, init: RequestInit = {}, retry = true): Promise<Response> {
  const headers = new Headers(init.headers);
  headers.set("Authorization", `Bearer ${accessToken}`);
  if (init.body) headers.set("Content-Type", "application/json");
  let response = await fetch(path, { ...init, headers, credentials: "include" });
  if (response.status === 401 && retry && await refreshAccessToken()) {
    response = await apiFetch(path, init, false);
  }
  return response;
}

function apiErrorMessage(response: Response, body: ApiEnvelope<unknown>, fallback: string) {
  return response.status < 500 ? body.error?.message ?? fallback : fallback;
}
```

Add a valid API key with `headers.set("X-API-Key", apiKey)` when scoped access is desired.
The single `401` refresh above is the only automatic retry. After a `500` from a mutating request such as `POST /chat/jobs`, reconcile sessions, messages, and jobs before offering a deliberate retry because persistence may have occurred before the failure.

Minimal streaming parser:

```ts
async function streamJob(
  jobId: string,
  onEvent: (name: "status" | "update", data: unknown) => void,
  signal?: AbortSignal,
) {
  const response = await apiFetch(`/chat/jobs/${jobId}/stream`, {
    headers: { Accept: "text/event-stream" },
    signal,
  });
  if (!response.ok || !response.body) throw new Error("Live updates unavailable");

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  for (;;) {
    const { value, done } = await reader.read();
    buffer += decoder.decode(value, { stream: !done });
    buffer = buffer.replaceAll("\r\n", "\n");
    let boundary: number;
    while ((boundary = buffer.indexOf("\n\n")) >= 0) {
      const block = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      let name = "message";
      const data: string[] = [];
      for (const line of block.split("\n")) {
        if (line.startsWith("event:")) name = line.slice(6).trim();
        if (line.startsWith("data:")) data.push(line.slice(5).trimStart());
      }
      if ((name === "status" || name === "update") && data.length) {
        onEvent(name, JSON.parse(data.join("\n")));
      }
    }
    if (done) break;
  }
}
```

The parser appends each decoded chunk before normalizing the whole buffer, so it handles chunk boundaries (including a CR/LF split across chunks) and multi-line `data`. Production code should catch JSON/network errors, abort stale streams, deduplicate updates, and then reconcile with `GET /chat/jobs/{id}`.

## End-to-end client algorithm

1. Login with `credentials: "include"`; keep the access token in memory or protected application storage.
2. Optionally create and securely retain a scoped API key. It only narrows scope.
3. Load sessions, select or create one, then load its messages.
4. On prompt submit, disable normal send and call `POST /chat/jobs`.
5. Save the returned `session_id` and `job_id`; do not assume the job is queued.
6. Branch on the returned status:
   - `queued` or `running`: show `current_step` and open SSE.
   - `waiting_for_user_input`: show clarification from `result_json.structured_response` or persisted assistant-message metadata.
   - `completed`: fetch session messages and render `metadata_json.assistant_response`, falling back to markdown `content`.
   - `failed`: show a safe error and enable normal send.
7. For clarification, enable only the relevant choice buttons or free-text submit. Post to `/responses` with the same job ID and disable those controls while submitting. The `201` data is the inserted clarification message, so fetch the same job and branch on its durable status; open SSE only if it is `queued` or `running`.
8. For every SSE update, deduplicate and treat it as a hint. On `kind: clarification`, reconcile the job and show clarification. On `kind: final` or `kind: error`, reconcile the job and messages.
9. When a stream closes or the page reloads, call `GET /chat/jobs/{id}`:
   - reopen SSE only for `queued` or `running`;
   - restore clarification for `waiting_for_user_input`;
   - fetch messages for `completed`;
   - show a safe error for `failed`.

Do not wait for SSE before handling the initial or clarification HTTP result: current handlers may have already completed the graph.

### Clarification helper

```ts
async function respondToClarification(jobId: string, message: string, optionId?: string) {
  const response = await apiFetch(`/chat/jobs/${jobId}/responses`, {
    method: "POST",
    body: JSON.stringify({ message, ...(optionId ? { option_id: optionId } : {}) }),
  });
  const body = await response.json();
  if (!response.ok || !body.success) throw new Error(apiErrorMessage(response, body, "Unable to continue"));
  // The response data is the inserted clarification ChatMessage, not job state.
  const clarificationMessage = body.data;
  const jobResponse = await apiFetch(`/chat/jobs/${jobId}`);
  const jobBody = await jobResponse.json();
  if (!jobResponse.ok || !jobBody.success) throw new Error(apiErrorMessage(jobResponse, jobBody, "Unable to load job"));
  return { clarificationMessage, job: jobBody.data }; // Open SSE only if job is queued/running.
}

// Choice button:
await respondToClarification(jobId, option.label, option.id);
// Missing parameter/free text:
await respondToClarification(jobId, "Use July 2026");
```

## UI state and response rendering

Disable normal prompt submission while the selected session's job is `queued`, `running`, or `waiting_for_user_input`. While waiting, enable only clarification controls. Disable a clicked choice/free-text submit until its request finishes to prevent duplicate responses. Re-enable normal input after `completed` or `failed`; for any other recovered state, follow the durable job result rather than SSE assumptions.

Assistant-message `content` is rendered markdown. Its `metadata_json` is `{ "type": "assistant_response", "assistant_response": <structured response> }`; prefer `assistant_response` and use `content` only as a markdown fallback. Job `result_json` separately contains `structured_response` and `markdown`; use those for in-progress recovery, then reconcile completed rendering from persisted messages.

```json
{
  "response_type": "table",
  "title": "Total deposits",
  "message": "Here are the matching records.",
  "sections": [],
  "table": {
    "columns": [{ "key": "client_name", "label": "Client" }],
    "rows": [{ "client_name": "Amina" }]
  },
  "cards": [],
  "options": [],
  "warnings": [],
  "actions": [],
  "evidence_refs": []
}
```

Render defensively:

- `summary`: title/message, sections, cards, warnings, actions, and safe evidence references.
- `table`: columns in server order and exactly the returned rows; show an empty state for none.
- `metric_cards`: only returned cards and values.
- `clarification`: message and returned options.
- `help`, `unsupported`, `out_of_domain`, `policy_blocked`, `error`: the server's safe copy and available warnings/actions.
- Never invent rows, metrics, labels, or requested counts that were not returned.
- Never expose diagnostic state, hidden prompt data, SQL, or internal errors.

## Curl flow

Cookie jar use is shown for local development. Keep real credentials out of shell history and logs.

```bash
BASE=http://127.0.0.1:3007

curl -sS -c cookies.txt -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"password123"}' "$BASE/auth/login"

TOKEN='<access_token>'

curl -sS -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"session_id":null,"message":"What is the total deposit this month?"}' \
  "$BASE/chat/jobs"

JOB_ID='<job_id>'

curl -N -H "Authorization: Bearer $TOKEN" -H 'Accept: text/event-stream' \
  "$BASE/chat/jobs/$JOB_ID/stream"

curl -sS -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"message":"Use July 2026"}' "$BASE/chat/jobs/$JOB_ID/responses"

curl -sS -H "Authorization: Bearer $TOKEN" "$BASE/chat/jobs/$JOB_ID"

curl -sS -b cookies.txt -c cookies.txt -X POST "$BASE/auth/refresh"
curl -sS -b cookies.txt -c cookies.txt -X POST "$BASE/auth/logout"
```

Add `-H 'X-API-Key: <valid_key>'` only when testing narrowed API-key scope.

## Security and deployment checklist

- Require HTTPS in production; keep the refresh cookie `Secure`.
- Use `credentials: "include"` only with the intended API origin and CORS policy.
- Current backend CORS accepts local `localhost` and `127.0.0.1` origins only. Change or configure backend CORS for the exact non-local frontend origin and verify it before deployment.
- Never place bearer tokens or API keys in URLs, query strings, analytics, or logs.
- Prefer an in-memory access token; protect any persistent client storage against script access.
- Treat a raw API key as a secret and remember it is displayed only once.
- Do not treat `X-API-Key` as authentication or broaden permissions client-side.
- Sanitize rendered markdown/HTML. Never render HTTP `500`/`internal_error` messages; use fixed generic copy until backend sanitization is fixed.
- Abort streams when changing account, session, or page; do not leak a previous user's updates.

## Troubleshooting

| Symptom | Likely cause | Action |
| --- | --- | --- |
| Login works but refresh cookie is absent locally | `Secure` cookie over HTTP | Set `AUTH_REFRESH_COOKIE_SECURE=false` only for local HTTP and use `credentials: "include"`. |
| Chat returns `401` | Missing/expired bearer token | Refresh once and retry once; otherwise login again. An API key alone cannot help. |
| Chat returns `403 role_not_authorized` | Bearer user is not an active admin | Use an authorized admin account. |
| Invalid API key does not return an auth error | Current optional-key behavior | The key is ignored; bearer auth remains authoritative. Replace the key if narrowed scope is required. |
| New job is already completed or waiting | Handler awaited the graph | Handle the returned status before opening SSE. |
| Clarification repeats capability choices | `option_id` sent for free text/parameters | Omit `option_id` unless selecting a returned choice. |
| Clarification response returns `404` | Job inaccessible or not waiting | Fetch the job/session and reconcile; current behavior is not `409`. After a `201`, also fetch the job because the response data is the inserted message. |
| SSE repeats the same update | Latest Redis event is polled | Deduplicate and keep rendering idempotent. |
| SSE closes after one status | Redis is not configured | Use the job response and `GET /chat/jobs/{id}` for durable state. |
| SSE closes unexpectedly | Redis unavailable, timeout, auth/network issue, or terminal job | Fetch the job; reopen only if it is `queued` or `running`. |
| Native `EventSource` cannot connect with auth | It cannot set bearer headers | Use fetch streaming or a header-capable library; never put tokens in the URL. |
| Browser reports a CORS failure in deployment | Current backend CORS accepts local origins only | Change or configure backend CORS and verify the exact production origin. |

## Source-of-truth caveats

PostgreSQL job/session/message state and `GET /chat/jobs/{id}` are durable source of truth. Redis/SSE is only a lossy live hint. Current behavior does not promise background job acknowledgement, ordered replay, reconnect cursors, API-key-only authentication, key listing, cancellation, or automatic access-token refresh. Client implementations must feature-detect from HTTP status and returned data rather than assuming those capabilities.
