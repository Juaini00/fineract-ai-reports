# Current Status

This file is the short source of truth for the current development state. Detailed phase history lives under [`docs/roadmap/phases/`](../roadmap/phases/index.md).

## Architecture state

- Rust workspace has exactly three crates: `app`, `core`, and `chat`.
- `app` is the binary entrypoint and composition root.
- `core` owns shared foundation: config, tracing, database pools, API primitives, auth, extractors, response envelope, validation, and `ClientContext`.
- `chat` owns chat/reporting routes, services, repositories, job pipeline, catalog use, retrieval, planner, policy guard, executor, formatter, and checkpoint/event handling.
- Do not add `api`, `infra`, `runtime`, `knowledge`, `reporting`, or `ai_report_*` crates yet.

## Completed foundation

- Baseline Rust workspace.
- HTTP bootstrap, typed config, tracing, graceful startup.
- App and Fineract database pools.
- `/health` and `/ready`.
- App database migrations.
- User login, access-token `GET /auth/me`, refresh-token cookie flow, API key generation/hashing, and `X-API-Key` chat authentication.
- Chat session/job durable tables and state revision.
- Chat job endpoints, clarification response endpoint, background worker, Redis-backed SSE fallback behavior.
- Authorization helpers for capability, office scope, and PII.

## Catalog, retrieval, and assistant state

- `knowledge/` folders exist for data scope, domains, schema, metrics, capabilities, queries, policies, and responses.
- Runtime catalog loader and validator exist under `crates/chat/src/knowledge/catalog`.
- Generic knowledge loading covers schema, metric, policy, and response layers.
- Typed field-specific schemas for those generic layers remain pending.
- Runtime SQL validation is wired into `POST /catalog/validate`.
- Vector index persistence exists through `knowledge_catalog_versions` and `knowledge_index`.
- Voyage embeddings run when configured.
- Runtime vector capability search and lexical fallback are wired into chat job creation.
- Broader non-capability retrieval context is appended for audit/future LLM use; it does not execute SQL.
- Semantic assistant foundation is partially implemented. The runtime has structured assistant types, structured responses, some job/session memory, catalog-aware execution, policy guards, and regression coverage for recent chat bugs.
- Semantic assistant migration is not complete: `petgraph` is not yet the graph control plane, `rig-core` is not yet the primary runtime agent/tool boundary, clarification still lacks clean source-intent snapshot preservation, context carry-over still uses tactical glue in places, and scenario coverage is not a full acceptance matrix.
- Tactical deterministic/manual fallback paths remain migration debt and must be deleted or quarantined before the semantic assistant platform can be called complete.
- LQR is available behind `LQR_ENABLED=false` and is not the default path yet.

## Reporting capability state

- Savings capabilities are implemented for totals, top-N, monthly breakdown, monthly top-N, and balance summary.
- Client domain has 7 approved MVP capabilities.
- Organization domain has 8 approved MVP capabilities.
- Catalog after Phase 21 records 25 capabilities and 25 queries.
- Group/center remains conditional.
- Loan, accounting/GL, tax, custom datatables, and audit/users/operations remain deferred until domain promotion.

## Known documentation sync issue

Some old long-form docs referenced older catalog snapshots such as 16/16, classifier-first runtime behavior, or overstated semantic assistant completion. Current docs should treat the assistant as partial foundation plus active full-brain migration until the target architecture plan passes its acceptance gates.
