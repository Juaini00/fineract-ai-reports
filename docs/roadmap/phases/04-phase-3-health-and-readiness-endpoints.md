# Implementation Steps: Phase 3: Health And Readiness Endpoints

Source: `docs-old/implementation-steps.md`

## Phase 3: Health And Readiness Endpoints

Goal: expose basic service status before implementing business features.

Endpoints:

```text
GET /health
GET /ready
```

`/health` checks:

```text
server process is alive
```

`/ready` checks:

```text
app database connection
fineract database connection
pgvector extension availability
optional redis connection if REDIS_ENABLED=true
```

Example `/health` response:

```json
{
  "status": "ok"
}
```

Example `/ready` response:

```json
{
  "status": "ready",
  "checks": {
    "app_database": "ok",
    "fineract_database": "ok",
    "pgvector": "ok",
    "redis": "disabled"
  }
}
```

Validation:

```bash
curl http://127.0.0.1:3007/health
curl http://127.0.0.1:3007/ready
```

Current status:

```text
DONE
```
