# Runtime Documentation

Runtime docs explain how the service behaves while running.

- [Job memory](./job-memory.md)
- [API docs](../api/README.md)
- [Scenarios](../scenarios/README.md)
- [Catalog validation](../knowledge/catalog/index.md)
- [Vector retrieval](../architecture/rag/index.md)

## Offline catalog knowledge rebuild

Rebuild the assistant knowledge index from project-owned sources only:

```bash
cargo test -p chat --test assistant_catalog_index
```

The pipeline ingests `knowledge/**/*.yaml`, `queries/**/*.sql`, and selected docs. It rejects Fineract row exports/client data paths or transactional row dumps. Rebuild failure is fail-closed: keep the previous runtime index and fix the rejected source before publishing a new one.
