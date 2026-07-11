# Reporting Capabilities: 5. Currently Implemented Capabilities

Source: `docs-old/reporting-capabilities.md`

## 5. Currently Implemented Capabilities

The capabilities detailed below have `status: approved_mvp` in `knowledge/capabilities/**/*.yaml` and are executable end-to-end. Each corresponds to an `implemented` cell in the coverage matrix. Additional executable capabilities are summarised in §6.

### 5.1 `savings_deposit_total`

Status: `approved_mvp`. Coverage matrix status: `implemented`.

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

Status: `approved_mvp`. Coverage matrix status: `implemented`.

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
