# Reporting Data: Savings Charges And Fees

This document contains the detailed table and field scope for Reporting Data Scope section `3.6 Savings Charges And Fees`.

Status: completed for initial review, secondary savings scope.

## 1. Scope

Savings Charges And Fees is included as a secondary savings scope.

Purpose:

- Support reporting on savings account charges, fees, penalties, paid amounts, waived amounts, written-off amounts, and outstanding amounts.
- Support product-level savings charge configuration when needed.
- Link charge payments to savings transactions.

High-level data concepts:

- Account-level savings charge.
- Product-level savings charge mapping.
- Shared charge definition.
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
- `m_savings_product_charge`.
- `m_charge`, shared charge definition table.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml` plus later audit migrations.
- Fineract domain models: `SavingsAccountCharge`, `SavingsAccountChargePaidBy`.
- Local database `information_schema.columns` on `fineract_default`.

Scope rule:

- Keep this secondary until core savings account and savings transaction reporting is stable.
- Include only charge data where `m_charge.charge_applies_to_enum` is confirmed to apply to savings.
- Do not mix loan/client charge reporting into this scope.

## 2. `m_savings_account_charge`

Purpose:

- Stores charges applied to a savings account.
- Contains account-specific amount, due date, paid/waived/written-off/outstanding derived values, and active/paid state.

### 2.1 Identity And Relationship Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary savings account charge id. |
| `savings_account_id` | `bigint` | no | Savings account id. Join to `m_savings_account.id`. |
| `charge_id` | `bigint` | no | Charge definition id. Join to `m_charge.id`. |

Primary relationship rules:

- `m_savings_account_charge.savings_account_id -> m_savings_account.id`.
- `m_savings_account_charge.charge_id -> m_charge.id`.
- `m_savings_account_charge_paid_by.savings_account_charge_id -> m_savings_account_charge.id`.

### 2.2 Charge Definition Snapshot Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `is_penalty` | `boolean` | no | Indicates penalty charge. |
| `charge_time_enum` | `smallint` | no | Charge timing enum. Needs enum mapping. |
| `charge_due_date` | `date` | yes | Charge due date. |
| `fee_on_month` | `smallint` | yes | Month for recurring/annual fee. Needs charge timing context. |
| `fee_on_day` | `smallint` | yes | Day for recurring fee. Needs charge timing context. |
| `fee_interval` | `smallint` | yes | Fee interval. Needs charge timing context. |
| `free_withdrawal_count` | `integer` | yes | Free withdrawal count. Withdrawal-fee scope only. |
| `charge_reset_date` | `date` | yes | Charge reset date. Withdrawal-fee/recurring charge scope only. |
| `charge_calculation_enum` | `smallint` | no | Charge calculation enum. Needs enum mapping. |
| `calculation_percentage` | `numeric` | yes | Percentage used for charge calculation. |
| `calculation_on_amount` | `numeric` | yes | Base amount for percentage calculation. |
| `settlement_priority` | `smallint` | yes | Settlement priority. Advanced charge settlement scope. |

Reporting rules:

- Use `is_penalty` to split fee vs penalty.
- Use charge enum fields only after charge enum mapping is documented.
- Recurring fee fields should not be interpreted without charge timing semantics.

### 2.3 Amount And State Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `amount` | `numeric` | no | Original/current charge amount. |
| `amount_paid_derived` | `numeric` | yes | Amount paid. Key charge metric. |
| `amount_waived_derived` | `numeric` | yes | Amount waived. Key charge metric. |
| `amount_writtenoff_derived` | `numeric` | yes | Amount written off. Key charge metric. |
| `amount_outstanding_derived` | `numeric` | no | Amount outstanding. Key charge metric. |
| `is_paid_derived` | `boolean` | no | Paid flag. |
| `waived` | `boolean` | no | Waived flag. |
| `is_active` | `boolean` | no | Active charge flag. |
| `inactivated_on_date` | `date` | yes | Inactivation date. |

Reporting rules:

- Use derived amount fields for account-level charge balances.
- Use `is_active = true` for current active charges unless the capability explicitly includes inactive charges.
- Use `amount_paid_derived`, `amount_waived_derived`, and `amount_writtenoff_derived` for summary reporting.

### 2.4 Audit Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `created_by` | `bigint` | no | Audit creator user id. Exclude from MVP unless operational audit scope is enabled. |
| `last_modified_by` | `bigint` | no | Audit updater user id. Exclude from MVP unless operational audit scope is enabled. |
| `created_on_utc` | `timestamp with time zone` | no | Audit creation timestamp. Exclude from MVP unless operational audit scope is enabled. |
| `last_modified_on_utc` | `timestamp with time zone` | no | Audit update timestamp. Exclude from MVP unless operational audit scope is enabled. |

## 3. `m_savings_account_charge_paid_by`

Purpose:

- Links savings charge payments to savings account transactions.
- Useful for tracing which transaction paid which charge and how much.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary paid-by id. |
| `savings_account_transaction_id` | `bigint` | no | Savings transaction id. Join to `m_savings_account_transaction.id`. |
| `savings_account_charge_id` | `bigint` | no | Savings account charge id. Join to `m_savings_account_charge.id`. |
| `amount` | `numeric` | no | Amount of the charge paid by the transaction. |

Primary relationship rules:

- `m_savings_account_charge_paid_by.savings_account_transaction_id -> m_savings_account_transaction.id`.
- `m_savings_account_charge_paid_by.savings_account_charge_id -> m_savings_account_charge.id`.

Reporting rules:

- Use this table for transaction-level fee/charge payment attribution.
- For high-level outstanding charge reports, `m_savings_account_charge` derived amount fields may be enough.
- Join to `m_savings_account_transaction` only when the report needs payment date or transaction context.

## 4. `m_savings_product_charge`

Purpose:

- Maps savings products to charge definitions.
- Useful for product charge configuration reporting.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `savings_product_id` | `bigint` | no | Savings product id. Join to `m_savings_product.id`. |
| `charge_id` | `bigint` | no | Charge definition id. Join to `m_charge.id`. |
| `settlement_priority` | `integer` | yes | Settlement priority. Advanced charge settlement scope. |

Primary relationship rules:

- `m_savings_product_charge.savings_product_id -> m_savings_product.id`.
- `m_savings_product_charge.charge_id -> m_charge.id`.

Reporting rules:

- Use for product setup/configuration reports.
- Do not treat product charge mappings as actual assessed/paid charges. Use `m_savings_account_charge` for account-level applied charges.

## 5. `m_charge`

Purpose:

- Shared charge definition table used by multiple Fineract domains.
- In this scope, only savings-applicable charge definitions should be used.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary charge definition id. |
| `name` | `character varying` | yes | Charge display name. |
| `currency_code` | `character varying` | no | Charge currency code. |
| `charge_applies_to_enum` | `smallint` | no | Domain the charge applies to. Needs enum mapping; must be savings for this scope. |
| `charge_time_enum` | `smallint` | no | Charge timing enum. Needs enum mapping. |
| `charge_calculation_enum` | `smallint` | no | Charge calculation enum. Needs enum mapping. |
| `charge_payment_mode_enum` | `smallint` | yes | Payment mode enum. Needs enum mapping. |
| `amount` | `numeric` | no | Default charge amount. |
| `fee_on_day` | `smallint` | yes | Day for recurring fee. |
| `fee_interval` | `smallint` | yes | Fee interval. |
| `fee_on_month` | `smallint` | yes | Month for recurring/annual fee. |
| `is_penalty` | `boolean` | no | Penalty flag. |
| `is_active` | `boolean` | no | Active flag. |
| `is_deleted` | `boolean` | no | Deleted flag. Exclude deleted charges by default. |
| `min_cap` | `numeric` | yes | Minimum cap. Advanced charge calculation scope. |
| `max_cap` | `numeric` | yes | Maximum cap. Advanced charge calculation scope. |
| `fee_frequency` | `smallint` | yes | Fee frequency. Needs enum mapping. |
| `is_free_withdrawal` | `boolean` | no | Free withdrawal charge flag. |
| `free_withdrawal_charge_frequency` | `integer` | yes | Free withdrawal charge frequency. |
| `restart_frequency` | `integer` | yes | Restart frequency. |
| `restart_frequency_enum` | `integer` | yes | Restart frequency enum. Needs enum mapping. |
| `is_payment_type` | `boolean` | yes | Indicates payment-type-linked charge. |
| `payment_type_id` | `integer` | yes | Payment type id. Join to `m_payment_type.id` if payment/channel scope is approved. |
| `income_or_liability_account_id` | `bigint` | yes | Accounting account id. Exclude until accounting scope is approved. |
| `tax_group_id` | `bigint` | yes | Tax group id. Exclude until tax scope is approved. |
| `settlement_priority` | `integer` | yes | Default settlement priority. Advanced charge settlement scope. |

Reporting rules:

- Use `id`, `name`, `currency_code`, `amount`, `is_penalty`, `is_active`, and `is_deleted` for basic charge definition reporting.
- Filter `is_deleted = false` by default.
- Filter `is_active = true` for active charge definition reporting.
- Use `charge_applies_to_enum` only after mapping confirms savings-applicable values.
- Exclude accounting and tax columns until those scopes are approved.

## 6. MVP Inclusion Decision

Included after savings core/transaction reporting is stable:

- `m_savings_account_charge.id`.
- `m_savings_account_charge.savings_account_id`.
- `m_savings_account_charge.charge_id`.
- `m_savings_account_charge.is_penalty`.
- `m_savings_account_charge.charge_time_enum`, after enum mapping is documented.
- `m_savings_account_charge.charge_due_date`.
- `m_savings_account_charge.charge_calculation_enum`, after enum mapping is documented.
- `m_savings_account_charge.amount`.
- `m_savings_account_charge.amount_paid_derived`.
- `m_savings_account_charge.amount_waived_derived`.
- `m_savings_account_charge.amount_writtenoff_derived`.
- `m_savings_account_charge.amount_outstanding_derived`.
- `m_savings_account_charge.is_paid_derived`.
- `m_savings_account_charge.waived`.
- `m_savings_account_charge.is_active`.
- `m_savings_account_charge.inactivated_on_date`.
- `m_charge.id`.
- `m_charge.name`.
- `m_charge.currency_code`.
- `m_charge.charge_applies_to_enum`, after enum mapping is documented.
- `m_charge.is_penalty`.
- `m_charge.is_active`.
- `m_charge.is_deleted`.

Conditionally included:

- `m_savings_account_charge_paid_by.*`, only for transaction-level charge payment attribution.
- `m_savings_product_charge.*`, only for product charge configuration reporting.
- `m_savings_account_charge.fee_on_month`.
- `m_savings_account_charge.fee_on_day`.
- `m_savings_account_charge.fee_interval`.
- `m_savings_account_charge.free_withdrawal_count`.
- `m_savings_account_charge.charge_reset_date`.
- `m_savings_account_charge.calculation_percentage`.
- `m_savings_account_charge.calculation_on_amount`.
- `m_savings_account_charge.settlement_priority`, only for settlement-priority reporting.
- `m_charge.fee_*` fields, only after recurring fee semantics are approved.
- `m_charge.min_cap` and `m_charge.max_cap`, only for advanced charge calculation reporting.
- `m_charge.payment_type_id`, only if payment/channel scope is approved.

Excluded from MVP output:

- `m_savings_account_charge.created_by`.
- `m_savings_account_charge.last_modified_by`.
- `m_savings_account_charge.created_on_utc`.
- `m_savings_account_charge.last_modified_on_utc`.
- `m_charge.income_or_liability_account_id` until accounting scope is approved.
- `m_charge.tax_group_id` until tax scope is approved.
- `m_charge.settlement_priority` unless settlement-priority reporting is approved.
