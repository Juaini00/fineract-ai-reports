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

| File | Covers | Latest test status |
| --- | --- | --- |
| `00-setup.md` | Local env, Docker Redis, migrations, bootstrap admin token | ✅ Passed 2026-06-28 rerun |
| `01-health-ready.md` | `GET /health`, `GET /ready` | ✅ Passed 2026-06-28 rerun |
| `02-auth-api-keys.md` | API key creation + `GET /auth/me` | ✅ Passed 2026-06-28 rerun |
| `03-catalog-validate.md` | `POST /catalog/validate` (Phase 10 + Phase 11 runtime prepare) | ✅ Passed 2026-06-28 rerun |
| `04-vector-index.md` | `POST /vector-index/rebuild`, `GET /vector-index/status` (Phase 18 admin) | ✅ Passed 2026-06-28 rerun |
| `05-chat-session-and-job.md` | Sessions + happy-path job execute + SSE (Phase 8/9/13/14/16) | ✅ Passed 2026-06-28 rerun |
| `06-chat-clarification-and-unsupported.md` | Decision policy: clarify + unsupported + clarification respond + deferred-domain detection (loan/accounting/tax/group_center) | ✅ Passed 2026-06-28 rerun |
| `07-authorization-scope.md` | Capability gate, office scope in SQL, PII rules | ✅ Passed 2026-06-28 rerun |

## Latest run

- ✅ Postman collection run `ai_report scenarios expanded verification 2026-06-28`: 23/23 assertions passed for `01`–`05`.
- ✅ Setup checks passed: Redis returned `PONG`; PostgreSQL has the `vector` extension.
- ✅ `07` rerun passed: narrow scope bound `office_ids=[1]`, full scope bound `[1,2,3]`, and cross-key read returned HTTP 404.
- ✅ `06` rerun passed after the off-domain override fix: C now reaches `no_allowed_capabilities`; loan/accounting/group-center return `off_domain_match`; tax returns `vector_no_match`.

## Knowledge expansion (post 2026-06-28)

The catalog now covers more than savings. New scenario expectations (re-run after `POST /vector-index/rebuild`):

- `03`: `data_areas=13, domains=7, capabilities=2, queries=2`.
- `04`: `document_count=24` (`13 data_area + 7 domain + 2 capability + 2 query`).
- `05` Top-N: result rows now include `client_id` + `client_display_name` (LEFT JOIN m_client; `client_display_name` is `pii` in the output contract).
- `06` section E: deferred-domain retrieval is visible in candidates and off-domain override returns `unsupported` for the documented loan/accounting/group-center prompts.

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
