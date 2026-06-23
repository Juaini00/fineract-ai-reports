# RAG Architecture

This document explains how Retrieval-Augmented Generation (RAG) works in the AI Reporting Service. It is a companion to `docs/ai-reporting-design.md` (overall design) and `docs/knowledge-catalog.md` (knowledge layers and YAML contracts). Read those first if you have not.

## 1. What "RAG" Means In This System

This service uses **constrained RAG**. Retrieval does not generate answers or SQL. It only helps the planner pick from a fixed, human-approved set of reporting capabilities.

Standard RAG vs. this system:

| Aspect | Standard RAG | This system |
| --- | --- | --- |
| What is retrieved | Free-form document chunks | Structured knowledge entries (data area, domain, capability, query, schema, metric) |
| What the LLM does with retrieval | Synthesize a free-form answer | Pick a `capability_id`; the answer comes from approved SQL output |
| Source of executable logic | Whatever the LLM writes | Pre-reviewed SQL files in `queries/` |
| Authority of vector search | Drives the answer | Only ranks candidates; Rust policy guard decides |
| Failure mode if retrieval is wrong | Hallucinated answer | `unsupported` or `clarify` response |

The principle: **vector search finds relevant knowledge, never executes a decision**. Authorization, capability selection, and SQL execution stay in Rust.

## 2. Two Pipelines

RAG in this system splits cleanly into a build-time **indexing pipeline** and a runtime **retrieval pipeline**.

```text
INDEXING (build / startup / admin trigger)
  YAML catalog
    -> typed load + validate (catalog/loader.rs, catalog/validator.rs)
    -> retrieval documents (retrieval.rs)
    -> SHA-256 content hash + catalog version
    -> Postgres: knowledge_catalog_versions + knowledge_index
    -> embedding sync (Voyage, 1024-dim, optional startup trigger)
    -> pgvector ivfflat index

RETRIEVAL (per user message)
  user message
    -> API key auth + ClientContext
    -> normalize text
    -> embed query (Voyage)             [pending]
    -> top-k vector search over knowledge_index
    -> filter to candidates the API key can use
    -> local classifier / DeepSeek planner picks capability_id
    -> policy guard (capability / office / PII)
    -> bind params to approved SQL in queries/
    -> execute on Fineract read-only pool
    -> format response from query output contract
    -> audit log (capability_id, query_id, latency, decision)
```

## 3. Indexing Pipeline (Build Side)

### 3.1 Inputs

- `knowledge/data-scope/**/*.yaml` — what Fineract areas are in scope.
- `knowledge/domains/*.yaml` — business language, synonyms, supported/unsupported intents.
- `knowledge/capabilities/**/*.yaml` — what reports are executable.
- `knowledge/queries/**/*.yaml` + `queries/**/*.sql` — approved SQL bound to each capability.
- (Later) `knowledge/schema/`, `knowledge/metrics/`, `knowledge/policies/`, `knowledge/responses/`.

The source of truth is human-edited YAML and SQL, not the database. The database holds derived artifacts only.

### 3.2 Steps

1. **Load and validate the catalog.** Typed Rust structs parse each YAML file. Cross-references (capability → query → SQL file, capability → data area, etc.) must resolve. See `docs/knowledge-catalog.md` §5 for the full validation matrix.
2. **Build retrieval documents.** Each catalog entry becomes one `RetrievalDocument` with: `source_type` (one of `data_area | domain | capability | query | schema | metric | policy | response`), `source_id`, a stable `title`, a flattened `retrieval_text` (description + synonyms + example phrases + metric meanings — the text that will be embedded), and a JSON `metadata` blob for runtime filtering.
3. **Hash for determinism.** Each document gets a SHA-256 over its retrieval text + metadata. The whole catalog also gets a content hash (`knowledge_catalog_versions.content_hash`, which is `UNIQUE`).
4. **Persist.** Documents land in `knowledge_index` linked to a `knowledge_catalog_versions` row. The catalog version progresses through statuses: `loaded → validated → indexed → embedded`. `failed` is terminal.
5. **Embed.** When `CATALOG_SYNC_ON_STARTUP=true`, the sync calls Voyage AI and fills `knowledge_index.embedding` (`vector(1024)`), `embedding_model`, and `embedded_at`. When sync is disabled, the app only loads the validated YAML catalog into memory.

