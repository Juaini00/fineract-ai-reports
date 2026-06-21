# Knowledge Catalog

This document defines the knowledge system for the AI Reporting Service.

Knowledge is controlled application context. It helps the service understand user requests, map them to approved reporting capabilities, execute approved SQL, and format safe responses. It must also keep the application aligned with the approved reporting data scope.

Knowledge is not a free-form prompt dump. It is structured application data that Rust loads, validates, and enforces.

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

## 2. Knowledge Layers

The service uses seven knowledge layers.

```text
Data Scope Knowledge
  -> Schema Knowledge
  -> Domain Knowledge
  -> Metric Knowledge
  -> Capability Knowledge
  -> Query Knowledge
  -> Response Knowledge
```

### 2.1 Data Scope Knowledge

Data scope knowledge defines which Fineract data areas may become runtime knowledge.

Human-readable source:

- `docs/reporting-data-scope.md`

Machine-readable source:

- `knowledge/data-scope/reporting-scope.yaml`
- `knowledge/data-scope/areas/*.yaml`

It contains:

- Included MVP data areas.
- Conditional data areas.
- Secondary data areas.
- Deferred data areas.
- Explicitly out-of-scope data areas.
- Included table families.
- Excluded table families.
- Detail document path for each area.
- Allowed domains for each area.

Runtime rule:

- No domain, capability, query, schema, metric, join, enum, or response field may use a Fineract table or column outside approved or explicitly enabled conditional data scope.

### 2.2 Schema Knowledge

Schema knowledge summarizes selected Fineract tables, columns, relationships, enums, and sensitivity classes.

It contains:

- Table name.
- Data area id.
- Business meaning.
- Important columns.
- Column sensitivity class.
- Relationships and join paths.
- Enum/value mappings.
- Default filters.
- Known data-quality caveats.
- Scope status: included, conditional, secondary, deferred, or rejected.

Runtime rule:

- Schema knowledge does not grant permission to query a table.
- A table can be documented but still unavailable at runtime unless an approved capability and approved query use it.

### 2.3 Domain Knowledge

Domain knowledge describes a business area in user/business language.

Examples:

- `organization`
- `client`
- `savings`
- `loan`, later
- `accounting`, later

It contains:

- Domain id.
- Display name.
- Business description.
- Supported data area ids.
- Core business concepts.
- Synonyms in English and Indonesian.
- Common user phrases.
- Supported intents.
- Explicit unsupported intents.
- Default business rules.

Example mappings:

```text
deposit = setoran = money in = savings credit from customer
withdrawal = penarikan = money out
interest = bunga = automatic balance increase from interest posting
office = branch = cabang = organizational reporting scope
```

Runtime use:

- Candidate domain matching.
- Unsupported intent detection.
- Clarification wording.
- AI prompt context when local classification is not enough.

### 2.4 Metric Knowledge

Metric knowledge defines reusable business calculations.

Examples:

- `savings.deposit_amount`
- `savings.deposit_count`
- `savings.withdrawal_amount`
- `savings.account_balance`

It contains:

- Metric id.
- Domain id.
- Data area ids.
- Source tables and columns.
- Required filters.
- Aggregation expression.
- Grouping compatibility.
- Sensitivity class.
- Default reversal/status handling.

Runtime rule:

- A metric may only reference tables and columns allowed by data scope knowledge.
- A capability may only expose metrics declared in metric knowledge or explicitly declared in the capability output contract.

### 2.5 Capability Knowledge

Capability knowledge defines what the system is allowed to execute.

Examples:

- `savings_deposit_total`
- `savings_deposit_top_n`
- `savings_deposit_monthly_breakdown`, later

It contains:

- Capability id.
- Status: `approved_mvp`, `candidate`, `deferred`, or `rejected`.
- Domain id.
- Data area ids.
- User intents and example phrases.
- Required API key capability.
- Required parameters.
- Optional parameters.
- Default filters.
- Output mode.
- PII behavior.
- Office authorization behavior.
- Query id.
- Metric ids.
- Cost class.
- Clarification rules.
- Unsupported variants.

Runtime rule:

- If a user request does not map to an approved capability, the system must reject it or ask for clarification.
- The service must not create a new runtime SQL query from AI output.

### 2.6 Query Knowledge

