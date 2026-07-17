# Implementation plan — API key and chat session ownership

Date: 2026-07-14
Spec: `docs/superpowers/specs/2026-07-14-api-key-session-ownership-design.md`
Issue: `docs/issues/active/004-api-key-session-ownership.md`

## Phase 1 — Schema migration

- Add `chat_sessions.user_id` and optional `created_by_api_key_id`.
- Backfill `chat_sessions.user_id` from `api_keys.user_id` using existing `chat_sessions.api_key_id`.
- Add indexes for `chat_sessions(user_id, updated_at)`.
- Keep old `api_key_id` until code is migrated and data is verified.
- Gate: migration runs on fresh and existing dev DB.

## Phase 2 — Core auth/API key metadata

- Add safe API key metadata list endpoint for bearer users.
- Return id, name/prefix, capabilities, offices, PII flag, timestamps, expiry/revocation status.
- Never return raw key or hash from list/get endpoints.
- Gate: tests prove no raw secret fields are returned.

## Phase 3 — Session repository/service ownership

- Change session create/list/get to use bearer `user_id` as owner.
- Store selected/current API key only as creation audit.
- Change message list to authorize through session `user_id`.
- Gate: one user with two keys sees the same sessions through both keys.

## Phase 4 — Job creation with selected execution key

- Add first-party selected-key contract: header `X-API-Key-Id` or body `api_key_id`.
- Validate selected key belongs to bearer user and is active.
- Validate session belongs to bearer user.
- Store selected key in `chat_jobs.api_key_id` and use it for policy/capability/office/PII.
- Gate: job created with key A in user-owned session uses key A scope.

## Phase 5 — Job read/respond/stream authorization

- Authorize job access through `chat_jobs.session_id -> chat_sessions.user_id` for bearer users.
- Clarification response continues with original `chat_jobs.api_key_id`.
- SSE stream follows the same authorization as job get.
- External raw-key mode remains compatible, but must not cross user boundaries.
- Gate: cross-user session/job access is rejected.

## Phase 6 — Client integration update

- Update `docs/current/chat-client-integration.md`.
- Frontend flow: login → fetch API key metadata → select key → create/read sessions by user → create jobs using selected key id.
- Recommend storing selected `api_key_id`, not raw API key, in local storage.
- Gate: docs show no manual raw-key entry requirement for first-party UI.

## Phase 7 — Compatibility and cleanup

- Keep `X-API-Key` raw mode for external clients.
- Add deprecation notes if existing first-party UI used raw key entry.
- After validation, decide whether to rename/drop `chat_sessions.api_key_id` in a later migration.
- Gate: old external API-key tests and new first-party selected-key tests both pass.

## Acceptance gates

- Same bearer user can list/read sessions across all owned keys.
- Different bearer users cannot access each other's sessions/jobs/messages.
- Job execution permission always comes from the selected/stored job API key.
- Raw API key is returned once only on create.
- First-party UI can operate without manually entering raw API keys.