### 3.3 Code Map

| Step | File |
| --- | --- |
| YAML loader | `crates/chat/src/knowledge/catalog/loader.rs` |
| Cross-reference validator | `crates/chat/src/knowledge/catalog/validator.rs` |
| Catalog entry → retrieval document | `crates/chat/src/knowledge/retrieval.rs` |
| Voyage embedding client | `crates/chat/src/knowledge/embedding.rs` |
| Index persistence | `crates/chat/src/knowledge/index/repository.rs` |
| End-to-end sync orchestration | `crates/chat/src/knowledge/index/sync.rs` |
| Schema | `migrations/20260621120000_create_knowledge_index.sql` |

### 3.4 Postgres Schema (Summary)

```text
knowledge_catalog_versions
  id, version, content_hash UNIQUE,
  status (loaded|validated|indexed|embedded|failed),
  document_count, embedding_model, embedding_dimensions,
  metadata_json, created_at, synced_at

knowledge_index
  id, catalog_version_id -> knowledge_catalog_versions(id) ON DELETE CASCADE,
  source_type (data_area|domain|capability|query|schema|metric|policy|response),
  source_id, source_path, title,
  retrieval_text, metadata_json, content_hash,
  embedding vector(1024) NULL,
  embedding_model, embedded_at, created_at

  unique(catalog_version_id, source_type, source_id)
  ivfflat(embedding vector_cosine_ops) WHERE embedding IS NOT NULL  -- lists=100
```

### 3.5 When Indexing Runs

- **Local/dev**: on app startup when `CATALOG_SYNC_ON_STARTUP=true`. This requires `VOYAGEAI_API_KEY`.
- **Production**: as a deploy step or admin-triggered job. Hot reload is intentionally deferred; the catalog must validate fully before becoming runtime.
- **Admin trigger (planned)**: `POST /vector-index/rebuild`, status via `GET /vector-index/status`.

## 4. Retrieval Pipeline (Runtime)

### 4.1 Position In The Request Flow

Authentication and scope checks happen **before** any retrieval, so unauthorized callers never spend embedding/LLM tokens. See `docs/ai-reporting-design.md` §3.

```text
request
  -> API key auth (core)
  -> ClientContext attached
  -> chat job created (chat::chat::service)
  -> local rule classifier fast-path
  -> if not matched and API key has allowed capabilities: embed query
  -> vector search latest catalog version capability rows filtered by allowed_capabilities
  -> choose one high-confidence capability or ask clarification from close candidates
  -> persist classification.source and classification.candidates in chat_jobs.state_json
  -> [retrieval ends here]
  -> policy guard (chat::policy::authorization)
  -> approved SQL execution
  -> response format
  -> audit
```

### 4.2 What Vector Search Returns

A ranked list of capability rows from `knowledge_index`. The runtime currently cares about:

- `source_type` and `source_id` (for example `capability:savings_deposit_total`) — used to look up the typed catalog entry.
- `metadata_json` — used as derived catalog metadata. The SQL query filters `source_type = capability` and `source_id = ANY(allowed_capabilities)` before candidates reach the classifier fallback.
- Similarity score — feeds the planner's confidence calculation.
- Only the latest `knowledge_catalog_versions` row with status `embedded` or `indexed` is searched. Duplicate capability ids are collapsed before decision-making.

The planner does **not** receive raw SQL text; it receives a selected approved capability id and resolves the query through the typed catalog.

### 4.3 Confidence And Decision

The planner combines vector similarity, lexical/example overlap, and parameter completeness into a single confidence per candidate. The decision policy from `docs/ai-reporting-design.md` §8 then routes to:

- **execute** — confidence >= `0.55`, one clear capability, complete params, and policy guard passes.
- **clarify** — confidence from `0.40` to `0.55`, close candidates within `0.05`, ambiguous capability, or missing required params (`from_date`, `to_date`, etc.).
- **unsupported** — confidence < `0.40`, no approved capability matches, or the request asks for excluded data / write / arbitrary SQL. The job is marked `failed` with `unsupported_request`; it must not remain queued.

