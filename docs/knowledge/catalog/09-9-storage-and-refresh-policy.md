# Knowledge Catalog: 9. Storage And Refresh Policy

Source: `docs-old/knowledge-catalog.md`

## 9. Storage And Refresh Policy

Current storage:

- YAML files in `knowledge/` are the source of truth for machine-readable catalog metadata.
- SQL files in `queries/` are the executable query source.
- Runtime catalog is loaded into memory.

Optional later storage:

- Catalog snapshots in PostgreSQL for auditability.
- Embeddings in pgvector for retrieval.
- Validation results in PostgreSQL for deployment checks.

Refresh policy:

- Catalog loads at startup.
- Hot reload is deferred.
- Admin-triggered refresh can be added later, but it must validate the full catalog before swapping runtime state.
