# Reporting Capabilities

This document defines the reporting capabilities that the AI Reporting Service is allowed to execute against the Fineract read-only database.

Capabilities are the runtime contract between user intent, authorization, approved SQL, and response formatting. The service must not execute arbitrary AI-generated SQL.

## 1. Capability Rules

Every reporting capability must declare:

- Capability id.
- Status.
- User intent it supports.
- Required API key scope.
- Allowed tables and joins.
- Required parameters.
- Optional parameters.
- Default filters.
- Output mode.
- Allowed output fields.
- PII behavior.
- Office authorization behavior.
- Approved query file path.

Runtime rules:

- A user request must map to one approved capability or be rejected/clarified.
- API key `allowed_capabilities` must contain the capability id.
- API key `allowed_office_ids` must be enforced on every Fineract query.
- Date ranges and limits must be validated before query execution.
- Reversed transactions must be excluded by default unless the capability explicitly analyzes reversals.
- Response output must use only declared fields.
- Raw SQL must come from approved query files, not from AI output.

## 2. Capability Statuses

| Status | Meaning |
| --- | --- |
| `approved_mvp` | Allowed for first implementation. |
| `candidate` | Documented as a likely next capability, but not executable yet. |
| `deferred` | Not executable until its data scope and business semantics are approved. |
| `rejected` | Explicitly unsupported. |

## 3. Common Parameters

These parameters may be shared across MVP savings capabilities.

| Parameter | Type | Required | Rule |
| --- | --- | --- | --- |
| `from_date` | `date` | yes | Inclusive business date lower bound. |
| `to_date` | `date` | yes | Inclusive business date upper bound. |
| `office_ids` | `array<bigint>` | no | Must be subset of API key `allowed_office_ids`. If omitted, use all allowed offices. |
| `currency_code` | `string` | no | Optional exact currency filter. |
| `product_ids` | `array<bigint>` | no | Optional savings product filter. |
| `limit` | `integer` | top/list only | Must be bounded by service max limit. |

Default validation:

- `from_date <= to_date`.
- Date range must not exceed the configured maximum range for the capability.
- `office_ids` must not broaden the caller's office scope.
- `limit` must be greater than zero and less than or equal to the configured max limit.

## 4. Common MVP Savings Joins

Approved table family:

- `m_savings_account_transaction`.
- `m_savings_account`.
- `m_savings_product`.
- `m_client`.
- `m_office`.
- `m_group`, only if group/center scope is enabled.

Default join rules:

- `m_savings_account_transaction.savings_account_id -> m_savings_account.id`.
- `m_savings_account.product_id -> m_savings_product.id`.
- Client-owned account office path: `m_savings_account.client_id -> m_client.id -> m_client.office_id`.
- Group-owned account office path: `m_savings_account.group_id -> m_group.id -> m_group.office_id`, only if group scope is enabled.
- Transaction office path: `m_savings_account_transaction.office_id -> m_office.id`.

Office authorization rule:

- Transaction `office_id` must be constrained to the caller's authorized offices.
- Account ownership office should also be validated where practical to prevent mismatched joins or data quality issues from broadening access.

## 5. Approved MVP Capabilities

### 5.1 `savings_deposit_total`

Status: `approved_mvp`.

Purpose:

- Answer total savings deposits for a date range and authorized office scope.

Supported examples:

- `What is the total deposit this month?`
- `Total deposits from January to September 2026.`
- `How much savings deposit did we receive today?`

Output mode:

- `total`.

Required API key capability:

- `savings_deposit_total`.

Required parameters:

- `from_date`.
- `to_date`.

Optional parameters:

- `office_ids`.
- `currency_code`.
- `product_ids`.

Allowed tables:

- `m_savings_account_transaction`.
- `m_savings_account`.
- `m_savings_product`.
- `m_client`.
- `m_office`.
- `m_group`, only if group/center scope is enabled.

Required transaction filters:

- `m_savings_account_transaction.transaction_type_enum = 1` for `DEPOSIT`.
- `m_savings_account_transaction.is_reversed = false`.
- `m_savings_account_transaction.transaction_date BETWEEN from_date AND to_date`.
- `m_savings_account_transaction.office_id` constrained to authorized office ids.

Allowed metrics:

- `total_deposit_amount = SUM(m_savings_account_transaction.amount)`.
- `deposit_count = COUNT(*)`.

Allowed dimensions:

- `currency_code`.
- `office_id` and office display name, if grouped or included in metadata.
- `product_id` and product name, if grouped or filtered.

Output fields:

- `from_date`.
- `to_date`.
- `currency_code`.
- `total_deposit_amount`.
- `deposit_count`.
- `office_scope` summary.

PII behavior:

- No client names, account numbers, external ids, payment references, or app user fields.
- Does not require `can_view_pii=true`.

Planned approved query file path:

- `queries/savings/deposit_total.sql`.

### 5.2 `savings_deposit_top_n`

Status: `approved_mvp`.

Purpose:

- Return the largest savings deposit transactions for a date range and authorized office scope.

Supported examples:

- `Who made the largest deposit today?`
- `Show the largest deposits this month.`
- `Top 10 savings deposits this week.`

Output mode:

- `top_n`.

Required API key capability:

- `savings_deposit_top_n`.

Required parameters:

- `from_date`.
- `to_date`.
- `limit`.