Query knowledge connects an approved capability to an approved SQL file.

It contains:

- Query id.
- SQL file path.
- Database target: `fineract` or `app`.
- Data area ids.
- Source tables.
- Required joins.
- Required parameters.
- Optional parameters.
- Parameter types and validation rules.
- Allowed output fields.
- Output contract.
- Required filters.
- Guard rules.
- Timeout.
- Cost class.

Runtime rule:

- Query SQL must come from a reviewed file under `queries/`.
- Query metadata must match the SQL file.
- Query execution must use typed/bound parameters, never string interpolation.

### 2.7 Response Knowledge

Response knowledge defines how results should become user-facing answers.

It contains:

- Response templates.
- Supported languages.
- Field labels.
- Currency/date formatting rules.
- Empty result behavior.
- Clarification message templates.
- Unsupported message templates.
- PII-safe formatting rules.

Runtime rule:

- Response formatting must use the selected capability output contract.
- The formatter must not expose fields that were not declared by the capability.

## 3. Initial Knowledge Needed For MVP

The MVP should not model the whole Fineract database. It should only include the minimum knowledge needed for approved organization, client, and savings reporting.

### 3.1 Data Scope Files

Initial files:

```text
knowledge/data-scope/reporting-scope.yaml
knowledge/data-scope/areas/organization-foundation.yaml
knowledge/data-scope/areas/client-foundation.yaml
knowledge/data-scope/areas/group-center-foundation.yaml
knowledge/data-scope/areas/savings-core.yaml
knowledge/data-scope/areas/savings-transactions.yaml
knowledge/data-scope/areas/savings-charges-fees.yaml
knowledge/data-scope/areas/deferred.yaml
knowledge/data-scope/areas/out-of-scope.yaml
```

These files mirror:

- `docs/reporting-data-scope.md`
- `docs/reporting-data/organization-foundation.md`
- `docs/reporting-data/client-foundation.md`
- `docs/reporting-data/group-center-foundation.md`
- `docs/reporting-data/savings-core.md`
- `docs/reporting-data/savings-transactions.md`
- `docs/reporting-data/savings-charges-fees.md`

Required MVP area statuses:

| Area | Machine-readable id | Status |
| --- | --- | --- |
| Organization Foundation | `organization_foundation` | `included_mvp_foundation` |
| Client Foundation | `client_foundation` | `included_mvp_foundation` |
| Group And Center Foundation | `group_center_foundation` | `conditional` |
| Savings Core | `savings_core` | `included_mvp_domain` |
| Savings Transactions | `savings_transactions` | `included_mvp_domain` |
| Savings Charges And Fees | `savings_charges_fees` | `secondary` |

Deferred areas must be represented explicitly:

- `loans`
- `accounting_gl`
- `tax`
- `custom_datatables`
- `audit_users_operations`

Out-of-scope areas must be represented explicitly:

- Arbitrary SQL exploration.
- Full Fineract schema search.
- Document/image/file reporting.
- Identity document reporting.
- Address-level reporting.
- Write/update/delete actions against Fineract.

### 3.2 Domain Files

Initial files:

```text
knowledge/domains/organization.yaml
knowledge/domains/client.yaml
knowledge/domains/savings.yaml
```

Purpose:

- `organization`: office hierarchy and office-scoped reporting.
- `client`: client ownership and PII-aware identity context.
- `savings`: savings accounts and savings transactions.

### 3.3 Schema Files

Initial files:

```text
knowledge/schema/fineract/organization.yaml
knowledge/schema/fineract/client.yaml
knowledge/schema/fineract/savings.yaml
knowledge/schema/fineract/enums/savings_transaction_type.yaml
knowledge/schema/fineract/enums/savings_account_status.yaml
knowledge/schema/fineract/enums/client_status.yaml
knowledge/schema/fineract/joins/office_scope.yaml
knowledge/schema/fineract/joins/client_savings_account.yaml
knowledge/schema/fineract/joins/group_savings_account.yaml
knowledge/schema/fineract/joins/savings_transaction_account.yaml
knowledge/schema/fineract/columns/sensitivity.yaml
knowledge/schema/fineract/columns/excluded.yaml
```

Initial table families:

