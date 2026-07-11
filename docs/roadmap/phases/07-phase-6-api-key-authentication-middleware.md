# Implementation Steps: Phase 6: API Key Authentication Middleware

Source: `docs-old/implementation-steps.md`

## Phase 6: API Key Authentication Middleware

Goal: protect all reporting and admin endpoints except health/readiness and key creation.

Supported headers:

```text
Authorization: Bearer <api_key>
X-API-Key: <api_key>
```

Runtime flow:

```text
extract API key
hash API key
find matching key_hash
check revoked_at is null
check expires_at is valid
load scopes
build ClientContext
attach ClientContext to request
```

Client context:

```json
{
  "api_key_id": "key_...",
  "owner": "Antun",
  "allowed_office_ids": [1],
  "allowed_capabilities": ["savings_deposit_total"],
  "can_view_pii": true
}
```

Validation endpoint for middleware:

```text
GET /auth/me
```

Expected response:

```json
{
  "api_key_id": "key_...",
  "owner": "Antun",
  "allowed_capabilities": ["savings_deposit_total"]
}
```

Validation:

```bash
curl http://127.0.0.1:3007/auth/me \
  -H "Authorization: Bearer <generated_api_key>"
```

Current status:

```text
DONE
```
