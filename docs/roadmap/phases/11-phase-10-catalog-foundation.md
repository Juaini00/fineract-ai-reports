# Implementation Steps: Phase 10: Catalog Foundation

Source: `docs-old/implementation-steps.md`

## Phase 10: Catalog Foundation

Goal: load and validate YAML knowledge files.

Reference design:

```text
docs/knowledge-catalog.md
```

Initial folders:

- [x] `knowledge/data-scope/`
- [x] `knowledge/domains/`
- [x] `knowledge/schema/`
- [x] `knowledge/metrics/`
- [x] `knowledge/capabilities/`
- [x] `knowledge/queries/`
- [x] `knowledge/policies/`
- [x] `knowledge/responses/`
- [x] `queries/`

Initial files:

- [x] `knowledge/data-scope/reporting-scope.yaml`
- [x] `knowledge/data-scope/areas/*.yaml`
- [x] `knowledge/domains/savings.yaml`
- [x] `knowledge/domains/client.yaml`
- [x] `knowledge/domains/organization.yaml`
- [x] `knowledge/schema/fineract/*.yaml`
- [x] `knowledge/schema/fineract/enums/*.yaml`
- [x] `knowledge/schema/fineract/joins/*.yaml`
- [x] `knowledge/schema/fineract/columns/*.yaml`
- [x] `knowledge/metrics/savings/*.yaml`
- [x] `knowledge/capabilities/savings/deposit_total.yaml`
- [x] `knowledge/capabilities/savings/deposit_top_n.yaml`
- [x] `knowledge/queries/savings/deposit_total.yaml`
- [x] `knowledge/queries/savings/deposit_top_n.yaml`
- [x] `knowledge/policies/*.yaml`
- [x] `knowledge/responses/*.yaml`
- [x] `queries/savings/deposit_total.sql`
- [x] `queries/savings/deposit_top_n.sql`

Catalog validation:

1. [x] Required YAML fields exist for loaded catalog layers.
2. [x] Capability references existing domain.
3. [x] Capability references existing query id.
4. [x] Query YAML references existing SQL file.
5. [x] Required parameters are declared.
6. [x] Output fields are declared.
7. [x] Guards are declared in query/capability YAML.
8. [x] Schema/metric/policy/response files are loaded into the runtime catalog.
9. [ ] Schema/metric/policy/response references are fully validated by typed Rust schemas.

Endpoint:

```text
POST /catalog/validate
```

Current status:

```text
PARTIALLY DONE

Project-level knowledge and query folders exist.
Initial MVP YAML/SQL files exist and are marked complete for data scope, domains, schema, metrics, capabilities, queries, policies, and responses.
Every `knowledge/**/*.yaml` file now declares explicit `checks` metadata.
Knowledge checks metadata covers capability-query contracts, office scope, PII, SQL safety, data scope, domain runtime status, metrics, responses, enums, and schema joins.
Loader and validator are implemented under crates/chat/src/knowledge/catalog.
Current loader coverage includes data areas, domains, schema, metrics, capabilities, queries, policies, and responses.
Current validator coverage includes ids/checks for every loaded layer, data area/domain refs where declared, status values, basic executable capability requirements, parameter types, output sensitivity classes, and static SQL safety checks.
Schema, metric, policy, and response layers currently use GenericKnowledge loading; field-specific typed schemas remain pending.
Retrieval document builder exists under crates/chat/src/knowledge/retrieval.rs.
Catalog/index persistence exists under crates/chat/src/knowledge/index and writes generated retrieval documents.
Voyage embedding sync exists for startup sync when CATALOG_SYNC_ON_STARTUP=true and VOYAGEAI_API_KEY is configured.
POST /catalog/validate is implemented and authenticated.

Still pending for this phase:
reject unknown YAML fields after schemas stabilize
validate guards and policy references more completely
runtime vector retrieval fallback exists for chat job creation
```
