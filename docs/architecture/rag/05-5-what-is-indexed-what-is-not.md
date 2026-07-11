# RAG Architecture: 5. What Is Indexed, What Is Not

Source: `docs-old/rag-architecture.md`

## 5. What Is Indexed, What Is Not

**Indexed in pgvector:**

- Domain knowledge (concepts and English synonyms).
- Capability descriptions and example user phrases.
- Query metadata that maps back to approved capabilities.
- Metric definitions and aggregation semantics.
- Schema summaries (table meaning, relationships) for developer mode.
- Unsupported intent statements (so "create a savings account" can match an unsupported template fast).

**Never indexed:**

- Fineract transactional rows (clients, accounts, transactions). Vector search is for *knowledge*, not for *facts in the warehouse*.
- Raw SQL text. SQL stays on disk under `queries/` and is loaded by file path declared in query YAML.
- API keys, secrets, prompts.
- PII fields, even for documentation purposes.
