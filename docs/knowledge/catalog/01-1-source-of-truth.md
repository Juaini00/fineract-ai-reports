# Knowledge Catalog: 1. Source Of Truth

Source: `docs-old/knowledge-catalog.md`

## 1. Source Of Truth

The knowledge catalog is downstream from the reporting data scope.

Source-of-truth order:

```text
docs/reporting-data-scope.md
  -> knowledge/data-scope/
  -> knowledge/schema/
  -> knowledge/domains/
  -> knowledge/metrics/
  -> knowledge/capabilities/
  -> knowledge/queries/ + queries/
  -> knowledge/responses/
```

Rules:

- `docs/reporting-data-scope.md` defines the human-approved reporting surface.
- `knowledge/data-scope/` is the machine-readable mirror of that approved surface.
- `knowledge/schema/` must not describe runtime-available tables outside `knowledge/data-scope/`.
- `knowledge/capabilities/` must not approve reports outside the data scope.
- `knowledge/queries/` and `queries/` must not access tables, columns, joins, or metrics outside approved capabilities.
- If documentation and machine-readable catalog disagree, catalog validation must fail until they are reconciled.
