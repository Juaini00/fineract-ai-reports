# AI Reporting Service Design: 11. Storage Architecture

Source: `docs-old/ai-reporting-design.md`

## 11. Storage Architecture

### 11.1 Fineract Read Replica

Business data source. Read-only access only.

### 11.2 Application PostgreSQL

Application database: `ai_reports`.

Stores:

1. Catalog snapshots.
2. API key metadata and hashed API keys.
3. Audit logs.
4. Execution logs.
5. Token usage.
6. Performance history.
7. Async jobs.
8. Durable report results.

### 11.3 pgvector

Vector extension inside the application PostgreSQL database.

Stores embeddings for:

1. Domain knowledge.
2. Capability knowledge.
3. Glossary.
4. Unsupported intents.
5. Schema summaries.

### 11.4 Redis

Enabled for local development through Docker Compose.

Used for:

1. Live chat job progress.
2. Short-lived SSE event state.
3. Job locks.
4. Rate limiting later.
5. Query estimate cache later.

Current local config:

```text
REDIS_ENABLED=true
REDIS_URL=redis://127.0.0.1:6380/0
```
