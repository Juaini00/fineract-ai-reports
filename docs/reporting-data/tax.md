# Reporting Data: Tax

This document contains the detailed table and field scope for Reporting Data Scope section `4.3 Tax`.

Status: deferred, documented for future activation.

## 1. Scope

Tax reporting is deferred.

Reason:

- Tax calculations and tax withholding need careful business validation.
- Tax details should not be inferred casually from base transaction amounts.
- Tax configuration can affect savings, charges, and loans differently.

Verified Fineract table family:

- `m_tax_component`.
- `m_tax_component_history`.
- `m_tax_group`.
- `m_tax_group_mappings`.
- `m_savings_account_transaction_tax_details`.
- `m_loan_charge_tax_details`.
- `m_loan_charge_tax_detail`.

Explicitly excluded from this scope:

- `mix_taxonomy`.
- `mix_taxonomy_mapping`.

Reason: these are MIX taxonomy tables, not financial tax tables for savings/loan tax reporting.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml` plus later loan tax migrations.
- Fineract tax domain package under `fineract-tax`.
- Fineract savings tax domain model: `SavingsAccountTransactionTaxDetails`.
- Fineract loan tax domain model: `LoanChargeTaxDetails`.
- Local database `information_schema.columns` on `fineract_default`.

Activation rule:

- Do not include tax-specific reporting until exact tax semantics are reviewed.
- Before promotion, define whether the report uses configured tax components, historical tax component rates, actual transaction tax rows, or product/account tax group references.

## 2. `m_tax_component`

Purpose:

- Canonical tax component definition table.
- Stores tax name, current percentage, accounting mapping, and start date.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary tax component id. |
| `name` | `character varying` | no | Tax component name. |
| `percentage` | `numeric` | no | Current tax percentage. |
| `debit_account_type_enum` | `smallint` | yes | Debit account type enum. Accounting/tax mapping scope only. |
| `debit_account_id` | `bigint` | yes | Debit GL/account id. Accounting scope only. |
| `credit_account_type_enum` | `smallint` | yes | Credit account type enum. Accounting/tax mapping scope only. |
| `credit_account_id` | `bigint` | yes | Credit GL/account id. Accounting scope only. |
| `start_date` | `date` | no | Tax component start date. |
| `createdby_id` | `bigint` | no | Audit creator user id. Exclude unless operational audit scope is enabled. |
| `created_date` | `timestamp without time zone` | no | Audit creation timestamp. Exclude unless operational audit scope is enabled. |
| `lastmodifiedby_id` | `bigint` | no | Audit updater user id. Exclude unless operational audit scope is enabled. |
| `lastmodified_date` | `timestamp without time zone` | no | Audit update timestamp. Exclude unless operational audit scope is enabled. |

Relationship rules:

- `m_tax_group_mappings.tax_component_id -> m_tax_component.id`.
- `m_tax_component_history.tax_component_id -> m_tax_component.id`.
- `m_savings_account_transaction_tax_details.tax_component_id -> m_tax_component.id`.
- `m_loan_charge_tax_details.tax_component_id -> m_tax_component.id`.
- `m_loan_charge_tax_detail.tax_component_id -> m_tax_component.id`.

Reporting rules:

- Use `id`, `name`, `percentage`, and `start_date` for tax configuration reports.
- Do not use current `percentage` to recalculate historical transaction tax unless historical rate rules are defined.
- Do not expose accounting account fields until accounting/GL scope is approved.

## 3. `m_tax_component_history`

Purpose:

- Stores historical percentage ranges for tax components.
- Needed if reports must explain tax rates over time.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary tax component history id. |
| `tax_component_id` | `bigint` | no | Tax component id. Join to `m_tax_component.id`. |
| `percentage` | `numeric` | no | Historical tax percentage. |
| `start_date` | `date` | no | History start date. |
| `end_date` | `date` | no | History end date. |
| `createdby_id` | `bigint` | no | Audit creator user id. Exclude unless operational audit scope is enabled. |
| `created_date` | `timestamp without time zone` | no | Audit creation timestamp. Exclude unless operational audit scope is enabled. |
| `lastmodifiedby_id` | `bigint` | no | Audit updater user id. Exclude unless operational audit scope is enabled. |
| `lastmodified_date` | `timestamp without time zone` | no | Audit update timestamp. Exclude unless operational audit scope is enabled. |

Reporting rules:

- Use only for tax configuration/history reports.
- Do not use to recompute transaction tax unless approved tax calculation semantics are documented.

## 4. `m_tax_group`

Purpose:

- Canonical tax group table.
- Tax groups are referenced by savings products, savings accounts, and charges.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary tax group id. |
| `name` | `character varying` | no | Tax group name. |
| `createdby_id` | `bigint` | no | Audit creator user id. Exclude unless operational audit scope is enabled. |
| `created_date` | `timestamp without time zone` | no | Audit creation timestamp. Exclude unless operational audit scope is enabled. |
| `lastmodifiedby_id` | `bigint` | no | Audit updater user id. Exclude unless operational audit scope is enabled. |
| `lastmodified_date` | `timestamp without time zone` | no | Audit update timestamp. Exclude unless operational audit scope is enabled. |

Relationship rules:

- `m_tax_group_mappings.tax_group_id -> m_tax_group.id`.
- `m_savings_product.tax_group_id -> m_tax_group.id`.
- `m_savings_account.tax_group_id -> m_tax_group.id`.
- `m_charge.tax_group_id -> m_tax_group.id`.

Reporting rules:

- Use for tax configuration reports only until tax reporting is approved.
- Do not infer actual tax paid from tax group configuration.

## 5. `m_tax_group_mappings`

Purpose:

- Maps tax groups to tax components over date ranges.
- Needed to understand which components belong to each tax group.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary tax group mapping id. |
| `tax_group_id` | `bigint` | no | Tax group id. Join to `m_tax_group.id`. |
| `tax_component_id` | `bigint` | no | Tax component id. Join to `m_tax_component.id`. |
| `start_date` | `date` | no | Mapping start date. |
| `end_date` | `date` | yes | Mapping end date. `NULL` likely indicates current mapping. |
| `createdby_id` | `bigint` | no | Audit creator user id. Exclude unless operational audit scope is enabled. |
| `created_date` | `timestamp without time zone` | no | Audit creation timestamp. Exclude unless operational audit scope is enabled. |
| `lastmodifiedby_id` | `bigint` | no | Audit updater user id. Exclude unless operational audit scope is enabled. |
| `lastmodified_date` | `timestamp without time zone` | no | Audit update timestamp. Exclude unless operational audit scope is enabled. |

Reporting rules:

- Use only for tax group configuration reports.
- Date-effective tax group behavior must be reviewed before using this table in transaction explanations.

## 6. `m_savings_account_transaction_tax_details`

Purpose:

- Stores actual tax details attached to savings account transactions.
- This is the safest source for actual savings transaction tax amount reporting, once tax scope is approved.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary savings transaction tax detail id. |
| `savings_transaction_id` | `bigint` | no | Savings transaction id. Join to `m_savings_account_transaction.id`. |
| `tax_component_id` | `bigint` | no | Tax component id. Join to `m_tax_component.id`. |
| `amount` | `numeric` | no | Actual tax amount for the savings transaction and component. |

Relationship rules:

- `m_savings_account_transaction_tax_details.savings_transaction_id -> m_savings_account_transaction.id`.
- `m_savings_account_transaction_tax_details.tax_component_id -> m_tax_component.id`.

Reporting rules:

- Use only after savings transaction reporting is approved and tax scope is promoted.
- Exclude reversed savings transactions by following savings transaction reversal rules.
- Prefer actual `amount` from this table over recalculating from tax percentages.

## 7. `m_loan_charge_tax_details`

Purpose:

- Stores tax amount by loan charge and tax component.
- Loan tax scope only.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary loan charge tax detail id. |
| `loan_charge_id` | `bigint` | no | Loan charge id. Join to `m_loan_charge.id`. |
| `tax_component_id` | `bigint` | no | Tax component id. Join to `m_tax_component.id`. |
| `amount` | `numeric` | no | Tax amount for loan charge and component. |

Reporting rules:

- Exclude until loan scope and tax scope are both approved.

## 8. `m_loan_charge_tax_detail`

Purpose:

- Stores tax details linking loan transactions, loan charges, and tax components.
- Loan transaction tax scope only.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary loan charge tax transaction detail id. |
| `loan_transaction_id` | `bigint` | no | Loan transaction id. Join to `m_loan_transaction.id`. |
| `loan_charge_id` | `bigint` | no | Loan charge id. Join to `m_loan_charge.id`. |
| `tax_component_id` | `bigint` | no | Tax component id. Join to `m_tax_component.id`. |
| `amount` | `numeric` | no | Tax amount for loan transaction/charge/component. |

Reporting rules:

- Exclude until loan scope and tax scope are both approved.
- Do not mix this table with `m_loan_charge_tax_details` without reviewing Fineract loan tax posting semantics.

## 9. Related Tax References In Other Scoped Tables

These fields are documented in their own area files but are relevant for tax configuration:

- `m_savings_product.tax_group_id`.
- `m_savings_product.withhold_tax`.
- `m_savings_account.tax_group_id`.
- `m_savings_account.withhold_tax`.
- `m_savings_account.total_withhold_tax_derived`.
- `m_charge.tax_group_id`.
- `m_loan_charge.tax_amount`.

Reporting rules:

- Do not treat tax group references as actual tax paid.
- Actual savings transaction tax should use `m_savings_account_transaction_tax_details.amount` after approval.
- Loan tax requires separate approval with loan scope.

## 10. Required Before Promoting Tax To Approved Scope

Before enabling tax reporting capabilities, document:

- Whether the report is configuration-based or actual-transaction-based.
- Tax group effective-date behavior.
- Tax component history behavior.
- Savings withholding tax semantics.
- Loan charge tax semantics.
- How to handle reversed transactions.
- Whether tax amounts should be grouped by component, tax group, product, office, or account.
- Whether tax fields are needed for accounting reconciliation.
- Whether tax data should be visible to all API keys or only a specific capability.

## 11. Initial Tax Activation Candidate

When Tax is promoted from deferred to approved scope, start narrowly:

- Tax component list.
- Tax group list.
- Tax group to component mapping.
- Savings transaction tax totals by date range and tax component.

Candidate tables for first activation:

- `m_tax_component`.
- `m_tax_group`.
- `m_tax_group_mappings`.
- `m_savings_account_transaction_tax_details`.

Keep out of first activation:

- Loan charge tax.
- Tax accounting mappings.
- Recalculated historical tax.
- MIX taxonomy tables.
