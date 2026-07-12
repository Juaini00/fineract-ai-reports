# 02 — Auth + API Keys

**Phase covered:** Phase 5–6.
**Precondition:** logged-in admin user access token from `POST /auth/login`.

## Test status

✅ Passed on 2026-06-28 via Postman MCP runner.

- API key creation returns HTTP 201 and a one-time raw `data.api_key`.
- Chat/reporting endpoints authenticate API keys only through `X-API-Key`.

## Create API key (dashboard user-gated)

```bash
curl -X POST {{BASE_URL}}/auth/api-keys \
  -H "Authorization: Bearer {{ACCESS_TOKEN}}" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "local-dev-client",
    "allowed_office_ids": [1, 2, 3],
    "allowed_capabilities": [
      "savings_deposit_total",
      "savings_deposit_top_n",
      "savings_withdrawal_total",
      "savings_withdrawal_top_n",
      "savings_deposit_monthly_breakdown",
      "savings_deposit_monthly_top_n",
      "savings_withdrawal_monthly_breakdown",
      "savings_withdrawal_monthly_top_n",
      "savings_balance_summary",
      "organization_office_summary",
      "client_lifecycle_summary"
    ],
    "can_view_pii": true
  }'
```

### Expected (HTTP 201)
```json
{
  "success": true,
  "data": {
    "id": "key_...",
    "api_key": "air_test_...",
    "key_prefix": "air_test_...",
    "message": "Store this API key securely. It will not be shown again."
  },
  "error": null
}
```

Copy `data.api_key` into `{{API_KEY}}`. **This is the only time the raw key is visible.**

## Side effects
- DB `api_keys`: row with `user_id`, `owner` derived from the authenticated user, `key_hash`, `key_prefix`, scopes, `revoked_at=null`. Raw key never stored.

## Use API key for chat/reporting

```bash
curl -X POST {{BASE_URL}}/chat/sessions \
  -H "X-API-Key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{ "title": "Savings report" }'
```

### Expected (HTTP 200)
```json
{
  "success": true,
  "data": { "id": "session_...", "title": "Savings report" },
  "error": null
}
```

## Failure modes

| Trigger | Expected response |
| --- | --- |
| Wrong / missing `ACCESS_TOKEN` on create | HTTP 401 `error.code=unauthorized` |
| API key sent through `Authorization` instead of `X-API-Key` on chat routes | HTTP 401 `error.code=unauthorized` |
| Wrong API key on chat routes | HTTP 401 `error.code=unauthorized` |
| Revoked key (`POST /auth/api-keys/{id}/revoke`) | HTTP 401 on next chat request |
| Expired key | HTTP 401 |