- `m_office`
- `m_staff`, only basic office/staff context if needed
- `m_client`
- `m_group`, conditional
- `m_savings_account`
- `m_savings_product`
- `m_savings_account_transaction`
- `m_charge`, later for savings charge reporting

### 3.4 Metric Files

Initial files:

```text
knowledge/metrics/savings/deposit_amount.yaml
knowledge/metrics/savings/deposit_count.yaml
knowledge/metrics/savings/withdrawal_amount.yaml
knowledge/metrics/savings/account_balance.yaml
```

### 3.5 Capability Files

Initial files:

```text
knowledge/capabilities/savings/deposit_total.yaml
knowledge/capabilities/savings/deposit_top_n.yaml
```

Initial approved capabilities:

- `savings_deposit_total`
- `savings_deposit_top_n`

Next likely capabilities:

- `savings_deposit_monthly_breakdown`
- `savings_withdrawal_total`
- `savings_balance_summary`

### 3.6 Query Files

Initial query metadata:

```text
knowledge/queries/savings/deposit_total.yaml
knowledge/queries/savings/deposit_top_n.yaml
```

Initial SQL files:

```text
queries/savings/deposit_total.sql
queries/savings/deposit_top_n.sql
```

### 3.7 Policy Files

Initial files:

```text
knowledge/policies/pii.yaml
knowledge/policies/query_safety.yaml
knowledge/policies/office_scope.yaml
knowledge/policies/execution_limits.yaml
knowledge/policies/unsupported_requests.yaml
```

These files may initially mirror existing docs:

- `docs/reporting-pii-policy.md`
- `docs/reporting-capabilities.md`
- `docs/reporting-data-scope.md`

### 3.8 Response Files

Initial files:

```text
knowledge/responses/reporting.yaml
knowledge/responses/clarification.yaml
knowledge/responses/unsupported.yaml
```

## 4. Recommended Directory Structure

```text
knowledge/
  data-scope/
    reporting-scope.yaml
    areas/
      organization-foundation.yaml
      client-foundation.yaml
      group-center-foundation.yaml
      savings-core.yaml
      savings-transactions.yaml
      savings-charges-fees.yaml
      deferred.yaml
      out-of-scope.yaml
  domains/
    organization.yaml
    client.yaml
    savings.yaml
  schema/
    fineract/
      organization.yaml
      client.yaml
      savings.yaml
      enums/
        savings_transaction_type.yaml
        savings_account_status.yaml
        client_status.yaml
      joins/
        office_scope.yaml
        client_savings_account.yaml
        group_savings_account.yaml
        savings_transaction_account.yaml
      columns/
        sensitivity.yaml
        excluded.yaml
  metrics/
    savings/
      deposit_amount.yaml
      deposit_count.yaml
      withdrawal_amount.yaml
      account_balance.yaml
  capabilities/
    savings/
      deposit_total.yaml
      deposit_top_n.yaml
  queries/
    savings/
      deposit_total.yaml
      deposit_top_n.yaml
  policies/
    pii.yaml
    query_safety.yaml
    office_scope.yaml
    execution_limits.yaml
    unsupported_requests.yaml
  responses/
    reporting.yaml
    clarification.yaml
    unsupported.yaml

queries/
  savings/
    deposit_total.sql
    deposit_top_n.sql
```

Directory rules:

- `knowledge/data-scope/` mirrors `docs/reporting-data-scope.md`.
- `knowledge/domains/` describes business language and user intent.
- `knowledge/schema/` describes allowed tables, columns, enums, joins, and sensitivity.
- `knowledge/metrics/` describes reusable business calculations.
- `knowledge/capabilities/` declares executable reporting abilities.
- `knowledge/queries/` maps capabilities to approved SQL files.
- `knowledge/policies/` defines cross-cutting enforcement rules.
- `knowledge/responses/` defines safe output and clarification templates.
- `queries/` contains reviewed SQL only.

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

```text
loaded and validated: data areas, domains, capabilities, queries
files present but not fully loaded/validated yet: schema, metrics, policies, responses
```

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

- `approved_mvp` capabilities may use only `included_mvp_foundation`, `included_mvp_domain`, or explicitly enabled `conditional` areas.
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

