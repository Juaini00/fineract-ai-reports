# Knowledge Catalog: 14. Adding A Capability

Source: `docs-old/knowledge-catalog.md`

## 14. Adding A Capability

The end-to-end workflow for adding a new capability to the runtime catalog. Follow all eight steps in order — skipping any of them leaves the classifier, planner, executor, or formatter in a state where the capability is partly reachable and produces wrong or unsafe output.

1. **Coverage matrix first.** Open [`docs/capability-coverage-matrix.md`](../../product/capability-coverage/index.md). Either add a new row (new question category) or flip an existing `planned` cell to `implemented` with the new capability id. This is the single-source-of-truth commit for scope. If the matrix cell does not exist yet, the classifier has no way to route intent here.
2. **Author the capability YAML.** Create `knowledge/capabilities/<domain>/<name>.yaml`. Required top-level fields: `id`, `status: approved_mvp`, `display_name`, `domain`, `output_mode`, `required_parameters`, `optional_parameters`, `allowed_tables`, `default_filters`, `output_fields`, `pii_behavior`, `office_authorization`, `approved_query_path`. Bilingual `examples: []` and rich `description` are retrieval-quality critical — the classifier is only as good as the semantic surface it can retrieve against.
3. **Author the approved SQL.** Create `queries/<domain>/<name>.sql`. Rules: office scope enforced via `:office_ids` bound parameter (never post-fetch); reversed transactions filtered by default (`is_reversed = false`) unless the capability explicitly analyzes reversals; date range parameterised; `LIMIT` present for `top_n` shapes; only columns listed in the capability's `output_fields` are selected.
4. **Author the query metadata YAML.** Create `knowledge/queries/<domain>/<name>.yaml` mapping the SQL file to its capability id and declaring the output column contract (name, type, nullable, sensitivity class). This is the contract the runtime `validate_runtime` (prepare + output-column check) enforces.
5. **Author retrieval surface.** Add `description`, `synonyms`, and 3+ bilingual (EN/ID) `examples` on the capability YAML. Retrieval quality is roughly proportional to example count and phrasing diversity. Include at least one paraphrase per common user intent.
6. **Validate.** `POST /catalog/validate` with the admin token. Both the referential-integrity validator (cross-file links, PII rule alignment, data-scope coverage) and the runtime validator (SQL prepares, output columns match the query metadata) must return `success: true`.
7. **Rebuild the vector index.** `POST /vector-index/rebuild`. Confirm `GET /vector-index/status` shows the expected `document_count` bump (one new document for the capability, plus deltas for any newly referenced domain/metric/schema docs).
8. **Add scenario and integration test fixture.** Author `docs/scenarios/<NN>-<name>.md` with the curl request, expected envelope shape, and side effects. Add one integration test fixture line under `crates/chat/tests/` covering the happy path — at minimum: request maps to the new capability id, SQL executes, response envelope contains the expected `output_mode` payload.

Skipping any of these leaves a known failure mode:

| Skipped step | Failure mode |
| --- | --- |
| 1 | Doc drift; user expectations misaligned with what the classifier actually does. |
| 2 | Capability id has no source of truth; policy guard rejects the plan. |
| 3 | Planner produces a capability id with no executable SQL; job fails at execution. |
| 4 | Runtime output validation fails or, worse, wrong-typed rows reach the formatter. |
| 5 | Classifier retrieval misses common phrasings and drops to `Unsupported` for valid asks. |
| 6 | Silent broken links between catalog files; caught only in production. |
| 7 | New capability is authored but never retrieved — permanent `Unsupported`. |
| 8 | Regressions ship silently on the next unrelated refactor. |

Retirement or supersession follows the same list in reverse: mark the coverage matrix row `superseded`, mark the capability YAML `status: superseded_by: <new_id>`, keep the SQL and metadata one release for backward compatibility, then remove.
