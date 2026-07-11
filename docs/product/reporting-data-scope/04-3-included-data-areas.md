# Reporting Data Scope: 3. Included Data Areas

Source: `docs-old/reporting-data-scope.md`

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
