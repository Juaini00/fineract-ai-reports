# Capability Coverage Matrix: Adding a row or column

Source: `docs-old/capability-coverage-matrix.md`

## Adding a row or column

Adding a capability is a documentation-and-catalog change, not just code. Follow the three-phase guide in [`docs/knowledge-catalog.md` §14 Adding A Capability](../../knowledge/catalog/index.md#14-adding-a-capability):

- **Phase A — Design:** flip a `planned` cell or add a new row here; write down expected inputs, expected outputs, PII contract.
- **Phase B — Implement:** author the capability YAML, SQL, query metadata, retrieval enrichment; run `POST /catalog/validate`; run `POST /vector-index/rebuild`.
- **Phase C — Verify:** contract-test fixture entry, scenario doc, integration test.

Adding a **column** (a new query shape) requires the same three phases applied to each row that gains an implementation, plus a new `output_mode` entry in `docs/ai-reporting-design.md` §6 if the shape needs one.
