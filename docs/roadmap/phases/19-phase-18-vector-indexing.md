# Implementation Steps: Phase 18: Vector Indexing

Source: `docs-old/implementation-steps.md`

## Phase 18: Vector Indexing

Goal: add semantic knowledge retrieval after catalog is stable.

Reference design for the full RAG pipeline (indexing + runtime retrieval):

```text
docs/rag-architecture.md
docs/Modern_RAG_Architecture_Blueprint.md
```

Initial vector content:

```text
domain knowledge
capability descriptions
example questions
synonyms
unsupported intents
schema summaries
```

Do not vectorize transactional Fineract rows.

Endpoint:

```text
POST /vector-index/rebuild
GET  /vector-index/status
```

Current status:

```text
PARTIALLY STARTED

Database tables exist for knowledge_catalog_versions and knowledge_index.
Retrieval document hashes and index persistence exist.
Voyage document embeddings are generated when catalog startup sync is enabled.
Runtime query embedding and capability vector search are wired into chat job creation.
Catalog lexical retrieval is used as a fallback when embedding/vector search is unavailable.
Vector search is restricted to rows that can map back to the caller's allowed_capabilities.
Capability rows and query rows can both select approved capabilities; query candidates are mapped back to their owning capability before planning.
Vector search uses the latest indexed/embedded catalog version and collapses duplicate capability ids.
Current confidence policy: <0.40 unsupported, 0.40-0.55 clarify, close candidates within 0.05 clarify, clear >=0.55 can execute after policy checks.
Classification state records source (`local_rule`, `vector`, or clarification source) and vector candidates for manual verification.
POST /vector-index/rebuild and GET /vector-index/status are implemented (authenticated; rebuild runs KnowledgeSyncService::with_embeddings, status returns the latest knowledge_catalog_versions row).
Broader retrieval: KnowledgeRepository::search_context queries non-capability rows from the latest indexed catalog version; results are appended to classification.candidates with their source_type for audit and future LLM planner consumption — they do not directly execute SQL.
ExecutionPlan now records structured Modern RAG stage outputs: retrieval_plan, evidence_evaluation, and answer_plan.

Important sequencing rule:
Vector retrieval only selects knowledge candidates that resolve to approved capabilities. SQL execution still goes through catalog validation, policy guard, and static approved SQL bindings.
```
