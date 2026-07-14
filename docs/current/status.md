# Current Status

This file is the short source of truth for the current development state. Detailed phase history lives under [`docs/roadmap/phases/`](../roadmap/phases/index.md).

## Architecture state

- Rust workspace has exactly three crates: `app`, `core`, and `chat`.
- `app` is the binary entrypoint and composition root.
- `core` owns shared foundation: config, tracing, database pools, API primitives, auth, extractors, response envelope, validation, and `ClientContext`.
- `chat` owns chat/reporting routes, services, repositories, job pipeline, catalog use, retrieval, planner, policy guard, executor, structured assistant responses/rendering, and checkpoint/event handling.
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
- Semantic assistant components are present in this working tree: graph runtime, semantic router boundary, source-intent clarification preservation, retrieval/evidence selection, guarded approved-catalog SQL tool execution, policy blocking, structured response authority, markdown rendering, and scenario/golden contract coverage are wired.
- Deterministic/catalog-aware parameter extraction is partial hardening: quantity/limit, ISO date ranges, currency, domain hints, and a few metric hints are merged before retrieval/execution, with broader coverage and provenance tests still pending.
- Primary runtime deterministic keyword/capability fallbacks are removed; no-router operation fails closed instead of silently routing by prompt text.
- Legacy formatter-first paths are deleted/quarantined; `chat/formatter/labels.rs` remains only as a label utility if referenced.
- LQR is available behind `LQR_ENABLED=false` and is not the default path yet.

## Reporting capability state

- Savings capabilities are implemented for totals, top-N, monthly breakdown, monthly top-N, and balance summary.
- Client domain has 7 currently approved executable capabilities.
- Organization domain has 8 currently approved executable capabilities.
- Catalog after Phase 21 records 25 capabilities and 25 queries.
- Group/center remains conditional.
- Loan, accounting/GL, tax, custom datatables, and audit/users/operations remain deferred until domain promotion.

## Known documentation sync issue

Some old long-form docs referenced older catalog snapshots such as 16/16, classifier-first runtime behavior, or overstated semantic assistant completion. Current docs should treat the assistant as partial foundation plus active full-brain migration until the target architecture plan passes its acceptance gates. Deterministic extraction is partial hardening, not complete.