Optional parameters:

- `office_ids`.
- `currency_code`.
- `product_ids`.

Allowed tables:

- `m_savings_account_transaction`.
- `m_savings_account`.
- `m_savings_product`.
- `m_client`.
- `m_office`.
- `m_group`, only if group/center scope is enabled.

Required transaction filters:

- `m_savings_account_transaction.transaction_type_enum = 1` for `DEPOSIT`.
- `m_savings_account_transaction.is_reversed = false`.
- `m_savings_account_transaction.transaction_date BETWEEN from_date AND to_date`.
- `m_savings_account_transaction.office_id` constrained to authorized office ids.
- `ORDER BY m_savings_account_transaction.amount DESC, m_savings_account_transaction.transaction_date DESC`.
- `LIMIT limit`.

Allowed output fields without PII:

- `transaction_id`.
- `transaction_date`.
- `amount`.
- `currency_code`.
- `office_id`.
- `office_name`.
- `product_id`.
- `product_name`.

Conditionally allowed output fields with `can_view_pii=true`:

- `client_id`.
- `client_display_name`.

Still excluded even with `can_view_pii=true`:

- Savings `account_no`.
- Savings `external_id`.
- Transaction `external_id`.
- Transaction `ref_no`.
- Payment detail references.
- App user audit fields.

Planned approved query file path:

- `queries/savings/deposit_top_n.sql`.

## 6. Additional Approved Savings Capabilities

The machine-readable catalog is authoritative. These approved MVP capabilities are currently executable in addition to the two detailed deposit sections above.

| Capability | Query | Output mode | Required filters |
| --- | --- | --- | --- |
| `savings_withdrawal_total` | `queries/savings/withdrawal_total.sql` | `total` | `transaction_type_enum = 2`, `is_reversed = false`, authorized `office_id` |
| `savings_withdrawal_top_n` | `queries/savings/withdrawal_top_n.sql` | `top_n` | same withdrawal filters plus bounded `limit` |
| `savings_deposit_monthly_breakdown` | `queries/savings/deposit_monthly_breakdown.sql` | `monthly_breakdown` | deposit filters grouped by `date_trunc('month', transaction_date)` |
| `savings_deposit_monthly_top_n` | `queries/savings/deposit_monthly_top_n.sql` | `monthly_top_n` | deposit filters plus `ROW_NUMBER()` per month |
| `savings_withdrawal_monthly_breakdown` | `queries/savings/withdrawal_monthly_breakdown.sql` | `monthly_breakdown` | withdrawal filters grouped by `date_trunc('month', transaction_date)` |
| `savings_withdrawal_monthly_top_n` | `queries/savings/withdrawal_monthly_top_n.sql` | `monthly_top_n` | withdrawal filters plus `ROW_NUMBER()` per month |
| `savings_balance_summary` | `queries/savings/balance_summary.sql` | `summary` | active client-owned accounts scoped by authorized office |

PII rule: `top_n` and `monthly_top_n` return client identity fields and require `can_view_pii=true`.

## 7. Candidate Savings Capabilities

### 7.1 `savings_charge_outstanding_summary`

Status: `candidate`.

Purpose:

- Outstanding savings charges by office/product/charge definition.

Primary data:

- `m_savings_account_charge.amount_outstanding_derived`.
- `m_charge`.

Reason candidate, not MVP:

- Savings Charges And Fees is secondary scope and needs charge enum mapping before activation.

## 8. Deferred Capabilities

Deferred until their data scope and business semantics are approved:

- Loan reporting capabilities.
- Accounting/GL reporting capabilities.
- Tax reporting capabilities.
- Custom datatable reporting capabilities.
- User audit/security reporting capabilities.
- Payment reference/channel reporting capabilities.
- Hold/lien reporting capabilities.
- Overdraft reporting capabilities.
- Transfer reporting capabilities.

## 9. Unsupported Requests

The service must reject or clarify requests that ask for:

- Arbitrary SQL or database exploration.
- Full Fineract schema search.
- Report fields not declared by the selected capability.
- Raw account numbers, external ids, payment references, tokens, passwords, command JSON, or command results.
- Loan/accounting/tax/custom-datatable results before those scopes are activated.
- Office scopes outside the API key's `allowed_office_ids`.

## 10. Implementation Notes

The implementation uses static approved SQL bindings in Rust, with each executable capability mapped through catalog metadata to one reviewed query file and output contract.

Current static approved SQL bindings:

```text
savings_deposit_total -> queries/savings/deposit_total.sql
savings_deposit_top_n -> queries/savings/deposit_top_n.sql
savings_withdrawal_total -> queries/savings/withdrawal_total.sql
savings_withdrawal_top_n -> queries/savings/withdrawal_top_n.sql
savings_deposit_monthly_breakdown -> queries/savings/deposit_monthly_breakdown.sql
savings_deposit_monthly_top_n -> queries/savings/deposit_monthly_top_n.sql
savings_withdrawal_monthly_breakdown -> queries/savings/withdrawal_monthly_breakdown.sql
savings_withdrawal_monthly_top_n -> queries/savings/withdrawal_monthly_top_n.sql
savings_balance_summary -> queries/savings/balance_summary.sql
```

The classifier should only emit capability ids that exist in the registry. The policy guard should validate capability scope, parameters, office scope, PII behavior, and limits before SQL execution.
