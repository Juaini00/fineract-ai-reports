# RAG Architecture: 3. Indexing Pipeline (Build Side)

Source: `docs-old/rag-architecture.md`

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
