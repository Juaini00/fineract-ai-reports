# Test Scenarios

Manual end-to-end scenarios for the AI Reporting Service, ordered from foundation to feature. Each file is a self-contained playbook with a precondition, the curl call, the expected response shape, and any side effect to verify (DB rows, Redis keys, etc.).

Use the same Postman variables as `AGENTS.md`:

```text
BASE_URL=http://127.0.0.1:3007
LOCAL_ADMIN_TOKEN=local-admin-token
API_KEY=<set after 02-auth-api-keys>
SESSION_ID=<set after 05-chat-session-and-job>
JOB_ID=<set after 05-chat-session-and-job>
```

## Index

| File | Covers |
| --- | --- |
| `00-setup.md` | Local env, Docker Redis, migrations, bootstrap admin token |
| `01-health-ready.md` | `GET /health`, `GET /ready` |
| `02-auth-api-keys.md` | API key creation + `GET /auth/me` |
| `03-catalog-validate.md` | `POST /catalog/validate` (Phase 10 + Phase 11 runtime prepare) |
| `04-vector-index.md` | `POST /vector-index/rebuild`, `GET /vector-index/status` (Phase 18 admin) |
| `05-chat-session-and-job.md` | Sessions + happy-path job execute + SSE (Phase 8/9/13/14/16) |
| `06-chat-clarification-and-unsupported.md` | Decision policy: clarify + unsupported + clarification respond |
| `07-authorization-scope.md` | Capability gate, office scope in SQL, PII rules |

## How to use

1. Run `00-setup.md` once per dev environment.
2. Run `01` → `04` to bring up the foundation.
3. Run `05`–`07` to exercise the chat feature. Each chat scenario assumes you have an `API_KEY` from `02`.
4. After implementing a new phase, add an `08-*.md` (etc.) following the same template. For minor code edits, update the existing file in place rather than adding a new one.

## Scenario template

```markdown
# <Scenario name>

**Phase covered:** <e.g. Phase 9>
**Precondition:** <state needed before this scenario runs>

## Request
[curl block]

## Expected response (HTTP <status>)
[JSON shape — show fields, not literal values where they vary]

## Side effects
- DB: <table / column / row condition>
- Redis: <key / TTL / value shape>
- Logs: <tracing span / event to watch>

## Failure modes
| Trigger | Expected response |
| --- | --- |
| ... | ... |
```
