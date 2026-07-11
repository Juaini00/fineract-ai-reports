# Reporting PII Policy: 5. Sensitive Business Identifiers

Source: `docs-old/reporting-pii-policy.md`

## 5. Sensitive Business Identifiers

These are not always personal data, but they can identify customers, accounts, transactions, or internal records. Exclude from default output.

Examples:

- `m_client.account_no`.
- `m_client.external_id`.
- `m_savings_account.account_no`.
- `m_savings_account.external_id`.
- `m_savings_account.iban`.
- `m_savings_account_transaction.external_id`.
- `m_savings_account_transaction.ref_no`.
- `m_group.account_no`.
- `m_group.external_id`.
- `m_office.external_id`.
- `m_staff.external_id`.
- Loan `account_no` and `external_id`, when loan scope is later approved.

Default rule:

- Do not return these fields in any currently implemented or planned capability without explicit sensitivity re-classification.
- Use internal numeric ids only where required for traceability and only if the capability declares them.