A vector-retrieved capability is still a candidate. The policy guard in `crates/chat/src/policy/authorization.rs` is the final gate before execution.

When clarification options are returned, the user can answer with the option text, the capability id, or a 1-based option number such as `1` or `2`.

### 4.4 Where DeepSeek Fits

DeepSeek is **not** part of the retrieval store. It is a planner fallback when the local classifier's confidence is low, and a formatter for natural-language responses over the structured SQL result. It receives:

- The user message.
- The top-k retrieved capability/domain descriptors (descriptions + example phrases + parameter schemas) — never raw SQL, never raw Fineract rows beyond the approved query output contract.
- `ClientContext` capability scope so it cannot recommend a capability the caller is not allowed to run.

DeepSeek can return only one of: a `capability_id` choice with extracted parameters, a clarification question, or an `unsupported` verdict. It cannot author new SQL.

## 5. What Is Indexed, What Is Not

**Indexed in pgvector:**

- Domain knowledge (concepts and English synonyms).
- Capability descriptions and example user phrases.
- Metric definitions and aggregation semantics.
- Schema summaries (table meaning, relationships) for developer mode.
- Unsupported intent statements (so "create a savings account" can match an unsupported template fast).

**Never indexed:**

- Fineract transactional rows (clients, accounts, transactions). Vector search is for *knowledge*, not for *facts in the warehouse*.
- Raw SQL text. SQL stays on disk under `queries/` and is loaded by file path declared in query YAML.
- API keys, secrets, prompts.
- PII fields, even for documentation purposes.

## 6. Current State

### Done

- Schema for `knowledge_catalog_versions` and `knowledge_index` with deterministic content hashing, source uniqueness, GIN over metadata, and ivfflat over the embedding column.
- Catalog loader and validator covering data areas, domains, capabilities, and queries.
- Retrieval document builder that flattens catalog entries into searchable text with stable metadata.
- Voyage embedding client for document embeddings.
- Sync orchestration that persists retrieval documents, fills embeddings when startup sync is enabled, and records an `indexed` or `embedded` catalog version.
- Runtime query embedding and capability-only vector fallback in chat job creation.
- `classification.source` and `classification.candidates` persisted in `chat_jobs.state_json` for manual verification.

### Pending

- `POST /vector-index/rebuild` and `GET /vector-index/status`.
- Admin rebuild/status endpoints.
- Broader candidate context assembly across domains, metrics, schema, and policy notes.
- Query embedding/vector retrieval for clarification responses.
- DeepSeek planner fallback over retrieved candidates.

### Sequencing Rule

Voyage embedding sync can run before runtime retrieval because it only stores derived catalog vectors. Runtime vector retrieval must not execute anything directly; retrieved capabilities still need catalog validation, SQL safety validation, API-key capability scope, office scope, and PII policy checks.

## 7. Why This Design

- **Auditable.** Every job records the catalog version, retrieved capability id, query id, and confidence. A bad answer can be traced to either a YAML/SQL change or a retrieval miss — never to an opaque LLM choice.
- **Safe by construction.** Vector search cannot widen the reporting surface. New reports require new YAML + reviewed SQL, not a smarter prompt.
- **Cheap to operate.** Embeddings are computed only when a document's content hash changes. Authentication and capability scope filtering happen before any LLM/embedding call.
- **Replaceable parts.** The embedding model, the planner LLM, and even the vector store are swappable; the contract is the `knowledge_index` row shape and the capability descriptor schema.

## 8. References

- `docs/ai-reporting-design.md` — overall reporting service design and runtime flow.
- `docs/knowledge-catalog.md` — knowledge layers, YAML contracts, and validation rules.
- `docs/chat-data-model.md` — chat sessions, jobs, checkpoints, and Redis live-state rules.
- `docs/implementation-steps.md` — phase order; RAG indexing sits at Phase 10, runtime retrieval at Phase 18.
- `docs/reporting-data-scope.md` — approved Fineract areas (constrains what may be indexed).
- `docs/reporting-pii-policy.md` — what must never appear in indexed text or in responses.
