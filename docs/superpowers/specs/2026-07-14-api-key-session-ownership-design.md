# API key and chat session ownership design

Date: 2026-07-14
Status: draft for implementation
Issue: `docs/issues/active/004-api-key-session-ownership.md`

## Goal

Separate conversation ownership from execution authorization.

The authenticated user owns chat sessions and message history. The selected API key authorizes each chat job and determines the capability, office, and PII scope for that job.

## Current mismatch

The current runtime uses `api_key_id` as both:

- the session visibility key; and
- the job execution permission key.

That is wrong for first-party user sessions because a single user may have several API keys.

## Target model

```text
users
  id

api_keys
  id
  user_id
  key_hash
  key_prefix
  permissions...

chat_sessions
  id
  user_id
  created_by_api_key_id nullable
  title
  status
  timestamps

chat_jobs
  id
  session_id
  api_key_id
  user_message_id
  status
  state/result/error
```

Ownership rules:

- `chat_sessions.user_id` is the source of truth for session ownership.
- `chat_sessions.created_by_api_key_id` is audit only.
- `chat_jobs.api_key_id` is the source of truth for job permission scope.
- `chat_messages` inherit access through `chat_sessions.user_id`.

## Authentication modes

### First-party dashboard mode

The frontend sends bearer token and selected API key id.

Possible request contract:

```http
Authorization: Bearer <ACCESS_TOKEN>
X-API-Key-Id: <API_KEY_ID>
```

or body field for job creation:

```json
{
  "session_id": "...",
  "api_key_id": "...",
  "message": "show 10 clients..."
}
```

Backend validation:

1. bearer user is valid;
2. selected API key exists and belongs to bearer user;
3. session exists and belongs to bearer user;
4. job uses selected API key scope.

### External API mode

Server-to-server clients may continue using:

```http
X-API-Key: <RAW_API_KEY>
```

The raw key identifies both user and scope through its stored hash. Raw key material is never returned after creation.

## API key metadata endpoints

First-party clients need safe metadata endpoints:

### `GET /auth/api-keys`

Returns keys owned by the authenticated bearer user.

Response shape:

```json
[
  {
    "id": "...",
    "name": "Reporting key",
    "key_prefix": "air_live_xxx",
    "capabilities": ["..."],
    "office_ids": [1, 2],
    "can_view_pii": false,
    "created_at": "...",
    "last_used_at": "...",
    "expires_at": null,
    "revoked_at": null
  }
]
```

No `key_hash` and no raw key are returned.

### `POST /auth/api-keys`

Returns raw key only once on creation. Ownership is derived from bearer user; clients do not send `owner`.

## Chat endpoint behavior

Session endpoints:

- `GET /chat/sessions`: list by `chat_sessions.user_id`.
- `POST /chat/sessions`: create with `user_id`; optionally store selected/current key as `created_by_api_key_id`.
- `GET /chat/sessions/{id}`: require `session.user_id == bearer.user_id`.
- `GET /chat/sessions/{id}/messages`: require `session.user_id == bearer.user_id`.

Job endpoints:

- `POST /chat/jobs`: require owned session and owned selected API key; store job `api_key_id`.
- `GET /chat/jobs/{id}`: require job session owner equals bearer user. Optionally require access to job's API key for external raw-key mode.
- `POST /chat/jobs/{id}/responses`: require session owner; continue with the original job `api_key_id` unless an explicit key switch feature is later designed.
- SSE stream: require session owner and same job access rule as `GET /chat/jobs/{id}`.

## Permission semantics

Permission scope is job-local:

- A session may contain jobs run with different API keys owned by the same user.
- Each job result must be interpreted under that job's API key scope.
- Follow-up/clarification for a job uses the same `chat_jobs.api_key_id`.
- Starting a new job in the same session may use another selected API key, but only if owned by the user.

## Migration

Migration steps:

1. Add `chat_sessions.user_id` nullable.
2. Backfill from `api_keys.user_id` via existing `chat_sessions.api_key_id`.
3. Add `created_by_api_key_id` if we want to preserve session creation audit separately.
4. Make `user_id` not null after backfill validation.
5. Update repositories to filter sessions/messages by `user_id`.
6. Keep existing `api_key_id` during migration or rename to `created_by_api_key_id` in a later cleanup.

## Security constraints

- Never expose raw API keys except once at creation.
- Never let a user select another user's API key id.
- Never let a user access another user's session through a known session id.
- Never reuse a previous job's broader API key scope for a new job unless explicitly selected and owned.
- Revoked/expired API keys cannot create new jobs. Existing completed job history remains visible to the session owner, but rerun/follow-up behavior must reject if the job's key is no longer usable.

## Tests required

- User with key A and key B sees the same session list.
- User with key B can read a session created using key A.
- User B cannot read user A's sessions.
- Job created with key A uses key A permissions even in a user-owned session.
- Clarification response continues with original job key.
- Revoked/expired selected key cannot start a new job.
- API key metadata list never returns raw key or hash.
