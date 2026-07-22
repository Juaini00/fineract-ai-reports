# Frontend Session Management Contract

This document is the frontend integration contract for listing, creating,
renaming, reading, and deleting chat sessions. The local API base URL is
`http://127.0.0.1:3007`.

## Authentication and response envelope

Every request in this document requires an active administrator bearer token:

```http
Authorization: Bearer <ACCESS_TOKEN>
Content-Type: application/json
```

`X-API-Key` is optional and may only narrow office scope. It never replaces the
bearer token.

All JSON responses use the same envelope:

```ts
export type ApiError = {
  code: string;
  message: string;
  details?: unknown;
};

export type ApiResponse<T> =
  | { success: true; data: T; error: null }
  | { success: false; data: null; error: ApiError };
```

## Data types

```ts
export type ChatSession = {
  id: string;
  user_id: string | null;
  api_key_id: string | null;
  title: string | null;
  status: "active" | "archived" | "expired";
  context_json: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  expires_at: string | null;
  archived_at: string | null;
};

export type ChatMessage = {
  id: string;
  session_id: string;
  job_id: string | null;
  role: "user" | "assistant" | "system" | "tool" | "clarification";
  content: string;
  metadata_json: Record<string, unknown>;
  created_at: string;
};

export type CreateSessionPayload = {
  title?: string | null;
};

export type RenameSessionPayload = {
  title: string;
};

export type DeleteSessionResult = {
  session_id: string;
  deleted: true;
};
```

Timestamps are ISO-8601 strings. Treat `context_json` and `metadata_json` as
opaque backend-owned values unless another contract explicitly documents a
field.

## Endpoint summary

| Method | Path | Request body | Success data |
| --- | --- | --- | --- |
| `GET` | `/chat/sessions` | None | `ChatSession[]` |
| `POST` | `/chat/sessions` | `CreateSessionPayload` | `ChatSession` |
| `GET` | `/chat/sessions/{session_id}` | None | `ChatSession` |
| `PATCH` | `/chat/sessions/{session_id}` | `RenameSessionPayload` | `ChatSession` |
| `DELETE` | `/chat/sessions/{session_id}` | None | `DeleteSessionResult` |
| `GET` | `/chat/sessions/{session_id}/messages` | None | `ChatMessage[]` |

### List sessions

```http
GET /chat/sessions
```

The API returns only accessible, non-archived sessions, ordered by
`updated_at DESC` and then `created_at DESC`. Renaming a session updates
`updated_at`, so it may move to the top of the list.

### Create a session

```http
POST /chat/sessions
Content-Type: application/json

{ "title": "Deposits Q3" }
```

`title` is optional and has a maximum length of 120 characters. `null`, an
omitted field, or whitespace-only text is stored as `null`. Success is `201`
with the full `ChatSession` in `data`.

### Read a session and its messages

```http
GET /chat/sessions/<session_id>
GET /chat/sessions/<session_id>/messages
```

Both return `200`. A missing, foreign, or archived session returns the same
sanitized `404`; the frontend must not attempt to distinguish those cases.

### Rename a session

```http
PATCH /chat/sessions/<session_id>
Content-Type: application/json

{ "title": " Deposits Q4 " }
```

Rules:

- `title` is required and cannot be `null`.
- The backend trims leading and trailing whitespace before storing it.
- The trimmed title must contain 1-120 Unicode characters.
- Missing, `null`, blank, or overlong titles return HTTP `400` with the normal
  error envelope.
- Success is `200`; `data` is the full updated `ChatSession`.
- Missing, foreign, or archived sessions return `404`.

After success, replace the cached session with the returned object. Do not
construct the updated object locally because the server owns `updated_at` and
title normalization.

### Delete a session

```http
DELETE /chat/sessions/<session_id>
```

No request body is sent. Success is `200`:

```json
{
  "success": true,
  "data": {
    "session_id": "<session_id>",
    "deleted": true
  },
  "error": null
}
```

Deletion is an immediate soft archive, not a physical purge. On success, the
frontend should remove the session from lists, close its panel or route, clear
its messages and job cache, and navigate to a safe fallback such as the session
list or a new chat.

A second delete returns `404`. Treat that as a reconciliation signal: remove
the session locally if it still exists, but do not display whether it was
missing, foreign, or already archived.

## Effect of deletion on jobs

After deletion, the backend returns `404` for all new access associated with
the archived session:

- creating another job with that `session_id`;
- reading a job or its audit timeline;
- opening a new SSE connection;
- sending a clarification response;
- reading the session or its messages.

An HTTP job request or SSE connection that started before deletion may finish
naturally. The frontend must ignore late responses/events for a session it has
already removed. Deletion does not force-cancel work, restore a session, purge
PostgreSQL history, or clean Redis keys.

## Error handling

| HTTP | Frontend action |
| --- | --- |
| `400` | Keep the form open and show field validation. |
| `401` | Refresh once, retry once, then require login. |
| `403` | Show that administrator access is required. |
| `404` | Remove/reconcile stale session state and navigate away if active. |
| `500` | Show fixed generic copy and reconcile state before retrying a mutation. |

Never expose SQL, prompt text, stack traces, or raw backend internals. Do not
automatically retry `PATCH` or `DELETE` after a `500`; first refetch the session
list to determine the durable state.

## Reference API client

```ts
const API_BASE_URL = "http://127.0.0.1:3007";

async function chatRequest<T>(
  path: string,
  accessToken: string,
  init: RequestInit = {},
): Promise<ApiResponse<T>> {
  const response = await fetch(`${API_BASE_URL}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${accessToken}`,
      ...(init.body ? { "Content-Type": "application/json" } : {}),
      ...init.headers,
    },
  });

  return response.json() as Promise<ApiResponse<T>>;
}

export function renameSession(
  sessionId: string,
  title: string,
  accessToken: string,
) {
  return chatRequest<ChatSession>(`/chat/sessions/${sessionId}`, accessToken, {
    method: "PATCH",
    body: JSON.stringify({ title }),
  });
}

export function deleteSession(sessionId: string, accessToken: string) {
  return chatRequest<DeleteSessionResult>(
    `/chat/sessions/${sessionId}`,
    accessToken,
    { method: "DELETE" },
  );
}
```

The example returns the envelope instead of throwing on non-2xx responses so
the caller can branch on the documented `error.code`. Production clients may
wrap this with the application's coordinated token-refresh and query-cache
strategy.
