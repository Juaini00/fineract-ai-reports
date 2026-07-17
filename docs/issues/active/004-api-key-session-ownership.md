# 004 — API key/session ownership model mismatch

Status: active — design required before implementation
Severity: blocker
Area: auth | chat | sessions | jobs | client integration
Created: 2026-07-14
Resolved:

## Problem

Chat sessions are currently scoped too tightly to the API key that created them. A user can own multiple API keys, but sessions created with one key are not naturally visible when the same user uses another key.

This mixes two different concepts:

- **Session ownership**: who owns the conversation history.
- **Job execution authorization**: which API key permission scope is used to run one report/job.

The result is a confusing UX and an incorrect ownership model.

## Observed behavior

- Chat endpoints require bearer user token plus `X-API-Key`.
- `chat_sessions` stores `api_key_id` and session list/get paths filter by the current API key.
- `chat_jobs` also stores `api_key_id`, which is correct for execution/audit.
- The frontend must manually provide a raw API key, even though first-party users should be able to choose from their own API keys after login.

## Impact

- One user with multiple API keys sees fragmented session history.
- Switching API keys can hide sessions owned by the same user.
- The UI encourages storing raw API keys client-side, which is not ideal for first-party dashboard use.
- Permission scope and conversation ownership are coupled, making future key rotation/revocation behavior unclear.
- It is hard to explain why a user cannot see their own session just because the active key changed.

## Expected behavior

Sessions should belong to the authenticated user. Jobs should be authorized by a selected API key.

Target ownership model:

```text
chat_sessions.user_id = bearer user owner
chat_sessions.created_by_api_key_id = optional audit field
chat_jobs.api_key_id = key used for this job's permission scope
```

Rules:

- Session list/get/messages are scoped by bearer `user_id`.
- Creating a job requires a session owned by the bearer user.
- Creating a job also requires an API key owned by the bearer user.
- The selected API key determines capability scope, office scope, and PII permissions for that job.
- Existing external/server-to-server API key auth remains supported, but first-party dashboard clients should not need to store raw API keys.

## Client/API key UX target

First-party dashboard flow:

1. Login with bearer credentials.
2. Fetch API key metadata owned by the user.
3. Select an API key by id/prefix/name/capabilities.
4. Send chat jobs using an API key id or selected-key header, not a raw secret.

The backend must never return raw API keys after creation. Raw key is still returned once on create for external API use.

## Required design outcomes

- Clear distinction between session owner and job execution credential.
- Migration path for existing `chat_sessions.api_key_id` data.
- API-key metadata listing endpoint for first-party clients.
- Request contract for selecting an API key without exposing raw key.
- Compatibility story for existing `X-API-Key` external clients.
- Tests proving same user can see sessions across multiple keys.
- Tests proving users cannot access other users' sessions or keys.
- Tests proving job execution uses the selected key's scope.

## Non-goals

- Do not weaken API key capability, office, or PII authorization.
- Do not return raw API key material from list/get endpoints.
- Do not remove external `X-API-Key` support without a deliberate migration.
- Do not post-filter chat history in application code if the repository query can enforce ownership.