Search index is optional for MVP.

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
retrieval documents can be persisted to knowledge_index with embedding NULL
knowledge_catalog_versions records the generated catalog version
```

Still pending:

```text
embedding generation
Voyage API client
vector rebuild/status endpoint
runtime vector retrieval
```

Sequencing rule:

```text
Index persistence may exist before embeddings, but vector retrieval must not drive authorization or execution decisions.
```

### 5.9 Step 9: Use At Runtime

Runtime request flow:

```text
User request
  -> API key authentication
  -> local normalization
  -> data scope candidate check
  -> domain candidate retrieval
  -> capability candidate retrieval
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

## 6. Machine-Readable File Contracts

The exact YAML schemas will be implemented later in Rust. These examples define the intended shape.

### 6.1 Data Scope YAML Example

```yaml
id: savings_transactions
status: included_mvp_domain
source_doc: docs/reporting-data-scope.md#35-savings-transactions
detail_doc: docs/reporting-data/savings-transactions.md
business_purpose: Support transaction-level reporting for savings movement.
included_tables:
  - m_savings_account_transaction
conditional_tables:
  - m_payment_detail
excluded_tables:
  - m_savings_account_transaction_tax_details
default_rules:
  - Reversed transactions are excluded by default.
  - Transaction type enum mapping must be declared before use.
allowed_domains:
  - savings
allowed_metrics:
  - savings.deposit_amount
  - savings.deposit_count
sensitivity_notes:
  - Payment references are sensitive and excluded from MVP output.
```

### 6.2 Domain YAML Example

```yaml
id: savings
status: approved_mvp
display_name: Savings
description: Savings account and transaction reporting.
data_areas:
  - savings_core
  - savings_transactions
concepts:
  - id: deposit
    meaning: Money credited into a savings account.
    synonyms: [deposit, setoran, money in, credit]
  - id: withdrawal
    meaning: Money debited from a savings account.
    synonyms: [withdrawal, penarikan, money out, debit]
supported_intents:
  - savings deposit totals
  - largest savings deposits
unsupported_intents:
  - create savings account
  - reverse transaction
default_rules:
  - Exclude reversed transactions unless explicitly requested by an approved capability.
```

### 6.3 Capability YAML Example

```yaml
id: savings_deposit_total
status: approved_mvp
domain: savings
data_areas:
  - organization_foundation
  - client_foundation
  - savings_core
  - savings_transactions
required_api_capability: savings_deposit_total
query_id: savings.deposit_total
metrics:
  - savings.deposit_amount
  - savings.deposit_count
output_mode: total
examples:
  - Total deposit bulan ini berapa?
  - How much savings deposit did we receive today?
required_parameters:
  - from_date
  - to_date
optional_parameters:
  - office_ids
  - currency_code
  - product_ids
defaults:
  exclude_reversed: true
guards:
  require_office_scope: true
  max_date_range_days: 366
pii:
  returns_pii: false
cost_class: low
clarification:
  missing_parameters:
    - from_date
    - to_date
```

### 6.4 Query YAML Example

```yaml
id: savings.deposit_total
database: fineract
sql_file: queries/savings/deposit_total.sql
data_areas:
  - organization_foundation
  - client_foundation
  - savings_core
  - savings_transactions
tables:
  - m_savings_account_transaction
  - m_savings_account
  - m_client
  - m_office
metrics:
  - savings.deposit_amount
  - savings.deposit_count
parameters:
  - name: from_date
    type: date
    required: true
  - name: to_date
    type: date
    required: true
  - name: office_ids
    type: array_bigint
    required: true
    source: authorized_scope
output_fields:
  - name: from_date
    type: date
    sensitivity: public_business
  - name: to_date
    type: date
    sensitivity: public_business
  - name: total_deposit_amount
    type: decimal
    sensitivity: public_business
  - name: deposit_count
    type: integer
    sensitivity: public_business
guards:
  select_only: true
  single_statement: true
  require_office_filter: true
timeout_ms: 3000
cost_class: low
```

### 6.5 Schema YAML Example

