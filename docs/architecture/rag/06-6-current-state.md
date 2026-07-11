# RAG Architecture: 6. Current State

Source: `docs-old/rag-architecture.md`

## 6. Current State

### Done

- Schema for `knowledge_catalog_versions` and `knowledge_index` with deterministic content hashing, source uniqueness, GIN over metadata, and ivfflat over the embedding column.
- Catalog loader and validator covering data areas, domains, capabilities, queries, schema, metrics, policies, and responses.
- Retrieval document builder that flattens catalog entries into searchable text with stable metadata.
- Voyage embedding client for document embeddings.
- Sync orchestration that persists retrieval documents, fills embeddings when startup sync is enabled, and records an `indexed` or `embedded` catalog version.
- Runtime query embedding and capability/query vector classification in chat job creation.
- `classification.source` and `classification.candidates` persisted in `chat_jobs.state_json` for manual verification.
- `ExecutionPlan` stores structured Modern RAG stage outputs: retrieval plan, evidence evaluation, and answer plan.

### Pending

- Field-specific typed Rust schemas for metric / schema / policy / response YAML.
- LLM planner fallback over broader retrieved context beyond approved clarification options. Context candidates are already attached to `chat_jobs.state_json.classification.candidates` with their `source_type` for future planner consumption.

### Blueprint Alignment Status

This table tracks the gap between the current implementation and `docs/Modern_RAG_Architecture_Blueprint.md`.

| Blueprint component | Current status | Notes |
| --- | --- | --- |
| Conversation Context | Done | Chat sessions, messages, jobs, checkpoints, and job state are persisted in PostgreSQL. Redis is only used for live progress/SSE. |
| Semantic Parser | Partial | Runtime classification produces structured `ClassificationResult` with outcome, domain, capability, confidence, params, options, source, and candidates. It is currently deterministic/vector-first with constrained LLM fallback only for approved clarification options. |
| Intent Router | Done | Rust routes matched, clarification, and unsupported outcomes deterministically before execution. Write/tool-like intents are blocked from reporting execution. |
| Entity & Constraint Resolver | Partial | Capability, query, date period, office scope, API-key capability scope, and PII constraints are resolved. General project/module/ticket/entity resolution from arbitrary knowledge is not implemented. |
| Ambiguity Detector | Done | Confidence floor/gap policy and missing required report parameters route to clarification instead of execution. |
| Retrieval Planner | Partial | `ExecutionPlan` stores `retrieval_plan` with vector query, keyword query, graph query string, and metadata filter. These are structured audit/planning artifacts; they do not yet drive separate BM25 or graph engines. |
| Vector Search | Done | Runtime query embeddings search pgvector `knowledge_index` over the latest indexed/embedded catalog version and restrict executable candidates to allowed capabilities. |
| Keyword/BM25 | Partial | Catalog lexical retrieval exists as a no-dependency fallback. A true BM25 index/scorer is not implemented. |
| Graph Search | Pending | `ExecutionPlan.retrieval_plan.graph_query` records the intended graph path, but no graph search engine exists yet. |
| Metadata Filter | Partial | Metadata filters are captured in the plan and capability scope is enforced in retrieval/policy. Separate metadata-filtered retrieval passes are not implemented yet. |
| Hybrid Retrieval | Partial | Current flow combines vector capability search, non-capability context search, and lexical fallback. It does not yet merge independent vector + BM25 + graph + metadata result sets. |
| Reranker | Partial | Current ranking uses vector distance plus deterministic confidence/gap policy. No cross-encoder/reranker model is implemented. |
| Evidence Evaluator | Partial | `ExecutionPlan.evidence_evaluation` records confidence sufficiency, source count, and source types. Contradiction checks, required-source checks, and retry decisions are not implemented. |
| Retrieval Retry | Pending | Weak evidence currently clarifies or fails unsupported; it does not automatically retry with expanded retrieval plans. |
| Answer Planner | Partial | `ExecutionPlan.answer_plan` records section structure by output mode. It is deterministic and not yet LLM-generated. |
| LLM Answer Generator | Pending | Approved SQL results are formatted by deterministic Rust formatters. LLM response formatting/generation for complex responses is still pending. |
| Grounded Response | Partial | Responses are grounded in approved SQL output and policy-filtered fields. Full generated-answer grounding over retrieved evidence is pending until LLM answer generation exists. |

### Sequencing Rule

Voyage embedding sync can run before runtime retrieval because it only stores derived catalog vectors. Runtime vector retrieval must not execute anything directly; retrieved capabilities still need catalog validation, SQL safety validation, API-key capability scope, office scope, and PII policy checks.
