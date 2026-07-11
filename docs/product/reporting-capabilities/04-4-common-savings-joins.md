# Reporting Capabilities: 4. Common Savings Joins

Source: `docs-old/reporting-capabilities.md`

## 4. Common Savings Joins

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
