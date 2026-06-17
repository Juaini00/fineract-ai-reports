# Reporting Data: Accounting And General Ledger

This document contains the detailed table and field scope for Reporting Data Scope section `4.2 Accounting And General Ledger`.

Status: deferred, documented for future activation.

## 1. Scope

Accounting and General Ledger reporting is deferred.

Reason:

- Accounting reports are financially sensitive.
- GL reports need stricter reconciliation rules and explicit business definitions.
- Journal entries can link to loans, savings, clients, shares, payment details, and product mappings.
- Trial balance and reconciliation reports must not be inferred casually from raw journal rows.

Verified Fineract table family:

- `acc_gl_account`.
- `acc_gl_journal_entry`.
- `acc_accounting_rule`.
- `acc_product_mapping`.
- `acc_gl_closure`.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml` plus later accounting migrations.
- Fineract enums: `JournalEntryType`, `GLAccountType`.
- Local database `information_schema.columns` on `fineract_default`.

Activation rule:

- Do not include accounting or GL tables in executable reporting capabilities until this deferred scope is explicitly promoted.
- Before promotion, document trial balance definitions, debit/credit sign conventions, reversal handling, product mapping semantics, and office/closure period behavior.

## 2. `acc_gl_account`

Purpose:

- Canonical GL account definition table.
- Stores chart-of-accounts structure, GL code, classification, hierarchy, and whether manual journal entries are allowed.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary GL account id. Use for joins. |
| `name` | `character varying` | no | GL account display name. |
| `parent_id` | `bigint` | yes | Parent GL account id. Join to `acc_gl_account.id`. |
| `hierarchy` | `character varying` | yes | Materialized GL hierarchy path. |
| `gl_code` | `character varying` | no | GL account code. Sensitive/internal financial identifier. |
| `disabled` | `boolean` | no | Disabled flag. |
| `manual_journal_entries_allowed` | `boolean` | no | Whether manual journal entries are allowed. |
| `account_usage` | `smallint` | no | Account usage enum. Needs mapping before use. |
| `classification_enum` | `smallint` | no | GL classification enum. Mapping below. |
| `tag_id` | `integer` | yes | Tag/code value id. Needs mapping before display. |
| `description` | `character varying` | yes | GL account description. Free text; use carefully. |

Primary relationship rules:

- `acc_gl_account.parent_id -> acc_gl_account.id` for chart-of-accounts hierarchy.
- `acc_gl_journal_entry.account_id -> acc_gl_account.id`.
- `acc_product_mapping.gl_account_id -> acc_gl_account.id`.

Reporting rules:

- Use `id` as canonical GL account key.
- Use `name`, `gl_code`, and `classification_enum` for GL account display only after accounting scope is approved.
- Filter `disabled = false` for active account lists unless disabled accounts are explicitly requested.
- Do not expose GL account details to non-accounting capabilities by default.

## 3. `classification_enum` Mapping

Verified from Fineract `GLAccountType` enum.

| Value | Name | Meaning |
| --- | --- | --- |
| `1` | `ASSET` | Asset account. |
| `2` | `LIABILITY` | Liability account. |
| `3` | `EQUITY` | Equity account. |
| `4` | `INCOME` | Income account. |
| `5` | `EXPENSE` | Expense account. |

Reporting rule:

- Do not calculate financial statement totals until sign convention and closing rules are approved.

## 4. `acc_gl_journal_entry`

Purpose:

- Canonical journal entry table.
- Stores debit/credit rows, amount, office, account, entity links, transaction links, running balances, reversal state, and audit fields.

### 4.1 Identity And Relationship Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary journal entry id. |
| `account_id` | `bigint` | no | GL account id. Join to `acc_gl_account.id`. |
| `office_id` | `bigint` | no | Office id. Join to `m_office.id`; required for office-scoped GL reports. |
| `reversal_id` | `bigint` | yes | Reversal journal entry id. Join to `acc_gl_journal_entry.id`. |
| `transaction_id` | `character varying` | no | Fineract transaction group/reference id. Sensitive/internal. |
| `loan_transaction_id` | `bigint` | yes | Loan transaction id. Join to `m_loan_transaction.id` when loan scope is active. |
| `savings_transaction_id` | `bigint` | yes | Savings transaction id. Join to `m_savings_account_transaction.id` when savings GL reconciliation is approved. |
| `client_transaction_id` | `bigint` | yes | Client transaction id. Join to `m_client_transaction.id` if client transaction scope is approved. |
| `payment_details_id` | `bigint` | yes | Payment detail id. Join to `m_payment_detail.id` if payment/channel scope is approved. |
| `share_transaction_id` | `bigint` | yes | Share transaction id. Share scope only. |
| `entity_type_enum` | `smallint` | yes | Entity type enum. Needs mapping before use. |
| `entity_id` | `bigint` | yes | Entity id matching `entity_type_enum`. Needs mapping before use. |

Relationship rules:

- `acc_gl_journal_entry.account_id -> acc_gl_account.id`.
- `acc_gl_journal_entry.office_id -> m_office.id`.
- `acc_gl_journal_entry.reversal_id -> acc_gl_journal_entry.id`.
- `acc_gl_journal_entry.savings_transaction_id -> m_savings_account_transaction.id` only after savings GL reconciliation is approved.
- `acc_gl_journal_entry.loan_transaction_id -> m_loan_transaction.id` only after loan scope is active.

### 4.2 Amount, Date, Type, And Balance Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `currency_code` | `character varying` | no | Journal currency code. |
| `reversed` | `boolean` | no | Reversal flag. Exclude reversed rows by default unless reversal reporting is approved. |
| `ref_num` | `character varying` | yes | Reference number. Sensitive/internal. |
| `manual_entry` | `boolean` | no | Manual journal entry flag. Operational/accounting reporting only. |
| `entry_date` | `date` | no | Journal entry date. Primary accounting date. |
| `transaction_date` | `date` | yes | Transaction date. Needs accounting semantics before use. |
| `submitted_on_date` | `date` | no | Submission date. Operational reporting only. |
| `type_enum` | `smallint` | no | Journal entry type: credit/debit. Mapping below. |
| `amount` | `numeric` | no | Journal amount. |
| `description` | `character varying` | yes | Journal description. Free text; sensitive/internal. |
| `is_running_balance_calculated` | `boolean` | no | Running balance calculation flag. |
| `office_running_balance` | `numeric` | no | Office-level running balance. Use only after balance semantics are approved. |
| `organization_running_balance` | `numeric` | no | Organization-level running balance. Use only after balance semantics are approved. |

Reporting rules:

- Use `entry_date` as default accounting date.
- Exclude `reversed = true` by default.
- Do not infer net signs from `amount` alone; always use `type_enum` and GL classification rules.
- Do not expose `description`, `ref_num`, or `transaction_id` by default.

### 4.3 Audit Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `created_by` | `bigint` | no | Audit creator user id. Exclude unless operational audit scope is enabled. |
| `last_modified_by` | `bigint` | no | Audit updater user id. Exclude unless operational audit scope is enabled. |
| `created_date` | `timestamp without time zone` | yes | Legacy audit timestamp. Exclude unless operational audit scope is enabled. |
| `lastmodified_date` | `timestamp without time zone` | yes | Legacy audit timestamp. Exclude unless operational audit scope is enabled. |
| `created_on_utc` | `timestamp with time zone` | no | Audit creation timestamp. Exclude unless operational audit scope is enabled. |
| `last_modified_on_utc` | `timestamp with time zone` | no | Audit update timestamp. Exclude unless operational audit scope is enabled. |

## 5. `type_enum` Mapping

Verified from Fineract `JournalEntryType` enum.

| Value | Name | Meaning |
| --- | --- | --- |
| `1` | `CREDIT` | Credit journal entry. |
| `2` | `DEBIT` | Debit journal entry. |

Reporting rule:

- Debit/credit sign must be defined per report. Do not assume `credit = positive` or `debit = negative` globally without an approved accounting convention.

## 6. `acc_product_mapping`

Purpose:

- Maps products, charges, payment types, and financial account types to GL accounts.
- Used for product-to-GL accounting configuration, not direct transaction reporting.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary product mapping id. |
| `gl_account_id` | `bigint` | yes | GL account id. Join to `acc_gl_account.id`. |
| `product_id` | `bigint` | yes | Product id. Product table depends on `product_type`. |
| `product_type` | `smallint` | yes | Product type enum. Needs mapping before use. |
| `payment_type` | `integer` | yes | Payment type id. Join to `m_payment_type.id` if payment/channel scope is approved. |
| `charge_id` | `bigint` | yes | Charge definition id. Join to `m_charge.id`. |
| `financial_account_type` | `smallint` | yes | Financial account type enum. Needs mapping before use. |
| `charge_off_reason_id` | `integer` | yes | Charge-off reason id. Charge-off scope only. |
| `capitalized_income_classification_id` | `integer` | yes | Capitalized income classification id. Advanced loan scope only. |
| `buydown_fee_classification_id` | `integer` | yes | Buy-down fee classification id. Advanced loan scope only. |
| `write_off_reason_id` | `bigint` | yes | Write-off reason id. Write-off scope only. |

Reporting rules:

- Use only for accounting configuration reports after product/accounting mapping semantics are approved.
- Do not use `product_id` without `product_type` mapping.
- Do not use advanced classification ids until related loan/accounting scopes are approved.

## 7. `acc_accounting_rule`

Purpose:

- Stores named accounting rules with debit and credit GL accounts.
- Used for accounting configuration, not transaction balances.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary accounting rule id. |
| `name` | `character varying` | yes | Accounting rule name. |
| `office_id` | `bigint` | yes | Office id. Join to `m_office.id`. |
| `debit_account_id` | `bigint` | yes | Debit GL account id. Join to `acc_gl_account.id`. |
| `allow_multiple_debits` | `boolean` | no | Multiple debits flag. |
| `credit_account_id` | `bigint` | yes | Credit GL account id. Join to `acc_gl_account.id`. |
| `allow_multiple_credits` | `boolean` | no | Multiple credits flag. |
| `description` | `character varying` | yes | Rule description. Free text. |
| `system_defined` | `boolean` | no | System-defined flag. |

Reporting rules:

- Use only for accounting configuration reports.
- Do not use for journal totals or trial balance.

## 8. `acc_gl_closure`

Purpose:

- Stores GL closure dates by office.
- Needed for period/closing behavior in accounting reports.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary GL closure id. |
| `office_id` | `bigint` | no | Office id. Join to `m_office.id`. |
| `closing_date` | `date` | no | GL closure date. |
| `is_deleted` | `boolean` | no | Deleted flag. Exclude deleted closures by default. |
| `createdby_id` | `bigint` | yes | Audit creator user id. Exclude unless operational audit scope is enabled. |
| `lastmodifiedby_id` | `bigint` | yes | Audit updater user id. Exclude unless operational audit scope is enabled. |
| `created_date` | `timestamp without time zone` | yes | Audit creation timestamp. Exclude unless operational audit scope is enabled. |
| `lastmodified_date` | `timestamp without time zone` | yes | Audit update timestamp. Exclude unless operational audit scope is enabled. |
| `comments` | `character varying` | yes | Closure comments. Free text; exclude by default. |

Reporting rules:

- Use only when accounting period/closure behavior is part of approved report definition.
- Filter `is_deleted = false` by default.
- Do not expose `comments` by default.

## 9. Required Before Promoting Accounting/GL To Approved Scope

Before enabling Accounting/GL reporting capabilities, document:

- Trial balance definition and formula.
- Debit/credit sign convention per account classification.
- Reversal handling for `acc_gl_journal_entry.reversed` and `reversal_id`.
- Which date field is used per report: `entry_date`, `transaction_date`, or `submitted_on_date`.
- Office hierarchy behavior for GL rollups.
- GL closure/period handling.
- Product mapping enum values for `acc_product_mapping.product_type`.
- Financial account type enum values for `acc_product_mapping.financial_account_type`.
- Entity type enum values for `acc_gl_journal_entry.entity_type_enum`.
- Whether journal descriptions/reference numbers may be exposed.
- Reconciliation rules between savings/loan transactions and GL journal entries.

## 10. Initial Accounting Activation Candidate

When Accounting/GL is promoted from deferred to approved scope, start narrowly:

- Chart of accounts listing by classification.
- Journal entry totals by date range, office, GL account, and debit/credit type.
- Product-to-GL mapping lookup.

Candidate tables for first activation:

- `acc_gl_account`.
- `acc_gl_journal_entry`.
- `acc_product_mapping`.

Keep out of first activation:

- Trial balance.
- Full financial statements.
- Savings/loan reconciliation.
- GL closure period enforcement.
- Manual journal audit analysis.
- Product mapping for advanced loan classifications.
