# Reporting Data Scope

This document defines the high-level data areas that the AI Reporting Service may use from the Fineract read-only database.

The purpose of this document is to establish the reporting surface before mapping exact tables, columns, joins, metrics, and approved questions.

Detailed field/table mapping will be added later per area.

## 1. Scope Principle

The service must not treat the full Fineract database as available reporting context.

Only explicitly approved data areas may be used.

Rules:

- Read from Fineract through `FINERACT_DATABASE_URL` only.
- Do not modify Fineract data or schema.
- Do not let AI generate or execute arbitrary SQL.
- Runtime queries must come from approved reporting capabilities.
- Each approved capability must declare its allowed tables, joins, filters, metrics, and PII behavior.
- If a user asks for data outside the approved scope, reject or ask for clarification.

## 2. Initial MVP Scope

The recommended first reporting scope is:

```text
Client + Organization + Savings
```

This gives the service enough context to answer business reporting needs around savings activity while keeping the first implementation bounded and auditable.

The first scope should not include the full loan, accounting, tax, or custom datatable surface.

## 3. Included Data Areas

### 3.1 Organization Foundation

Status: included in MVP foundation.

Purpose:

- Provide organizational hierarchy and branch/office filters.
- Support authorization by allowed office ids.
- Anchor all client and account reporting to office-level access control.

High-level data concepts:

- Office.
- Parent office.
- Office hierarchy.
- Office opening date.
- Staff assigned to an office.
- Active/inactive staff.

Verified Fineract table family:

- `m_office`.
- `m_staff`.
- `m_staff_assignment_history`, later if center/staff assignment history is needed.

Detail file:

- `docs/reporting-data/organization-foundation.md`.

### 3.2 Client Foundation

Status: included in MVP foundation.

Purpose:

- Identify customers connected to savings and future loan reports.
- Provide client lifecycle/status filters.
- Support office-scoped reporting.
- Support PII-aware output masking.

High-level data concepts:

- Client identity.
- Client account number.
- Client external id.
- Client status.
- Client office.
- Assigned staff.
- Client activation/submission/closure lifecycle.
- Client type/classification.
- Client legal form.
- Basic contact fields, subject to PII rules.

Verified Fineract table family:

- `m_client`.
- `m_client_identifier`, later and only if identity document reporting is approved.
- `m_client_address`, later and only if address/location reporting is approved.
- `m_client_non_person`, later if entity/business clients are in scope.
- `m_client_transfer_details`, later if client transfer reporting is needed.

PII note:

- Client names, phone numbers, email addresses, identifiers, dates of birth, and addresses must be treated as PII or sensitive client data.
- Default behavior should be aggregate reporting or masked output unless `can_view_pii=true`.

Detail file:

- `docs/reporting-data/client-foundation.md`.

### 3.3 Group And Center Foundation

Status: conditionally included.

Purpose:

- Support installations that use group/center-based client organization.
- Allow savings reporting by group or center if the Fineract setup uses this model.

High-level data concepts:

- Group.
- Center.
- Group hierarchy.
- Client membership in group.
- Group staff assignment.

Verified Fineract table family:

- `m_group`.
- `m_group_client`.
- `m_group_level`.
- `m_group_roles`.
- `m_staff_assignment_history`, if needed.

Scope rule:

- Include this area only if the local Fineract usage relies on groups/centers.
- Otherwise keep it as future/optional context.

Detail file:

- `docs/reporting-data/group-center-foundation.md`.

### 3.4 Savings Core

Status: included in MVP domain.

Purpose:

- Provide the first business reporting domain.
- Cover savings accounts, savings products, balances, deposits, withdrawals, and account lifecycle.

High-level data concepts:

- Savings account.
- Savings product.
- Savings account status.
- Savings account owner: client or group.
- Savings account office via client/group/transaction context.
- Currency.
- Account balance.
- Total deposits.
- Total withdrawals.
- Interest earned/posted.
- Fees and penalties derived on the account.
- Account activation/closure lifecycle.

Verified Fineract table family:

- `m_savings_account`.
- `m_savings_product`.
- `m_savings_product_charge`, later if product charge reporting is needed.
- `m_savings_officer_assignment_history`, later if field officer history is needed.

Detail file:

- `docs/reporting-data/savings-core.md`.

### 3.5 Savings Transactions

Status: included in MVP domain.

Purpose:

- Support transaction-level reporting for savings movement.
- Provide deposit, withdrawal, interest posting, fee, reversal, and balance movement context.

High-level data concepts:

