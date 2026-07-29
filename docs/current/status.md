# Current Status

This file is the short source of truth for the current development state. Detailed phase history lives under [`docs/roadmap/phases/`](../roadmap/phases/index.md).

## Architecture state

- Rust workspace has exactly three crates: `app`, `core`, and `chat`.
- `app` is the binary entrypoint and composition root.
- `core` owns shared foundation: config, tracing, database pools, API primitives, auth, extractors, response envelope, validation, and `ClientContext`.
- `chat` owns `api`, `conversation`, durable `job`, assistant understanding/context/retrieval/state/execution/presentation/LLM boundaries, `knowledge`, `policy`, `audit`, and the approved-SQL execution repository.
- Do not add `api`, `infra`, `runtime`, `knowledge`, `reporting`, or `ai_report_*` crates yet.

## Completed foundation

- Chat sessions support owner-scoped rename and immediate soft archive; archived sessions and their job surfaces are hidden with sanitized `404` responses while persisted history is retained.

- Baseline Rust workspace.
- HTTP bootstrap, typed config, tracing, graceful startup.
- App and Fineract database pools.
- `/health` and `/ready`.
- App database migrations.
- User login, access-token `GET /auth/me`, refresh-token cookie flow, API key generation/hashing, and bearer-session JWT chat authentication requiring `role == "admin"`; optional `X-API-Key` only narrows office scope voluntarily.
- Chat session/job durable tables and state revision.
- Chat job endpoints, clarification response endpoint, background worker, Redis-backed SSE fallback behavior.
- Structured clarification contract v1 is implemented: job-scoped pending authority, UUID/revision-bound typed responses, planner/bundled fields and canonical provenance, durable job/message recovery, and matching SSE hints. Deprecated top-level options remain a compatibility projection.
- Clarification continuation treats `message` as authoritative when clients send both fields: `others` now continues missing parameters or reroutes meaningful new requests without discarding source constraints; recognized option/message conflicts re-clarify and unresolved retries are bounded.
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
- Deterministic/catalog-aware parameter extraction is partial hardening: quantity/limit, ISO and bilingual relative date ranges, currency, domain hints, and a few metric hints are merged before retrieval/execution. Relative dates are anchored to the tenant business date, preserve wall-clock provenance, and successful results disclose a differing Fineract reporting date.
- Approved-SQL execution now loads per-query `timeout_ms`, applies it with transaction-local PostgreSQL `statement_timeout`, and uses a declared `hard_cap` or `QUERY_GLOBAL_MAX_ROWS` backstop when that cap replaces a missing or over-cap row request. Truncation is explicitly surfaced as the English-only `result_truncated` warning. Within-cap per-group ranks retain their approved SQL semantics.
- Primary runtime deterministic keyword/capability fallbacks are removed; no-router operation fails closed instead of silently routing by prompt text.
- Bundle 8 remediates Bundle 7's 28 bilingual retrieval gaps against the real approved catalog: all 62 covered phrases rank their intended capability first at the unchanged 0.40 floor / 0.05 gap, while all ten loan phrases remain explicit Issue 008 Unsupported cases. It adds the office-scoped `savings_strictly_overdue_charges_clients` approved query for the real G2 filter gap, records G1's shipped `amount_levied_total` as covered, and normalizes fallback lexical coverage so broad shared terms cannot saturate unrelated candidates. An out-of-catalog request remains Unsupported with no alternatives.
- Legacy formatter, compatibility façade, and strict pipeline paths are removed; approved-SQL execution lives in `execution::repository`, semantic LLM parsing in `assistant::llm::semantic`, and LQR in `assistant::understanding::lqr`.
- LQR is available behind `LQR_ENABLED=false` and is not the default path yet.

## Reporting capability state

- Savings capabilities are implemented for totals, top-N, monthly breakdown, monthly top-N, balance summary, pending-charge detail, and strictly-overdue charge detail.
- Client domain has 10 currently approved executable capabilities.
- Organization domain has 9 currently approved executable capabilities.
- Catalog currently records 31 approved capabilities and 31 approved queries.
- Group/center remains conditional.
- Loan, accounting/GL, tax, custom datatables, and audit/users/operations remain deferred until domain promotion.

## Known documentation caveat

Some old long-form docs still reference older catalog snapshots such as 16/16 or classifier-first runtime behavior. This current-status file and the synchronized architecture docs take precedence. Deterministic extraction remains partial hardening, not complete.
