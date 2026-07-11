# RAG Architecture: 7. Why This Design

Source: `docs-old/rag-architecture.md`

## 7. Why This Design

- **Auditable.** Every job records the catalog version, retrieved capability id, query id, and confidence. A bad answer can be traced to either a YAML/SQL change or a retrieval miss — never to an opaque LLM choice.
- **Safe by construction.** Vector search cannot widen the reporting surface. New reports require new YAML + reviewed SQL, not a smarter prompt.
- **Cheap to operate.** Embeddings are computed only when a document's content hash changes. Authentication and capability scope filtering happen before any LLM/embedding call.
- **Replaceable parts.** The embedding model, the planner LLM, and even the vector store are swappable; the contract is the `knowledge_index` row shape and the capability descriptor schema.
