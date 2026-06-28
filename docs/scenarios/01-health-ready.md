# 01 — Health and Readiness

**Phase covered:** Phase 3.
**Precondition:** App running on `{{BASE_URL}}`.

## Test status

✅ Passed on 2026-06-28 via Postman MCP runner.

- Collection: `ai_report scenarios full verification corrected 2026-06-28`.
- Assertions: `health 200`, `health ok`, `ready 200`, `ready status`, `dependencies ok`.

## Liveness

```bash
curl {{BASE_URL}}/health
```

### Expected (HTTP 200)
```json
{ "status": "ok" }
```

No envelope on `/health` — it must answer even when DB pools are down.

## Readiness

```bash
curl {{BASE_URL}}/ready
```

### Expected (HTTP 200)
```json
{
  "status": "ready",
  "checks": {
    "app_database": "ok",
    "fineract_database": "ok",
    "pgvector": "ok",
    "redis": "ok"
  }
}
```

`redis` returns `"disabled"` when `REDIS_ENABLED=false`. `is_ok_or_disabled()` keeps the overall status `ready` in that case.

## Failure modes

| Trigger | Expected |
| --- | --- |
| App DB unreachable | `checks.app_database = "error"`, status `not_ready`, HTTP 503 |
| Fineract DB unreachable | `checks.fineract_database = "error"`, status `not_ready`, HTTP 503 |
| Redis enabled but down | `checks.redis = "error"`, status `not_ready`, HTTP 503 |
