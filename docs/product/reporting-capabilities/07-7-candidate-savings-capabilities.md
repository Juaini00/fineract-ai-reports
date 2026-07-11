# Reporting Capabilities: 7. Candidate Savings Capabilities

Source: `docs-old/reporting-capabilities.md`

## 7. Candidate Savings Capabilities

Detailed contract candidates that are not yet executable. See §11 for the full `planned` roster.

### 7.1 `savings_charge_outstanding_summary`

Runtime YAML status: `candidate`. Coverage matrix status: `planned` (target v0.2).

Purpose:

- Outstanding savings charges by office/product/charge definition.

Primary data:

- `m_savings_account_charge.amount_outstanding_derived`.
- `m_charge`.

Reason not yet enabled:

- The Savings Charges And Fees data area is currently `frontier`. Charge enum mapping (`m_charge.charge_type_enum`, `m_charge.charge_calculation_enum`) must be documented and the `amount_outstanding_derived` running-balance semantics reviewed before activation.