- Savings transaction.
- Transaction date.
- Transaction type.
- Transaction amount.
- Reversal flag.
- Running balance.
- Office associated with the transaction.
- Payment detail reference, later if payment/channel reporting is approved.
- Manual transaction flag.
- Created date and app user, later if operational audit reporting is approved.

Verified Fineract table family:

- `m_savings_account_transaction`.
- `m_payment_detail`, later if payment/channel reporting is approved.
- Payment type/reference tables, later if payment/channel reporting is approved.

Scope rule:

- Reversed transactions must be excluded by default unless the approved capability explicitly asks for reversal analysis.
- Transaction type mapping must be documented before using transaction numbers in responses.

Detail file:

- `docs/reporting-data/savings-transactions.md`.

### 3.6 Savings Charges And Fees

Status: included as secondary savings scope.

Purpose:

- Support reporting on savings charges, fees, penalties, paid amounts, waived amounts, and outstanding amounts.

High-level data concepts:

- Account-level savings charge.
- Charge amount.
- Paid amount.
- Waived amount.
- Written-off amount.
- Outstanding amount.
- Paid-by transaction relationship.
- Active/inactive charge state.

Verified Fineract table family:

- `m_savings_account_charge`.
- `m_savings_account_charge_paid_by`.
- `m_savings_product_charge`, later if product charge configuration reporting is needed.
- `m_charge`, shared charge definition table.

Scope rule:

- Keep this secondary until the core savings account and savings transaction scope is stable.

Detail file:

- `docs/reporting-data/savings-charges-fees.md`.

## 4. Deferred Data Areas

### 4.1 Loans

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

- Do not include loan tables in MVP capabilities until savings reporting and authorization are stable.

Verified Fineract table family:

- `m_loan`.
- `m_product_loan`.
- `m_loan_transaction`.
- `m_loan_repayment_schedule`.
- `m_loan_charge`.

Detail file:

- `docs/reporting-data/loans.md`.

### 4.2 Accounting And General Ledger

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

- Do not include accounting tables in MVP capabilities.
- Add only after approved accounting definitions are documented.

Verified Fineract table family:

- `acc_gl_account`.
- `acc_gl_journal_entry`.
- `acc_accounting_rule`.
- `acc_product_mapping`.
- `acc_gl_closure`.

Detail file:

- `docs/reporting-data/accounting-gl.md`.

### 4.3 Tax

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

### 4.4 Custom Datatables

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

### 4.5 Audit, Users, And Operations

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

## 5. Explicitly Out Of Scope For MVP

The following are out of scope for the first implementation:

- Arbitrary SQL exploration.
- Full Fineract schema search.
- Loan account reporting.
- Accounting/GL reporting.
- Tax reporting.
- Custom datatable reporting.
- Document/image/file reporting.
- Identity document reporting.
- Address-level reporting.
- User audit reporting beyond fields needed for traceability.
- Any write/update/delete action against Fineract.

## 6. Authorization Boundaries

Every reporting capability must be checked against API key scope.

Required authorization dimensions:

- Allowed capabilities.
- Allowed office ids.
- PII visibility through `can_view_pii`.

Office rules:

- Office filtering must apply to client/account/report queries.
- A caller should not be able to bypass `allowed_office_ids` through user-provided filters.
- Office hierarchy behavior must be defined before allowing parent-office rollups.

PII rules:

- Aggregates should be preferred by default.
- Client-level rows require `can_view_pii=true` if they include identifying fields.
- If `can_view_pii=false`, names and contact fields must be omitted or masked.

## 7. Required Follow-Up Documents

Detailed data documents are tracked in this order:

1. `docs/reporting-data/organization-foundation.md`.
2. `docs/reporting-data/client-foundation.md`.
3. `docs/reporting-data/group-center-foundation.md`.
4. `docs/reporting-data/savings-core.md`.
5. `docs/reporting-data/savings-transactions.md`.
6. `docs/reporting-data/savings-charges-fees.md`.
7. `docs/reporting-data/loans.md`.
8. `docs/reporting-data/accounting-gl.md`.
9. `docs/reporting-data/tax.md`.
10. `docs/reporting-data/custom-datatables.md`.
11. `docs/reporting-data/audit-users-operations.md`.
12. `docs/reporting-capabilities.md`.
13. `docs/reporting-pii-policy.md`.

Each detailed data document should include:

- Included tables.
- Excluded tables.
- Column mapping.
- Join rules.
- Status enum mapping.
- Transaction type enum mapping, when relevant.
- Allowed filters.
- Allowed aggregate metrics.
- PII/sensitive fields.
- Notes from Fineract source code or schema.
