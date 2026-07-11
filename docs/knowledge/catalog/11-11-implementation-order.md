# Knowledge Catalog: 11. Implementation Order

Source: `docs-old/knowledge-catalog.md`

## 11. Implementation Order

Recommended order:

1. Create `knowledge/data-scope/` from `docs/reporting-data-scope.md`.
2. Create the rest of the `knowledge/` and `queries/` folder structure.
3. Add typed Rust structs for data scope, domain, schema, metric, capability, query, policy, and response metadata.
4. Load data scope YAML before all other catalog files.
5. Load YAML from `CATALOG_PATH` and SQL from `QUERY_PATH`.
6. Validate required fields and duplicate ids.
7. Validate cross-file references and data scope boundaries.
8. Validate SQL safety for approved queries.
9. Build immutable runtime catalog.
10. Add `POST /catalog/validate` for local/admin validation.
11. Add local classifier using domain/capability examples.
12. Add policy guard integration before query execution.
13. Add optional embedding index after lexical/local matching works.
