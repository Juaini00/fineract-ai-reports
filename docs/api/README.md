# API Documentation

Use scenario docs for executable request flows. This directory is the lightweight endpoint map.

## Endpoint groups

- Health/readiness: `GET /health`, `GET /ready` — see [`scenarios/01-health-ready.md`](../scenarios/01-health-ready.md)
- Auth/API keys: `POST /auth/api-keys`, `GET /auth/me` — see [`scenarios/02-auth-api-keys.md`](../scenarios/02-auth-api-keys.md)
- Catalog validation: `POST /catalog/validate` — see [`scenarios/03-catalog-validate.md`](../scenarios/03-catalog-validate.md)
- Vector index: `POST /vector-index/rebuild`, `GET /vector-index/status` — see [`scenarios/04-vector-index.md`](../scenarios/04-vector-index.md)
- Chat: sessions, jobs, SSE, clarification responses — see [`scenarios/05-chat-session-and-job.md`](../scenarios/05-chat-session-and-job.md)
- Frontend session management: payloads, response types, rename/delete behavior, and TypeScript examples — see [`current/frontend-session-management.md`](../current/frontend-session-management.md)
