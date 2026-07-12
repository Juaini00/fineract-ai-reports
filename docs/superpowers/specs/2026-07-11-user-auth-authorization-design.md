# User Authentication and Authorization Design

## Goal

Introduce first-party user authentication while keeping chat/reporting authorization on API keys. User tokens identify dashboard users; API keys continue to enforce reporting scope, capability, and PII policy.

## Existing context

- The workspace stays limited to `app`, `core`, and `chat` crates.
- `core` already owns auth primitives, API key generation, bootstrap admin token auth, and `ClientContext`.
- Chat/session/job routes already use API key authentication.
- `api_keys.owner` exists today as text-only ownership metadata.

## V1 decisions

- Login uses `username` and `password`.
- User records also store `email` and basic profile fields for future user management.
- Role V1 is only `admin`.
- Permission entities and role-permission mapping are created now, but full permission enforcement is deferred.
- Chat/reporting requests continue to use API keys through `X-API-Key` only.
- User access tokens do not replace API keys because API keys carry stricter reporting security scope.
- Creating/bootstraping a user auto-generates one default API key for that user.
- Bootstrap admin values come from env, not hardcoded migrations.

## Data model

Add a migration for:

- `users`
  - `id UUID PRIMARY KEY`
  - `username TEXT NOT NULL UNIQUE`
  - `email TEXT NULL UNIQUE`
  - `password_hash TEXT NOT NULL`
  - `full_name TEXT NULL`
  - `role TEXT NOT NULL DEFAULT 'admin'`
  - `is_active BOOLEAN NOT NULL DEFAULT true`
  - `created_at`, `updated_at`, `last_login_at`
- `permissions`
  - master permission rows for future dashboard/API-key management.
- `role_permissions`
  - `role TEXT`, `permission_id UUID`, unique pair.
- `user_sessions`
  - login/session row tied to a user.
- `refresh_tokens`
  - stores refresh token hash, session id, expiry, revoked timestamp.
- `api_keys.user_id UUID NULL REFERENCES users(id)`
  - keep `owner TEXT` for backward compatibility.

## Configuration

Update `.env.example` and `.env` with concrete local values:

- `AUTH_BOOTSTRAP_ADMIN_ENABLED=true`
- `AUTH_BOOTSTRAP_ADMIN_USERNAME=admin`
- `AUTH_BOOTSTRAP_ADMIN_PASSWORD=password123`
- `AUTH_BOOTSTRAP_ADMIN_EMAIL=admin@example.com`
- `JWT_ACCESS_SECRET=local-access-secret-change-me`
- `JWT_REFRESH_SECRET=local-refresh-secret-change-me`
- `JWT_ACCESS_TOKEN_EXPIRY_SECONDS=900`
- `JWT_REFRESH_TOKEN_EXPIRY_SECONDS=604800`
- `AUTH_REFRESH_COOKIE_NAME=refresh_token`
- `AUTH_REFRESH_COOKIE_SECURE=false` for local HTTP; production should set true.
- `AUTH_REFRESH_COOKIE_SAME_SITE=strict`
- `AUTH_REFRESH_COOKIE_PATH=/`

Production should override the bootstrap username/password/secrets through env.

## Startup bootstrap

On app startup, if `AUTH_BOOTSTRAP_ADMIN_ENABLED=true`:

1. Check whether the bootstrap username already exists.
2. If missing, hash the bootstrap password and create an active admin user.
3. Generate one default API key for that user.
4. If the user already exists, do not change the password or create duplicate keys.

This must be idempotent.

## API endpoints

Add user-auth endpoints:

- `POST /auth/login`
  - request: `username`, `password`
  - response: access token, token type, expiry, user biodata
  - sets `refresh_token` as HttpOnly Secure cookie
- `POST /auth/refresh`
  - reads refresh token cookie
  - verifies stored token hash and session status
  - returns a new access token
- `POST /auth/logout`
  - revokes the current refresh token/session
  - clears refresh cookie
- `GET /auth/me`
  - uses bearer access token
  - returns user biodata
- `POST /auth/api-keys`
  - uses bearer access token for an authenticated admin user
  - creates an API key tied to that user

There is no API-key `me` endpoint under `/auth`; API keys are for AI assistant/chat/reporting access, not dashboard identity.

## Redis use

Redis should be configured for user auth, but it must not be the durable source of truth. PostgreSQL remains authoritative for `user_sessions` and `refresh_tokens`.

Use Redis only when `REDIS_ENABLED=true` for:

- short-lived access-token/session lookup cache, keyed by token id or session id.
- refresh-token revocation fast check, backed by PostgreSQL.
- logout propagation across app instances.

If Redis is unavailable, auth should still work from PostgreSQL, with degraded performance only. Do not store raw tokens in Redis; store hashes or opaque session ids with TTL no longer than token expiry.

## Auth behavior

- Access token is passed in `Authorization: Bearer <access_token>`.
- Refresh token is stored in HttpOnly cookie and only used by refresh/logout.
- API keys are passed only in `X-API-Key` on chat/reporting routes.
- API keys are not accepted through `Authorization`.
- Refresh token is hashed before persistence.
- User-facing errors stay sanitized.
- Passwords are never logged or returned.

## Testing

Small checks:

- Bootstrap startup creates admin and one API key once.
- Admin can login and call `GET /auth/me`.
- Login sets refresh cookie.
- Refresh returns a new access token from cookie.
- Logout revokes refresh token.
- Existing chat/API-key auth still works.
- API key context includes `user_id` when tied to a user.

## Deferred

- Non-admin roles.
- Fine-grained user permission enforcement.
- Dashboard API-key management UI.
- Supporting chat requests directly with user tokens.
- Bootstrap admin command/endpoint.