```yaml
id: fineract.savings
status: approved_mvp
data_areas:
  - savings_core
  - savings_transactions
tables:
  - name: m_savings_account_transaction
    data_area: savings_transactions
    meaning: Savings account transaction records.
    default_filters:
      - is_reversed = false
    columns:
      - name: id
        meaning: Transaction id.
        sensitivity: public_business
      - name: amount
        meaning: Transaction amount.
        sensitivity: public_business
      - name: external_id
        meaning: External transaction reference.
        sensitivity: sensitive_business_identifier
    relationships:
      - from: m_savings_account_transaction.savings_account_id
        to: m_savings_account.id
```

## 7. Validation Rules

Catalog validation should fail fast when any critical rule is violated.

Critical validation failures:

- Duplicate ids.
- Data scope file missing for an area listed in `docs/reporting-data-scope.md`.
- Machine-readable data scope disagrees with human-readable reporting data scope.
- Unknown data area reference.
- Unknown domain reference.
- Unknown query reference.
- Missing SQL file.
- Query metadata points to an unsafe SQL file.
- Capability marked `approved_mvp` without query metadata.
- Fineract query without office authorization behavior.
- Capability, query, schema, metric, or response references a deferred or out-of-scope data area.
- Capability, query, schema, or metric references a table not listed in approved data scope.
- Query references a column classified as excluded or `secret_never_expose`.
- Output field missing sensitivity classification.
- PII field returned without explicit capability approval.
- Query output contract includes a field marked `secret_never_expose`.

Warnings:

- Candidate capability has no query yet.
- Domain has no approved capability.
- Synonym appears in multiple domains with ambiguous meaning.
- Example phrase maps to multiple capabilities with similar score.
- Schema table is documented but unused by approved capabilities.
- Data area is included but has no schema knowledge yet.
- Data area is included but has no capability yet.

## 8. Runtime Decision Rules

The catalog supports three runtime outcomes.

### 8.1 Execute

Execute only when:

- Domain confidence is high enough.
- Capability confidence is high enough.
- Required parameters are complete.
- API key allows the capability.
- Capability data areas are approved.
- Office scope is valid.
- PII policy allows the output contract.
- Query safety validation has passed.

### 8.2 Clarify

Ask clarification when:

- Domain is likely but capability is ambiguous.
- Required date range is missing.
- User asks for `top` or `largest` without a limit and no default applies.
- Office/product/currency filter is ambiguous.
- The request combines multiple capabilities and MVP only supports atomic execution.

### 8.3 Unsupported

Reject safely when:

- The request maps to no approved capability.
- The request asks to modify data.
- The request asks for arbitrary SQL.
- The request asks for excluded PII or secrets.
- The request asks for a deferred data domain.
- The request requires tables outside approved data scope.

## 9. Storage And Refresh Policy

MVP storage:

- YAML files in `knowledge/` are the source of truth for machine-readable catalog metadata.
- SQL files in `queries/` are the executable query source.
- Runtime catalog is loaded into memory.

Optional later storage:

- Catalog snapshots in PostgreSQL for auditability.
- Embeddings in pgvector for retrieval.
- Validation results in PostgreSQL for deployment checks.

Refresh policy:

- MVP loads catalog at startup.
- Hot reload is deferred.
- Admin-triggered refresh can be added later, but it must validate the full catalog before swapping runtime state.

## 10. Audit Requirements

Every report job should record enough knowledge metadata to explain the decision.

Audit fields:

- Request id or job id.
- API key id.
- Catalog version or checksum.
- Selected data area ids.
- Selected domain id.
- Selected capability id.
- Selected query id.
- Selected metric ids.
- Confidence score.
- Required parameters and sanitized values.
- Office scope applied.
- PII mode applied.
- Decision outcome: execute, clarify, unsupported, forbidden, or failed.
- Query latency and row count.

Do not audit raw API keys, raw secrets, unsafe prompt details, or large raw result payloads.

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

## 12. Non-Goals For MVP

Do not implement these in the first catalog version:

- Arbitrary SQL generation.
- Full Fineract schema ingestion.
- Hot reload without full validation.
- User-editable knowledge through public APIs.
- Vector-only decision making.
- Automatic approval of new capabilities.
- Runtime table discovery against production Fineract.
- Knowledge that silently expands beyond `docs/reporting-data-scope.md`.

## 13. References

- `docs/reporting-data-scope.md`
- `docs/reporting-capabilities.md`
- `docs/reporting-pii-policy.md`
- `docs/ai-reporting-design.md`
- `docs/chat-data-model.md`
