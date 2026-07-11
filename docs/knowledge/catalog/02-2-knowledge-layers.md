# Knowledge Catalog: 2. Knowledge Layers

Source: `docs-old/knowledge-catalog.md`

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
- English synonyms.
- Common user phrases.
- Supported intents.
- Explicit unsupported intents.
- Default business rules.

Example mappings:

```text
deposit = money in = savings credit from customer
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
- `savings_deposit_monthly_breakdown`
- `savings_withdrawal_monthly_top_n`

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
