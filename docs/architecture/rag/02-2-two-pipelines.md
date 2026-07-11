# RAG Architecture: 2. Two Pipelines

Source: `docs-old/rag-architecture.md`

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
    -> local classifier / LLM planner picks capability_id
    -> policy guard (capability / office / PII)
    -> bind params to approved SQL in queries/
    -> execute on Fineract read-only pool
    -> format response from query output contract
    -> audit log (capability_id, query_id, latency, decision)
```
