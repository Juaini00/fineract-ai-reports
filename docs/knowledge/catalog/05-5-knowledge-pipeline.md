# Knowledge Catalog: 5. Knowledge Pipeline

Source: `docs-old/knowledge-catalog.md`

## 5. Knowledge Pipeline

The knowledge pipeline has nine steps.

```text
1. Author
2. Load Data Scope
3. Load Catalog Files
4. Validate Structure
5. Validate Links And Scope
6. Validate SQL Safety
7. Build Runtime Catalog
8. Optionally Build Search Index
9. Use At Runtime
```

### 5.1 Step 1: Author

Developers write or update documentation, YAML metadata, and SQL files.

Rules:

- New business support starts in `docs/reporting-data-scope.md` or detailed reporting data docs.
- Machine-readable data scope must be updated before schema, metrics, capabilities, or queries use a new area.
- A capability is not executable until its query metadata and SQL file exist.
- Every output field must declare sensitivity behavior.
- Every Fineract query must declare office-scope enforcement.

### 5.2 Step 2: Load Data Scope

The loader first loads data scope files.

Inputs:

- `knowledge/data-scope/reporting-scope.yaml`
- `knowledge/data-scope/areas/*.yaml`
- `docs/reporting-data-scope.md` as human-readable source

Rules:

- Data scope must load before all other catalog files.
- Deferred and out-of-scope areas must be loaded, not ignored.
- Every machine-readable data area must point to its detail document.
- If data scope files and `docs/reporting-data-scope.md` disagree, catalog validation fails.

### 5.3 Step 3: Load Catalog Files

The service loads catalog files from configured paths.

Config:

```env
CATALOG_PATH=knowledge
QUERY_PATH=queries
CATALOG_VALIDATE_ON_STARTUP=true
CATALOG_SYNC_ON_STARTUP=false
```

Rules:

- Local/dev can validate catalog during app startup.
- Production should validate catalog during deployment and may also validate at startup.
- Invalid catalog must fail fast before serving report endpoints.

Current implementation:

```text
crates/chat/src/knowledge/catalog/loader.rs
crates/chat/src/knowledge/catalog/validator.rs
```

Current coverage:

YAML present and content-complete for MVP:

- [x] data-scope
- [x] domains
- [x] schema
- [x] metrics
- [x] capabilities
- [x] queries
- [x] policies
- [x] responses

Checks metadata present:

- [x] every `knowledge/**/*.yaml` file declares a top-level `checks` block.
- [x] capability/query checks cover query existence, required parameters, output-field contract, office scope, SQL safety, and PII behavior.
- [x] data-scope/schema/metric/policy/response checks declare the intended validation rules for future typed validators.

Loaded by typed catalog loader:

- [x] data areas
- [x] domains
- [x] capabilities
- [x] queries
- [x] schema
- [x] metrics
- [x] policies
- [x] responses

Validated by typed catalog validator:

- [x] data area ids/statuses
- [x] domain refs to data areas
- [x] capability refs to domains, queries, and data areas
- [x] query refs to data areas
- [x] query SQL existence and static safety
- [x] query output sensitivity classes
- [x] schema/metric/policy/response ids and checks presence
- [x] schema/metric refs to known domains and data areas where declared
- [ ] schema table/column refs
- [ ] metric refs to schema tables/columns
- [ ] policy refs to capabilities/query guards
- [ ] response refs to capability output contracts

### 5.4 Step 4: Validate Structure

The loader validates each YAML file against typed Rust schemas.

Checks:

- Required fields exist.
- IDs are valid and stable.
- Status values are valid.
- Parameter types are valid.
- Data area statuses are valid.
- Output fields are declared.
- Sensitivity classes are valid.
- Unknown fields are rejected unless explicitly allowed.

### 5.5 Step 5: Validate Links And Scope

The catalog validator checks references across files and against data scope.

Checks:

- Domain references existing data areas.
- Schema references existing data areas.
- Metric references existing domain, data areas, tables, and columns.
- Capability references existing domain, data areas, metrics, and query id.
- Query metadata references existing SQL file.
- Query metadata references only approved tables, columns, joins, metrics, and enums.
- Output fields used by response templates exist in query metadata.
- Policy ids referenced by capabilities exist.
- Deferred areas do not appear in approved capabilities, queries, metrics, or response fields.
- Out-of-scope areas produce hard validation failures if referenced by runtime catalog files.

Scope status rules:

