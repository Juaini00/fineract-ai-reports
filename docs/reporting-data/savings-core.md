# Reporting Data: Savings Core

This document contains the detailed table and field scope for Reporting Data Scope section `3.4 Savings Core`.

Status: completed for initial review.

## 1. Scope

Savings Core is included in the MVP domain.

Purpose:

- Provide the first business reporting domain.
- Cover savings accounts, savings products, balances, lifecycle, ownership, and product setup.
- Provide the base account/product dimensions used by savings transaction reporting.

High-level data concepts:

- Savings account.
- Savings product.
- Savings account status.
- Savings account owner: client or group.
- Savings account office through client/group/transaction context.
- Currency.
- Account balance.
- Total deposits and withdrawals derived at account level.
- Interest earned/posted derived at account level.
- Fees and penalties derived at account level.
- Account activation/closure lifecycle.

Verified Fineract table family:

- `m_savings_account`.
- `m_savings_product`.
- `m_savings_product_charge`, later if product charge reporting is needed.
- `m_savings_officer_assignment_history`, later if field officer history is needed.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml` plus later savings migrations.
- Fineract domain models: `SavingsAccount`, `SavingsProduct`, `SavingsOfficerAssignmentHistory`.
- Local database `information_schema.columns` on `fineract_default`.

## 2. `m_savings_account`

Purpose:

- Canonical savings account table.
- Stores account ownership, product link, lifecycle dates, status, configuration copied from product, and account-level derived balances/totals.

### 2.1 Identity And Ownership Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary savings account id. Use for joins. |
| `account_no` | `character varying` | no | Savings account number. Sensitive business identifier. |
| `external_id` | `character varying` | yes | External account identifier. Sensitive business identifier. |
| `client_id` | `bigint` | yes | Client owner id. Join to `m_client.id`. |
| `group_id` | `bigint` | yes | Group owner id. Join to `m_group.id` if group scope is enabled. |
| `gsim_id` | `bigint` | yes | Group savings individual monitoring id. Out of MVP unless GSIM is approved. |
| `product_id` | `bigint` | yes | Savings product id. Join to `m_savings_product.id`. |
| `field_officer_id` | `bigint` | yes | Field officer id. Join to `m_staff.id`. |
| `iban` | `character varying` | yes | IBAN/account identifier. Sensitive; exclude from MVP output. |

Relationship rules:

- `m_savings_account.client_id -> m_client.id` for client-owned accounts.
- `m_savings_account.group_id -> m_group.id` for group-owned accounts.
- `m_savings_account.product_id -> m_savings_product.id`.
- `m_savings_account.field_officer_id -> m_staff.id`.
- `m_savings_account.id -> m_savings_account_transaction.savings_account_id` for transaction reporting.

Reporting rules:

- Use `id` as canonical account key.
- Use owner path to enforce office authorization:
  - client-owned account: `m_savings_account.client_id -> m_client.office_id`.
  - group-owned account: `m_savings_account.group_id -> m_group.office_id` when group scope is enabled.
- Do not expose `account_no`, `external_id`, or `iban` by default.

### 2.2 Status And Lifecycle Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `status_enum` | `smallint` | no | Savings account status enum. Needs enum mapping before lifecycle reports. |
| `sub_status_enum` | `smallint` | no | Savings account sub-status enum. Needs enum mapping. |
| `account_type_enum` | `smallint` | no | Account type enum. Needs enum mapping. |
| `deposit_type_enum` | `smallint` | no | Deposit type enum. Needs enum mapping. |
| `submittedon_date` | `date` | no | Submission date. |
| `submittedon_userid` | `bigint` | yes | Submission user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `approvedon_date` | `date` | yes | Approval date. |
| `approvedon_userid` | `bigint` | yes | Approval user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `rejectedon_date` | `date` | yes | Rejection date. |
| `rejectedon_userid` | `bigint` | yes | Rejection user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `withdrawnon_date` | `date` | yes | Withdrawn date. |
| `withdrawnon_userid` | `bigint` | yes | Withdrawal user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `activatedon_date` | `date` | yes | Activation date. |
| `activatedon_userid` | `bigint` | yes | Activation user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `closedon_date` | `date` | yes | Closure date. |
| `closedon_userid` | `bigint` | yes | Closure user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `reason_for_block` | `character varying` | yes | Block reason text. Sensitive/free text; exclude from MVP output. |
| `last_closed_business_date` | `date` | yes | Last closed business date for account processing. Operational; not default business reporting. |

Reporting rules:

- Use lifecycle date fields for account lifecycle filters.
- Use `status_enum` only after Fineract savings account status mapping is documented.
- Do not expose user id fields by default.
- Do not expose `reason_for_block` by default because it is free text and may contain sensitive details.

### 2.3 Currency And Interest Configuration Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `currency_code` | `character varying` | no | Account currency code. |
| `currency_digits` | `smallint` | no | Currency decimal digits. |
| `currency_multiplesof` | `smallint` | yes | Currency multiples-of value. |
| `nominal_annual_interest_rate` | `numeric` | no | Account nominal annual interest rate. |
| `interest_compounding_period_enum` | `smallint` | no | Interest compounding enum. Needs enum mapping. |
| `interest_posting_period_enum` | `smallint` | no | Interest posting enum. Needs enum mapping. |
| `interest_calculation_type_enum` | `smallint` | no | Interest calculation type enum. Needs enum mapping. |
| `interest_calculation_days_in_year_type_enum` | `smallint` | no | Days-in-year enum. Needs enum mapping. |
| `start_interest_calculation_date` | `date` | yes | Interest calculation start date. |
| `last_interest_calculation_date` | `date` | yes | Last interest calculation date. |
| `interest_posted_till_date` | `date` | yes | Interest posted-through date. |
| `accrued_till_date` | `date` | yes | Accrued-through date. |

Reporting rules:

- Include `currency_code` in monetary outputs.
- Interest configuration is included for product/account setup reporting, not for recomputing interest in MVP.
- Do not derive interest calculations manually from these fields unless a later approved capability defines exact rules.

### 2.4 Balance And Derived Amount Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `total_deposits_derived` | `numeric` | yes | Account-level derived total deposits. Useful for summary reporting. |
| `total_withdrawals_derived` | `numeric` | yes | Account-level derived total withdrawals. Useful for summary reporting. |
| `total_withdrawal_fees_derived` | `numeric` | yes | Derived withdrawal fees. Secondary scope. |
| `total_fees_charge_derived` | `numeric` | yes | Derived fee charges. Secondary scope. |
| `total_penalty_charge_derived` | `numeric` | yes | Derived penalty charges. Secondary scope. |
| `total_annual_fees_derived` | `numeric` | yes | Derived annual fees. Secondary scope. |
| `total_interest_earned_derived` | `numeric` | yes | Derived interest earned. |
| `total_interest_posted_derived` | `numeric` | yes | Derived interest posted. |
| `total_overdraft_interest_derived` | `numeric` | yes | Derived overdraft interest. Exclude unless overdraft scope is enabled. |
| `total_withhold_tax_derived` | `numeric` | yes | Derived withheld tax. Exclude until tax scope is approved. |
| `account_balance_derived` | `numeric` | no | Current derived account balance. Key MVP metric. |
| `on_hold_funds_derived` | `numeric` | yes | Funds on hold. Include only if hold/lien scope is approved. |
| `total_savings_amount_on_hold` | `numeric` | yes | Total savings amount on hold. Include only if hold/lien scope is approved. |
| `available_balance_derived` | `numeric` | yes | Available balance after holds/lien if populated. Key metric when present. |

Reporting rules:

- Prefer transaction-level aggregation for date-bounded movement reports.
- Use account-level `*_derived` fields for current snapshot metrics.
- Do not mix account-level lifetime derived totals with date-bounded transaction reports unless the capability explicitly defines the semantics.
- Exclude tax-derived fields until tax reporting is approved.

### 2.5 Limits, Holds, Overdraft, Tax, And Configuration Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `min_required_opening_balance` | `numeric` | yes | Minimum opening balance. Product/account setup reporting. |
| `lockin_period_frequency` | `numeric` | yes | Lock-in frequency. Needs enum interpretation. |
| `lockin_period_frequency_enum` | `smallint` | yes | Lock-in frequency enum. Needs enum mapping. |
| `withdrawal_fee_for_transfer` | `boolean` | yes | Fee-on-transfer flag. Secondary scope. |
| `allow_overdraft` | `boolean` | no | Overdraft allowed flag. |
| `overdraft_limit` | `numeric` | yes | Overdraft limit. Include only if overdraft scope is approved. |
| `nominal_annual_interest_rate_overdraft` | `numeric` | yes | Overdraft interest rate. Include only if overdraft scope is approved. |
| `min_overdraft_for_interest_calculation` | `numeric` | yes | Minimum overdraft for interest. Include only if overdraft scope is approved. |
| `lockedin_until_date_derived` | `date` | yes | Derived lock-in end date. |
| `min_required_balance` | `numeric` | yes | Minimum required balance. |
| `enforce_min_required_balance` | `boolean` | no | Minimum balance enforcement flag. |
| `min_balance_for_interest_calculation` | `numeric` | yes | Minimum balance for interest calculation. |
| `version` | `integer` | no | Optimistic lock/version. Exclude from reporting. |
| `withhold_tax` | `boolean` | no | Tax withholding flag. Exclude until tax scope is approved. |
| `tax_group_id` | `bigint` | yes | Tax group id. Exclude until tax scope is approved. |
| `max_allowed_lien_limit` | `numeric` | yes | Maximum allowed lien limit. Include only if lien scope is approved. |
| `is_lien_allowed` | `boolean` | no | Lien allowed flag. Include only if lien scope is approved. |
| `receivable_settlement_mode` | `integer` | yes | Receivable settlement mode. Needs enum mapping. Exclude from MVP. |

Reporting rules:

- Include only basic setup fields needed for account/product description in MVP.
- Exclude overdraft, lien, tax, and settlement details until explicitly approved.

### 2.6 Audit Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `created_by` | `bigint` | no | Audit creator user id. Exclude from MVP unless operational audit scope is enabled. |
| `last_modified_by` | `bigint` | no | Audit updater user id. Exclude from MVP unless operational audit scope is enabled. |
| `created_on_utc` | `timestamp with time zone` | no | Audit creation timestamp. Exclude from MVP unless operational audit scope is enabled. |
| `last_modified_on_utc` | `timestamp with time zone` | no | Audit update timestamp. Exclude from MVP unless operational audit scope is enabled. |

## 3. `m_savings_product`

Purpose:

- Canonical savings product table.
- Provides product-level dimensions and default configuration for savings accounts.

### 3.1 Product Identity Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary savings product id. Use for joins. |
| `name` | `character varying` | no | Product display name. |
| `short_name` | `character varying` | no | Product short name. |
| `description` | `character varying` | yes | Product description. Free text; use carefully. |
| `deposit_type_enum` | `smallint` | no | Deposit type enum. Needs enum mapping. |

### 3.2 Product Currency And Interest Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `currency_code` | `character varying` | no | Product currency code. |
| `currency_digits` | `smallint` | no | Currency decimal digits. |
| `currency_multiplesof` | `smallint` | yes | Currency multiples-of value. |
| `nominal_annual_interest_rate` | `numeric` | no | Product nominal annual interest rate. |
| `interest_compounding_period_enum` | `smallint` | no | Interest compounding enum. Needs enum mapping. |
| `interest_posting_period_enum` | `smallint` | no | Interest posting enum. Needs enum mapping. |
| `interest_calculation_type_enum` | `smallint` | no | Interest calculation enum. Needs enum mapping. |
| `interest_calculation_days_in_year_type_enum` | `smallint` | no | Days-in-year enum. Needs enum mapping. |

### 3.3 Product Rules And Configuration Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `min_required_opening_balance` | `numeric` | yes | Product minimum opening balance. |
| `lockin_period_frequency` | `numeric` | yes | Product lock-in frequency. Needs enum interpretation. |
| `lockin_period_frequency_enum` | `smallint` | yes | Product lock-in frequency enum. Needs enum mapping. |
| `accounting_type` | `smallint` | no | Accounting type enum. Exclude until accounting scope is approved. |
| `withdrawal_fee_amount` | `numeric` | yes | Withdrawal fee amount. Secondary scope. |
| `withdrawal_fee_type_enum` | `smallint` | yes | Withdrawal fee type enum. Needs enum mapping. |
| `withdrawal_fee_for_transfer` | `boolean` | yes | Fee-on-transfer flag. Secondary scope. |
| `allow_overdraft` | `boolean` | no | Overdraft allowed flag. |
| `overdraft_limit` | `numeric` | yes | Product overdraft limit. Exclude unless overdraft scope is approved. |
| `nominal_annual_interest_rate_overdraft` | `numeric` | yes | Product overdraft interest rate. Exclude unless overdraft scope is approved. |
| `min_overdraft_for_interest_calculation` | `numeric` | yes | Product min overdraft for interest. Exclude unless overdraft scope is approved. |
| `min_required_balance` | `numeric` | yes | Product minimum required balance. |
| `enforce_min_required_balance` | `boolean` | no | Minimum balance enforcement flag. |
| `min_balance_for_interest_calculation` | `numeric` | yes | Minimum balance for interest calculation. |
| `withhold_tax` | `boolean` | no | Tax withholding flag. Exclude until tax scope is approved. |
| `tax_group_id` | `bigint` | yes | Tax group id. Exclude until tax scope is approved. |
| `is_dormancy_tracking_active` | `boolean` | yes | Dormancy tracking flag. Secondary scope. |
| `days_to_inactive` | `integer` | yes | Days to inactive. Secondary scope. |
| `days_to_dormancy` | `integer` | yes | Days to dormancy. Secondary scope. |
| `days_to_escheat` | `integer` | yes | Days to escheat. Secondary scope. |
| `max_allowed_lien_limit` | `numeric` | yes | Max lien limit. Exclude unless lien scope is approved. |
| `is_lien_allowed` | `boolean` | no | Lien allowed flag. Exclude unless lien scope is approved. |
| `allow_account_level_override` | `boolean` | no | Account-level override flag. Product setup reporting only. |
| `receivable_settlement_mode` | `integer` | no | Settlement mode enum. Exclude from MVP. |
| `default_charge_ordering_rule` | `integer` | no | Charge ordering rule enum. Exclude until charge scope is approved. |

Primary relationship rules:

- `m_savings_account.product_id -> m_savings_product.id`.
- `m_client.default_savings_product -> m_savings_product.id`.
- `m_savings_product_charge.savings_product_id -> m_savings_product.id`.

Reporting rules:

- Use `id` as canonical product key.
- Use `name` and `short_name` for product display.
- Include `currency_code` in monetary product/account summaries.
- Do not use accounting, tax, overdraft, lien, or charge ordering fields until those scopes are approved.

## 4. `m_savings_product_charge`

Purpose:

- Maps savings products to charge definitions.
- Belongs mostly to Savings Charges And Fees scope, but is referenced here as product configuration.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `savings_product_id` | `bigint` | no | Savings product id. Join to `m_savings_product.id`. |
| `charge_id` | `bigint` | no | Charge id. Charge definition table must be mapped before use. |
| `settlement_priority` | `integer` | yes | Charge settlement priority. Exclude until charge scope is approved. |

Scope rule:

- Exclude from MVP output unless product charge reporting is approved.
- Do not join to charge definition tables until Savings Charges And Fees detail is reviewed.

## 5. `m_savings_officer_assignment_history`

Purpose:

- Historical assignment of savings officers to savings accounts.
- Useful for historical officer attribution if field officer changes over time.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary assignment history id. |
| `account_id` | `bigint` | no | Savings account id. Join to `m_savings_account.id`. |
| `savings_officer_id` | `bigint` | yes | Staff/officer id. Join to `m_staff.id`. |
| `start_date` | `date` | no | Assignment start date. |
| `end_date` | `date` | yes | Assignment end date. `NULL` means current assignment. |
| `created_by` | `bigint` | no | Audit creator user id. Exclude from MVP. |
| `created_date` | `timestamp without time zone` | yes | Legacy audit timestamp. Exclude from MVP. |
| `lastmodified_date` | `timestamp without time zone` | yes | Legacy audit timestamp. Exclude from MVP. |
| `last_modified_by` | `bigint` | no | Audit updater user id. Exclude from MVP. |
| `created_on_utc` | `timestamp with time zone` | no | Audit creation timestamp. Exclude from MVP. |
| `last_modified_on_utc` | `timestamp with time zone` | no | Audit update timestamp. Exclude from MVP. |

Scope rule:

- Exclude from MVP unless historical officer attribution is approved.
- For current officer attribution, prefer `m_savings_account.field_officer_id`.

## 6. MVP Inclusion Decision

Included immediately:

- `m_savings_account.id`.
- `m_savings_account.client_id`.
- `m_savings_account.group_id`, only if group/center scope is active.
- `m_savings_account.product_id`.
- `m_savings_account.field_officer_id`.
- `m_savings_account.status_enum`, after enum mapping is documented.
- `m_savings_account.sub_status_enum`, after enum mapping is documented.
- `m_savings_account.account_type_enum`, after enum mapping is documented.
- `m_savings_account.deposit_type_enum`, after enum mapping is documented.
- `m_savings_account.submittedon_date`.
- `m_savings_account.approvedon_date`.
- `m_savings_account.activatedon_date`.
- `m_savings_account.closedon_date`.
- `m_savings_account.currency_code`.
- `m_savings_account.currency_digits`.
- `m_savings_account.total_deposits_derived` for current/lifetime snapshot semantics only.
- `m_savings_account.total_withdrawals_derived` for current/lifetime snapshot semantics only.
- `m_savings_account.total_interest_earned_derived`.
- `m_savings_account.total_interest_posted_derived`.
- `m_savings_account.account_balance_derived`.
- `m_savings_account.available_balance_derived`.
- `m_savings_product.id`.
- `m_savings_product.name`.
- `m_savings_product.short_name`.
- `m_savings_product.deposit_type_enum`, after enum mapping is documented.
- `m_savings_product.currency_code`.
- `m_savings_product.nominal_annual_interest_rate`.

Conditionally included:

- `m_savings_account.account_no`, only for account-level output and subject to business identifier policy.
- `m_savings_account.external_id`, only for explicitly approved internal/reference use.
- `m_savings_account.nominal_annual_interest_rate`.
- `m_savings_account.interest_*` configuration fields, for setup reporting only.
- `m_savings_account.min_required_*` fields, for setup reporting only.
- `m_savings_account.lockin_*` fields, after enum mapping.
- `m_savings_account.on_hold_funds_derived`, only if hold/lien scope is approved.
- `m_savings_account.total_savings_amount_on_hold`, only if hold/lien scope is approved.
- `m_savings_product.description`, only if free-text exposure is approved.
- `m_savings_product_charge.*`, only if product charge reporting is approved.
- `m_savings_officer_assignment_history.*`, only if historical officer attribution is approved.

Excluded from MVP output:

- `m_savings_account.iban`.
- `m_savings_account.reason_for_block`.
- `m_savings_account.gsim_id` unless GSIM reporting is approved.
- `m_savings_account.total_withhold_tax_derived` until tax scope is approved.
- `m_savings_account.tax_group_id` until tax scope is approved.
- `m_savings_account.withhold_tax` until tax scope is approved.
- `m_savings_account.overdraft_*` fields until overdraft scope is approved.
- `m_savings_account.*lien*` fields until lien scope is approved.
- `m_savings_account.receivable_settlement_mode`.
- `m_savings_account.version`.
- All `*_userid`, `created_by`, `last_modified_by`, `created_on_utc`, and `last_modified_on_utc` audit fields unless operational audit scope is enabled.
- `m_savings_product.accounting_type` until accounting scope is approved.
- `m_savings_product.tax_group_id` and `withhold_tax` until tax scope is approved.
- `m_savings_product.receivable_settlement_mode`.
- `m_savings_product.default_charge_ordering_rule` until charge scope is approved.
