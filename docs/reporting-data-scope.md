# Reporting Data Scope

This document defines the high-level data areas that the AI Reporting Service may use from the Fineract read-only database.

The purpose of this document is to establish the reporting surface before mapping exact tables, columns, joins, metrics, and approved questions.

Detailed field/table mapping will be added later per area.

This document is the human-approved source of truth for allowed reporting data. The machine-readable counterpart belongs under `knowledge/data-scope/` and is defined by `docs/knowledge-catalog.md`.

## 0. Status Legend And Area Index

Each of the 13 data areas below is tagged with a lifecycle status. Reviewers should update the tag on the area's subsection heading whenever the coverage matrix moves a related capability. Tags are the same across this doc, the runtime `knowledge/data-scope/areas/*.yaml`, and [`docs/capability-coverage-matrix.md`](./capability-coverage-matrix.md).

| Status | Meaning |
| --- | --- |
| `in_use` | At least one enabled (runtime `approved_mvp`) capability queries this area today. Tables here are validated as safe to widen incrementally. |
| `frontier` | Not queried today, but the next `planned` capability will use it. Activation criteria below must be met before flipping to `in_use`. |
| `conditional` | Queryable but only under a narrower feature flag (e.g. group/center scope only when the tenant enables group workflows). |
| `deferred` | Whole area is deferred. No capability may reference it. Requires domain-level approval, not just a code change, to activate. |
| `rejected` | Permanent out-of-scope. Even future roadmap items will not be built against these tables. |

Every in-scope area listed below is a commitment — capabilities that use it are either `implemented` today, `planned` on the roadmap, or `deferred` (activated at a domain-level approval). Area index with current tags (four areas `in_use` today; every other in-scope area has planned capabilities):

| # | Area | Status | Approved capabilities that touch it |
| --- | --- | --- | --- |
| 3.1 | Organization Foundation | `in_use` | All 9 approved capabilities (office join for scope) |
| 3.2 | Client Foundation | `in_use` | All top-N capabilities (client display join) |
| 3.3 | Group And Center Foundation | `conditional` | none today; enabled per-tenant |
| 3.4 | Savings Core | `in_use` | `savings_balance_summary` plus every deposit/withdrawal capability |
| 3.5 | Savings Transactions | `in_use` | All deposit/withdrawal capabilities |
| 3.6 | Savings Charges And Fees | `frontier` | none today; targets `savings_charge_outstanding_*` (v0.2 – v0.3) |
| 4.1 | Loans | `deferred` | — |
| 4.2 | Accounting And General Ledger | `deferred` | — |
| 4.3 | Tax | `deferred` | — |
| 4.4 | Custom Datatables | `deferred` | — |
| 4.5 | Audit, Users, And Operations | `deferred` | — |
| 5   | Explicitly Out Of Scope | `rejected` | — |

Activation criteria for `frontier` and `conditional` areas — what must be true before flipping to `in_use`:

- **Savings Charges And Fees (`frontier` → `in_use`).** Charge enum mapping (`m_charge.charge_type_enum`, `m_charge.charge_calculation_enum`) must be documented in a schema knowledge file. The `m_savings_account_charge.amount_outstanding_derived` semantics must be reviewed to confirm it is a running balance we can trust across time zones. Sensitivity class of the charge reference identifier must be assigned in `docs/reporting-pii-policy.md` (currently reserved as `sensitive_business_identifier`).
- **Group And Center Foundation (`conditional` remains `conditional`).** Only queryable when the API key `allowed_capabilities` set includes a group-scoped capability AND the tenant is configured for group workflows. The group office path (`m_group.office_id`) is validated separately from the client office path in the same query.
- **Any `deferred` area (`deferred` → `frontier`).** A capability YAML must be authored under `knowledge/capabilities/<domain>/`, the coverage matrix must gain an `implemented` entry (row flipped from `deferred`), the domain YAML under `knowledge/domains/` must be moved from deferred to approved, and a PII rule review under `docs/reporting-pii-policy.md` must sign off on every field the new capability will output.

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
- Every approved, conditional, deferred, and out-of-scope data area must have a matching machine-readable entry under `knowledge/data-scope/` before catalog validation is considered complete.
- Knowledge files must not introduce runtime access to tables, columns, joins, metrics, or response fields outside this scope.

## 2. Current In-Scope Reporting Surface

The in-scope reporting surface for the AI chatbot is:

```text
Organization + Client + Savings + (Group/Center, conditional)
```

This covers all reasonable savings-activity questions plus foundation lookups (offices, clients, groups). Loan, accounting/GL, tax, custom datatables, and audit/users/operations are deferred domains — inside the product commitment for a future milestone, but not yet activated. See §4 and Category G in the coverage matrix.

The current in-scope surface is not a "first implementation, more to come" ceiling on capability breadth — it defines which Fineract tables approved SQL may touch today. Within this surface, capability shortage is a `planned` gap, not a scope restriction.

## 3. Included Data Areas

### 3.1 Organization Foundation — status: `in_use`

Status: in-scope foundation.

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

### 3.2 Client Foundation — status: `in_use`

Status: in-scope foundation.

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

### 3.3 Group And Center Foundation — status: `conditional`

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

### 3.4 Savings Core — status: `in_use`

Status: in-scope domain.

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

### 3.5 Savings Transactions — status: `in_use`

Status: in-scope domain.

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

### 3.6 Savings Charges And Fees — status: `frontier`

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

## 5. Explicitly Out Of Scope (permanent)

The following are permanently out-of-scope and will never be built:

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