- Enabled (runtime `approved_mvp`) capabilities may use only `included_mvp_foundation`, `included_mvp_domain`, or explicitly enabled `conditional` areas.
- `secondary` areas may be documented but must not be executable until a capability explicitly approves them.
- Deferred areas may be documented but are not runtime-available.
- Out-of-scope areas are never runtime-available.

### 5.6 Step 6: Validate SQL Safety

The SQL validator checks executable SQL before runtime use.

Checks:

- SQL file exists.
- SQL is SELECT-only.
- SQL is single-statement.
- SQL does not contain unsafe commands.
- Placeholder names or positions match query metadata.
- Required office filters are present for Fineract queries.
- Date range and limit parameters are represented where required.
- Referenced tables and columns are allowed by data scope knowledge.
- Referenced joins are declared in schema join knowledge or query metadata.
- Referenced metrics match metric knowledge when the query implements a named metric.
- `EXPLAIN` succeeds with sample parameters when database validation is available.

Unsafe commands include:

```text
INSERT
UPDATE
DELETE
TRUNCATE
DROP
ALTER
CREATE
GRANT
REVOKE
COPY
VACUUM
ANALYZE
```

Current implementation:

```text
crates/chat/src/knowledge/catalog/validator.rs
```

Current static coverage:

```text
SQL file existence
SELECT-only start check
single-statement check
blocked unsafe command token check
placeholder count/order check against query metadata
basic placeholder cast check against declared parameter types
required office/date/limit clause presence checks
```

Still pending:

```text
EXPLAIN validation against Fineract/app database
declared output field validation against real query columns
table and column validation against loaded schema knowledge
```

### 5.7 Step 7: Build Runtime Catalog

After validation, the service builds an immutable in-memory runtime catalog.

Runtime catalog should include:

- Data areas by id.
- Table-to-data-area index.
- Column sensitivity index.
- Domains by id.
- Capabilities by id.
- Capabilities by domain.
- Query metadata by id.
- SQL text by query id or SQL file path.
- Synonym indexes.
- Metric definitions by id.
- Join definitions by id.
- Enum mappings by id.
- Capability examples for local classification.
- Policy lookup tables.

Refresh can be added later as an explicit admin operation. Hot reload is deferred.

### 5.8 Step 8: Optionally Build Search Index

Search index is optional in principle but wired today via pgvector.

Potential indexes:

- Lexical index over synonyms and examples.
- Embedding index in pgvector for domain/capability retrieval.
- Schema/documentation search index for developer mode.

Rules:

- Vector search only finds relevant knowledge candidates.
- Vector search must not decide authorization or execute queries.
- A retrieved capability still needs Rust validation and policy checks.
- Search results must carry source ids such as data area id, domain id, capability id, query id, schema id, or metric id.

Current implementation:

```text
migrations/20260621120000_create_knowledge_index.sql
crates/chat/src/knowledge/retrieval.rs
crates/chat/src/knowledge/index/repository.rs
crates/chat/src/knowledge/index/sync.rs
```

Current behavior:

```text
validated catalog data is converted into retrieval documents
catalog and document content hashes are deterministic
retrieval documents can be persisted to knowledge_index
when CATALOG_SYNC_ON_STARTUP=true, Voyage embeddings are stored in knowledge_index.embedding
knowledge_catalog_versions records indexed or embedded status
runtime vector fallback searches the latest indexed/embedded catalog version only
runtime vector fallback searches capability/query rows and maps query rows back to approved capabilities
runtime context search also retrieves non-executable rows such as data_area, domain, schema, metric, policy, and response for audit/planner context
```

Still pending:

```text
LLM planner fallback consumption of broader context rows
```

Sequencing rule:

```text
Vector retrieval ranks knowledge candidates only. Query candidates must map back to approved capability ids, and SQL execution still requires catalog validation, API-key capability scope, policy checks, and static approved SQL bindings.
```

### 5.9 Step 9: Use At Runtime

Runtime request flow:

```text
User request
  -> API key authentication
  -> deterministic write-intent guard
  -> query embedding when caller has allowed capabilities
  -> latest catalog version capability candidate retrieval
  -> catalog lexical fallback when embedding/vector search is unavailable
  -> parameter extraction
  -> confidence scoring
  -> clarification / unsupported / execution plan
  -> policy guard
  -> approved SQL execution
  -> response formatting
  -> audit event
```

Rules:

- Authentication happens before knowledge retrieval.
- Knowledge retrieval does not bypass API key capability scope.
- The selected capability must be in `allowed_capabilities`.
- The selected capability must use only approved data areas.
- Office filters must be constrained by `allowed_office_ids`.
- PII output must follow the selected capability and API key context.
