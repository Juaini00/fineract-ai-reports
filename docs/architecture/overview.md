# Architecture Overview

## Crates

The workspace contains exactly three crates:

```text
app -> core
app -> chat
chat -> core
```

- `app`: binary entrypoint and composition root.
- `core`: shared foundation: config, tracing, DB pools, auth, API primitives, response envelope, validation, and readiness.
- `chat`: chat-driven reporting. `api` owns HTTP DTOs, handlers, routes, and SSE; `conversation` owns sessions, messages, and clarification lifecycle; `job` owns durable jobs, checkpoints, events, and worker flow; `assistant` owns understanding, context, retrieval, state, execution, presentation, and LLM boundaries; `knowledge`, `policy`, and `audit` own catalog/indexing, authorization, and trace records respectively. The legacy `chat::executor` stays in place because it directly executes approved catalog SQL.

## Runtime flow

```text
client
  -> bearer session JWT auth (`role == "admin"`)
  -> chat route
  -> conversation/job service
  -> PostgreSQL durable job state
  -> assistant understanding/context/retrieval
  -> policy guard and approved plan
  -> approved SQL executor with bound office ids
  -> structured response/rendering
  -> PostgreSQL checkpoints/events + Redis live SSE state
```

Chat authorization is established by the bearer-admin identity. An optional `X-API-Key` can only voluntarily narrow office scope; it is not chat authentication and cannot cause a chat 401. Clarification continues the same job through `POST /chat/jobs/{job_id}/responses`. Approved SQL binds office scope inside SQL, PII policy remains enforced, and HTTP responses use the `{ success, data, error }` envelope with sanitized errors and English-only product text.

## Detailed docs

- [Project setup](./project-setup/index.md)
- [AI reporting design](./ai-reporting-design/index.md)
- [Chat data model](./chat-data-model/index.md)
- [RAG architecture](./rag/index.md)
- [Modern RAG blueprint](./modern-rag-blueprint/index.md)
- [Audit trail](./audit-trail/index.md)
