# Reporting Data: Savings Transactions

This document contains the detailed table and field scope for Reporting Data Scope section `3.5 Savings Transactions`.

Status: completed for initial review.

## 1. Scope

Savings Transactions is included in the MVP domain.

Purpose:

- Support transaction-level reporting for savings movement.
- Provide deposit, withdrawal, interest posting, fee, reversal, and balance movement context.
- Provide date-bounded reporting for savings movement.

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
- Created/submitted timestamps and audit user references, later if operational audit reporting is approved.

Verified Fineract table family:

- `m_savings_account_transaction`.
- `m_payment_detail`, later if payment/channel reporting is approved.
- `m_payment_type`, later if payment/channel reporting is approved.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml` plus later savings transaction migrations.
- Fineract domain model: `SavingsAccountTransaction`.
- Fineract enum: `SavingsAccountTransactionType`.
- Local database `information_schema.columns` on `fineract_default`.

## 2. `m_savings_account_transaction`

Purpose:

- Canonical savings account transaction table.
- Stores all monetary transaction movements against savings accounts.

### 2.1 Identity And Relationship Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary savings transaction id. Use for joins. |
| `savings_account_id` | `bigint` | no | Savings account id. Join to `m_savings_account.id`. |
| `office_id` | `bigint` | no | Transaction office id. Join to `m_office.id`; useful for office authorization and branch reporting. |
| `payment_detail_id` | `bigint` | yes | Payment detail id. Join to `m_payment_detail.id` if payment/channel scope is approved. |
| `external_id` | `character varying` | yes | External transaction identifier. Sensitive business identifier. |
| `ref_no` | `character varying` | yes | Transaction reference number. Sensitive business/payment reference. |
| `original_transaction_id` | `bigint` | yes | Original transaction id for reversal relationship. Join to `m_savings_account_transaction.id`. |
| `release_id_of_hold_amount` | `bigint` | yes | Related hold release transaction id. Hold/lien scope only. |
| `parent_hold_transaction_id` | `bigint` | yes | Parent hold transaction id. Hold/lien scope only. |

Primary relationship rules:

- `m_savings_account_transaction.savings_account_id -> m_savings_account.id`.
- `m_savings_account_transaction.office_id -> m_office.id`.
- `m_savings_account_transaction.payment_detail_id -> m_payment_detail.id`.
- `m_savings_account_transaction.original_transaction_id -> m_savings_account_transaction.id`.
- `m_savings_account_transaction.release_id_of_hold_amount -> m_savings_account_transaction.id` when hold/lien scope is enabled.
- `m_savings_account_transaction.parent_hold_transaction_id -> m_savings_account_transaction.id` when hold/lien scope is enabled.

Reporting rules:

- Use `id` as canonical transaction key.
- Use `office_id` as the direct transaction office dimension.
- For client/group authorization, also validate through account ownership path:
  - transaction -> savings account -> client -> office.
  - transaction -> savings account -> group -> office, if group scope is enabled.
- Do not expose `external_id` or `ref_no` by default.

### 2.2 Type, Date, Amount, And Balance Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `transaction_type_enum` | `smallint` | no | Savings transaction type enum. See mapping below. |
| `transaction_date` | `date` | no | Business transaction date. Primary date for business reporting. |
| `submitted_on_date` | `date` | no | Date transaction was submitted. Useful for operational timing, not default movement date. |
| `value_date` | `date` | yes | Value date. Use only if value-date reporting is approved. |
| `amount` | `numeric` | no | Transaction amount. Key metric. |
| `overdraft_amount_derived` | `numeric` | yes | Derived overdraft amount. Overdraft scope only. |
| `balance_end_date_derived` | `date` | yes | Balance end date for derived balance intervals. Advanced interest/balance scope. |
| `balance_number_of_days_derived` | `integer` | yes | Number of days for derived balance interval. Advanced interest/balance scope. |
| `running_balance_derived` | `numeric` | yes | Running balance after transaction. Useful for transaction ledger views. |
| `cumulative_balance_derived` | `numeric` | yes | Cumulative balance used by interest calculations. Advanced scope. |

Reporting rules:

- Use `transaction_date` for business date filters by default.
- Use `submitted_on_date` only for operational submission reporting.
- Use `amount` with `transaction_type_enum` to determine semantic direction.
- Do not infer debit/credit direction without the transaction type mapping.
- Exclude reversed transactions by default using `is_reversed = false`, unless the capability explicitly analyzes reversals.

### 2.3 Reversal, Manual, Loan, Hold, And Lien Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `is_reversed` | `boolean` | no | Indicates this transaction has been reversed. Exclude by default. |
| `is_reversal` | `boolean` | no | Indicates this row is a reversal transaction. Use only in reversal analysis. |
| `is_manual` | `boolean` | yes | Manual transaction flag. Operational reporting only. |
| `is_loan_disbursement` | `boolean` | yes | Indicates transaction related to loan disbursement. Loan-linked scope only. |
| `reason_for_block` | `character varying` | yes | Block/hold reason text. Sensitive free text; exclude from MVP. |
| `is_lien_transaction` | `boolean` | no | Lien transaction flag. Lien scope only. |
| `hold_type` | `smallint` | yes | Hold type enum. Hold/lien scope only; needs enum mapping. |
| `hold_status` | `smallint` | yes | Hold status enum. Hold/lien scope only; needs enum mapping. |

Reporting rules:

- Default transaction reports must filter `is_reversed = false`.
- For reversal-specific reports, include both `is_reversed` and `is_reversal` semantics explicitly.
- Do not expose `reason_for_block` by default.
- Exclude hold/lien fields until hold/lien scope is approved.

### 2.4 Audit Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `created_date` | `timestamp without time zone` | yes | Legacy creation timestamp. Exclude from MVP unless operational audit scope is enabled. |
| `created_by` | `bigint` | no | Audit creator user id. Exclude from MVP unless operational audit scope is enabled. |
| `last_modified_by` | `bigint` | no | Audit updater user id. Exclude from MVP unless operational audit scope is enabled. |
| `created_on_utc` | `timestamp with time zone` | no | Audit creation timestamp. Exclude from MVP unless operational audit scope is enabled. |
| `last_modified_on_utc` | `timestamp with time zone` | no | Audit update timestamp. Exclude from MVP unless operational audit scope is enabled. |

Reporting rules:

- Do not use audit fields for business date filtering.
- Use only if operational audit reporting is explicitly approved.

## 3. `transaction_type_enum` Mapping

Verified from Fineract `SavingsAccountTransactionType` enum.

| Value | Name | Entry type | MVP use |
| --- | --- | --- | --- |
| `0` | `INVALID` | none | Exclude. |
| `1` | `DEPOSIT` | credit | Included for deposit reporting. |
| `2` | `WITHDRAWAL` | debit | Included for withdrawal reporting. |
| `3` | `INTEREST_POSTING` | credit | Included for interest posted reporting. |
| `4` | `WITHDRAWAL_FEE` | debit | Secondary fee scope. |
| `5` | `ANNUAL_FEE` | debit | Secondary fee scope. |
| `6` | `WAIVE_CHARGES` | none | Secondary charge/fee scope. |
| `7` | `PAY_CHARGE` | debit | Secondary charge/fee scope. |
| `8` | `DIVIDEND_PAYOUT` | credit | Deferred unless dividend reporting is approved. |
| `10` | `ACCRUAL` | none | Deferred accrual scope. |
| `12` | `INITIATE_TRANSFER` | none | Deferred transfer scope. |
| `13` | `APPROVE_TRANSFER` | none | Deferred transfer scope. |
| `14` | `WITHDRAW_TRANSFER` | none | Deferred transfer scope. |
| `15` | `REJECT_TRANSFER` | none | Deferred transfer scope. |
| `16` | `WRITTEN_OFF` | none | Deferred write-off scope. |
| `17` | `OVERDRAFT_INTEREST` | debit | Deferred overdraft scope. |
| `18` | `WITHHOLD_TAX` | debit | Deferred tax scope. |
| `19` | `ESCHEAT` | debit | Deferred dormant/escheat scope. |
| `20` | `AMOUNT_HOLD` | debit | Deferred hold/lien scope. |
| `21` | `AMOUNT_RELEASE` | credit | Deferred hold/lien scope. |

MVP reporting rules:

- Include `DEPOSIT`, `WITHDRAWAL`, and `INTEREST_POSTING` initially.
- Include `WITHDRAWAL_FEE`, `ANNUAL_FEE`, `WAIVE_CHARGES`, and `PAY_CHARGE` only after Savings Charges And Fees scope is approved.
- Exclude tax, overdraft, hold/lien, transfer, accrual, write-off, dividend, and escheat types until explicitly approved.

## 4. `m_payment_detail`

Purpose:

- Stores payment reference details linked from transactions.
- Useful for payment/channel reporting, but contains sensitive references.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary payment detail id. |
| `payment_type_id` | `integer` | yes | Payment type id. Join to `m_payment_type.id`. |
| `account_number` | `character varying` | yes | Payment account number. Sensitive; exclude by default. |
| `check_number` | `character varying` | yes | Check number. Sensitive; exclude by default. |
| `receipt_number` | `character varying` | yes | Receipt number. Sensitive; exclude by default. |
| `bank_number` | `character varying` | yes | Bank number/reference. Sensitive; exclude by default. |
| `routing_code` | `character varying` | yes | Routing code. Sensitive; exclude by default. |

Scope rule:

- Exclude from MVP output unless payment/channel reporting is approved.
- If included later, expose only `payment_type_id` and payment type display by default.
- Do not expose account/check/receipt/bank/routing references without explicit approval.

## 5. `m_payment_type`

Purpose:

- Defines payment types/channels used by payment details.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `integer` | no | Primary payment type id. |
| `value` | `character varying` | yes | Payment type display value. |
| `description` | `character varying` | yes | Payment type description. |
| `is_cash_payment` | `boolean` | yes | Indicates cash payment type. |
| `order_position` | `integer` | no | Ordering field. Not relevant for reporting. |
| `code_name` | `character varying` | yes | Payment type code name. |
| `is_system_defined` | `boolean` | no | System-defined flag. |

Scope rule:

- Include only if payment/channel reporting is approved.
- `value`, `code_name`, and `is_cash_payment` are the useful reporting fields.

## 6. MVP Inclusion Decision

Included immediately:

- `m_savings_account_transaction.id`.
- `m_savings_account_transaction.savings_account_id`.
- `m_savings_account_transaction.office_id`.
- `m_savings_account_transaction.transaction_type_enum` with approved mapping.
- `m_savings_account_transaction.is_reversed` for default exclusion filter.
- `m_savings_account_transaction.transaction_date`.
- `m_savings_account_transaction.submitted_on_date`.
- `m_savings_account_transaction.amount`.
- `m_savings_account_transaction.running_balance_derived`.
- `m_savings_account_transaction.external_id`, only for internal trace/debug and not default output.

Conditionally included:

- `m_savings_account_transaction.payment_detail_id`, only if payment/channel reporting is approved.
- `m_savings_account_transaction.ref_no`, only with explicit sensitive reference approval.
- `m_savings_account_transaction.original_transaction_id`, only for reversal reporting.
- `m_savings_account_transaction.is_reversal`, only for reversal reporting.
- `m_savings_account_transaction.is_manual`, only for operational reporting.
- `m_payment_detail.payment_type_id`, only if payment/channel reporting is approved.
- `m_payment_type.value`, only if payment/channel reporting is approved.
- `m_payment_type.code_name`, only if payment/channel reporting is approved.
- `m_payment_type.is_cash_payment`, only if payment/channel reporting is approved.

Excluded from MVP output:

- `m_savings_account_transaction.reason_for_block`.
- `m_savings_account_transaction.overdraft_amount_derived` until overdraft scope is approved.
- `m_savings_account_transaction.cumulative_balance_derived` until advanced balance/interest scope is approved.
- `m_savings_account_transaction.balance_end_date_derived` until advanced balance/interest scope is approved.
- `m_savings_account_transaction.balance_number_of_days_derived` until advanced balance/interest scope is approved.
- `m_savings_account_transaction.release_id_of_hold_amount` until hold/lien scope is approved.
- `m_savings_account_transaction.is_lien_transaction` until hold/lien scope is approved.
- `m_savings_account_transaction.hold_type` until hold/lien scope is approved.
- `m_savings_account_transaction.hold_status` until hold/lien scope is approved.
- `m_savings_account_transaction.parent_hold_transaction_id` until hold/lien scope is approved.
- `m_savings_account_transaction.is_loan_disbursement` until loan-linked reporting is approved.
- `m_savings_account_transaction.value_date` until value-date reporting is approved.
- All `created_by`, `last_modified_by`, `created_on_utc`, `last_modified_on_utc`, and `created_date` audit fields unless operational audit scope is enabled.
- `m_payment_detail.account_number`.
- `m_payment_detail.check_number`.
- `m_payment_detail.receipt_number`.
- `m_payment_detail.bank_number`.
- `m_payment_detail.routing_code`.
