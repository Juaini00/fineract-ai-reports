# Implementation Steps: Phase 2: Database Connections

Source: `docs-old/implementation-steps.md`

## Phase 2: Database Connections

Goal: initialize database pools and verify connectivity.

Tasks:

1. Create App PostgreSQL pool using `APP_DATABASE_URL`.
2. Create Fineract PostgreSQL pool using `FINERACT_DATABASE_URL`.
3. Configure max connections.
4. Configure connection timeout.
5. Add database ping helpers.
6. Add pgvector readiness check.
7. Add Redis readiness check when `REDIS_ENABLED=true`.

App database is used for:

```text
api keys
audit logs
execution logs
token usage
catalog snapshots
jobs
vector embeddings
```

Fineract database is used for:

```text
read-only business reporting queries
```

Validation queries:

```sql
SELECT 1;
```

pgvector validation query:

```sql
SELECT extname, extversion FROM pg_extension WHERE extname = 'vector';
```

Expected result:

```text
both database pools connect successfully
pgvector extension is active in app database
```

Current status:

```text
DONE
```
