# 02 — Auth + API Keys

**Phase covered:** Phase 5–6.
**Precondition:** `LOCAL_ADMIN_TOKEN` from `00-setup.md`.

## Test status

✅ Passed on 2026-06-28 via Postman MCP runner.

- API key creation returned HTTP 201 and a one-time raw `data.api_key`.
- `GET /auth/me` returned HTTP 200 with the authenticated client under `data.client`.

## Create API key (bootstrap-gated)

```bash
curl -X POST {{BASE_URL}}/auth/api-keys \
  -H "Authorization: Bearer {{LOCAL_ADMIN_TOKEN}}" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "local-dev-client",
    "owner": "Antun",
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
- DB `api_keys`: row with `key_hash`, `key_prefix`, scopes, `revoked_at=null`. Raw key never stored.

## Verify identity

```bash
curl {{BASE_URL}}/auth/me -H "Authorization: Bearer {{API_KEY}}"
# or:
curl {{BASE_URL}}/auth/me -H "X-API-Key: {{API_KEY}}"
```

### Expected (HTTP 200)
```json
{
  "success": true,
  "data": {
    "auth_type": "api_key",
    "client": {
      "api_key_id": "key_...",
      "owner": "Antun",
      "allowed_office_ids": [1, 2, 3],
      "allowed_capabilities": ["savings_deposit_total", "savings_deposit_top_n", "savings_withdrawal_total", "savings_withdrawal_top_n", "savings_deposit_monthly_breakdown", "savings_deposit_monthly_top_n", "savings_withdrawal_monthly_breakdown", "savings_withdrawal_monthly_top_n", "savings_balance_summary", "organization_office_summary", "client_lifecycle_summary"],
      "can_view_pii": true
    }
  },
  "error": null
}
```

## Failure modes

| Trigger | Expected response |
| --- | --- |
| Wrong / missing `LOCAL_ADMIN_TOKEN` on create | HTTP 401 `error.code=unauthorized` |
| Wrong API key on `/auth/me` | HTTP 401 `error.code=unauthorized` |
| Revoked key (`POST /auth/api-keys/{id}/revoke`) | HTTP 401 on next `/auth/me` |
| Expired key | HTTP 401 |
