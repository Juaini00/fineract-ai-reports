# Chat Data Model: 1. Storage Principles

Source: `docs-old/chat-data-model.md`

## 1. Storage Principles

Use three storage layers with different responsibilities:

```text
PostgreSQL -> durable source of truth
Redis      -> live progress / temporary coordination
Memory     -> temporary data during the current request only
```

Rules:

1. If state is required to resume a job, store it in PostgreSQL.
2. If state is only for live UI progress, store it in Redis.
3. Do not hold DB connections while waiting for user input or streaming SSE.
4. Do not store raw API keys.
5. Do not store raw SQL or hidden prompts in user-visible payloads.
6. Keep large report results out of chat messages; store them as job result payloads or report result records.
