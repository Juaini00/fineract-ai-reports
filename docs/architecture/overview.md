# Architecture Overview

## Crates

```text
app -> core
app -> chat
chat -> core
```

- `app`: binary entrypoint and composition root.
- `core`: shared foundation: config, tracing, DB pools, auth, API primitives, response envelope, validation, and readiness.
- `chat`: chat-driven reporting feature: API, jobs, catalog access, retrieval, planner, policy, executor, formatter, checkpoints, and events.

## Runtime flow

```text
client
  -> API key auth
  -> chat route
  -> service
  -> durable job state
  -> catalog/vector retrieval
  -> planner
  -> policy guard
  -> approved SQL executor
  -> formatter
  -> checkpoints/events
```

## Detailed docs

- [Project setup](./project-setup/index.md)
- [AI reporting design](./ai-reporting-design/index.md)
- [Chat data model](./chat-data-model/index.md)
- [RAG architecture](./rag/index.md)
- [Modern RAG blueprint](./modern-rag-blueprint/index.md)
- [Audit trail](./audit-trail/index.md)
