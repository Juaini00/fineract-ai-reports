# Implementation Steps: Phase 5: API Key Generation

Source: `docs-old/implementation-steps.md`

## Phase 5: API Key Generation

Goal: allow creating API keys for clients.

Endpoint:

```text
POST /auth/api-keys
```

Protection:

```text
Authorization: Bearer <AUTH_BOOTSTRAP_ADMIN_TOKEN>
```

Request:

```json
{
  "name": "local-dev-client",
  "owner": "Antun",
  "expires_at": null,
  "allowed_office_ids": [1, 2, 3],
  "allowed_capabilities": [
    "savings_deposit_total",
    "savings_deposit_top_n"
  ],
  "can_view_pii": true
}
```

Response:

```json
{
  "success": true,
  "data": {
    "id": "...",
    "api_key": "air_test_...",
    "message": "Store this API key securely. It will not be shown again."
  },
  "error": null
}
```

Implementation rules:

1. Generate a cryptographically secure random secret.
2. Prefix key using `API_KEY_PREFIX`.
3. Hash the full raw key.
4. Store only the hash and metadata.
5. Return raw key only once.
6. Store visible prefix for debugging.

Validation:

```bash
curl -X POST http://127.0.0.1:3007/auth/api-keys \
  -H "Authorization: Bearer local-admin-token" \
  -H "Content-Type: application/json" \
  -d '{"name":"local-dev-client","owner":"Antun","allowed_office_ids":[1],"allowed_capabilities":["savings_deposit_total"],"can_view_pii":true}'
```

Use local port `3007`:

```bash
curl -X POST http://127.0.0.1:3007/auth/api-keys \
  -H "Authorization: Bearer local-admin-token" \
  -H "Content-Type: application/json" \
  -d '{"name":"local-dev-client","owner":"Antun","allowed_office_ids":[1],"allowed_capabilities":["savings_deposit_total"],"can_view_pii":true}'
```

Expected result:

```text
raw API key is returned once
hashed key is stored in database
```

Current implementation notes:

```text
route -> AuthService -> ApiKeyRepository -> PostgreSQL
request validation uses validator crate + global ValidatedJson extractor
responses use a consistent success/data/error envelope
```

Current status:

```text
DONE
```
