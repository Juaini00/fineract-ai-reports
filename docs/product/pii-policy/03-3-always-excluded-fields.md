# Reporting PII Policy: 3. Always Excluded Fields

Source: `docs-old/reporting-pii-policy.md`

## 3. Always Excluded Fields

These must never be returned to API clients or sent to AI prompts.

Credential and token fields:

- `m_appuser.password`.
- `m_appuser.temporary_password`.
- `request_audit_table.password`.
- `request_audit_table.authentication_token`.

Raw command/request fields:

- `m_portfolio_command_source.command_as_json`.
- `m_portfolio_command_source.result`.
- `m_portfolio_command_source.idempotency_key`.

Sensitive payment/reference fields unless a future capability explicitly approves masked display:

- `m_payment_detail.account_number`.
- `m_payment_detail.check_number`.
- `m_payment_detail.receipt_number`.
- `m_payment_detail.bank_number`.
- `m_payment_detail.routing_code`.

Sensitive free-text fields excluded from every currently implemented and planned capability:

- `m_savings_account.reason_for_block`.
- `m_savings_account_transaction.reason_for_block`.
