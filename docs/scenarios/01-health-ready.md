# 01 — Health and Readiness

**Phase covered:** Phase 3.
**Precondition:** App running on `{{BASE_URL}}`.

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
