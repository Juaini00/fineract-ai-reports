# Knowledge Catalog: 6. Machine-Readable File Contracts

Source: `docs-old/knowledge-catalog.md`

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
    synonyms: [deposit, money in, credit]
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
  - What is the total deposit this month?
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
