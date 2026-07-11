# Reporting Data Scope: 4. Deferred Data Areas

Source: `docs-old/reporting-data-scope.md`

## 4. Deferred Data Areas

### 4.1 Loans — status: `deferred`

Status: deferred.

Reason:

- Loan reporting is important but more complex than savings.
- It introduces repayment schedules, disbursements, arrears, delinquency, write-off, overpayment, charge-off, and product-specific rules.

Future high-level data concepts:

- Loan account.
- Loan product.
- Disbursement.
- Repayment.
- Outstanding principal.
- Interest and fees.
- Arrears and overdue.
- Loan status and lifecycle.

Scope rule:

- Do not include loan tables in any executable capability until the loan domain is activated (see coverage matrix Category G).

Verified Fineract table family:

- `m_loan`.
- `m_product_loan`.
- `m_loan_transaction`.
- `m_loan_repayment_schedule`.
- `m_loan_charge`.

Detail file:

- `docs/reporting-data/loans.md`.

### 4.2 Accounting And General Ledger — status: `deferred`

Status: deferred.

Reason:

- Accounting reports are financially sensitive.
- GL reports need stricter reconciliation rules and more explicit business definitions.

Future high-level data concepts:

- Journal entries.
- GL accounts.
- Product-to-GL mappings.
- Trial balance.
- Asset/liability/income/expense movements.

Scope rule:

- Do not include accounting tables in any executable capability until the accounting domain is activated.
- Add only after approved accounting definitions are documented.

Verified Fineract table family:

- `acc_gl_account`.
- `acc_gl_journal_entry`.
- `acc_accounting_rule`.
- `acc_product_mapping`.
- `acc_gl_closure`.

Detail file:

- `docs/reporting-data/accounting-gl.md`.

### 4.3 Tax — status: `deferred`

Status: deferred.

Reason:

- Tax calculations and tax withholding need careful business validation.
- Tax details should not be inferred casually from base transaction amounts.

Future high-level data concepts:

- Tax group.
- Tax component.
- Savings transaction tax details.
- Loan transaction tax details.

Scope rule:

- Exclude tax-specific reporting until exact tax semantics are reviewed.

Verified Fineract table family:

- `m_tax_component`.
- `m_tax_component_history`.
- `m_tax_group`.
- `m_tax_group_mappings`.
- `m_savings_account_transaction_tax_details`.
- `m_loan_charge_tax_details`.
- `m_loan_charge_tax_detail`.

Detail file:

- `docs/reporting-data/tax.md`.

### 4.4 Custom Datatables — status: `deferred`

Status: deferred.

Reason:

- Custom datatables vary by installation.
- They may contain PII, local business fields, or poorly documented semantics.

Scope rule:

- Do not automatically expose custom datatables.
- Add each custom datatable explicitly after reviewing its columns and sensitivity.

Verified Fineract metadata table family:

- `x_registered_table`.
- `x_table_column_code_mappings`.
- `m_entity_datatable_check`.
- `m_code`.
- `m_code_value`.

Detail file:

- `docs/reporting-data/custom-datatables.md`.

### 4.5 Audit, Users, And Operations — status: `deferred`

Status: deferred except basic created/approved user references when needed.

Reason:

- Operational audit can be useful but should not be mixed into business reporting before core data definitions are stable.
- User tables contain PII and credential fields.
- Command source tables can contain raw request JSON, raw result payloads, idempotency keys, and client IP addresses.

Future high-level data concepts:

- App user.
- Role and permission assignments.
- Maker/checker command source.
- Created by / approved by / rejected by users.
- Manual transaction flags.

Scope rule:

- Include operational audit reporting only after explicit approval.
- Never expose passwords, temporary passwords, authentication tokens, raw command JSON, raw command results, or idempotency keys.
- Treat usernames, names, emails, roles, permissions, and client IPs as sensitive operational data.

Verified Fineract table family:

- `m_appuser`.
- `m_appuser_role`.
- `m_role`.
- `m_permission`.
- `m_role_permission`.
- `m_portfolio_command_source`.
- `request_audit_table`.

Detail file:

- `docs/reporting-data/audit-users-operations.md`.
