# Reporting Data: Loans

This document contains the detailed table and field scope for Reporting Data Scope section `4.1 Loans`.

Status: deferred, documented for future activation.

## 1. Scope

Loan reporting is deferred.

Reason:

- Loan reporting is important but more complex than savings.
- It introduces repayment schedules, disbursements, repayments, arrears, delinquency, charge-off, write-off, overpayment, rescheduling, tranches, capitalized income, buy-down fees, and product-specific rules.
- Loan metrics require stricter definitions than savings movement reports.

This document identifies the core loan table family and the fields likely needed when loan reporting is promoted into approved scope.

Verified Fineract table family:

- `m_loan`.
- `m_product_loan`.
- `m_loan_transaction`.
- `m_loan_repayment_schedule`.
- `m_loan_charge`.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml` plus later loan migrations.
- Fineract domain models: `Loan`, `LoanTransaction`.
- Local database `information_schema.columns` on `fineract_default`.

Activation rule:

- Do not include loan tables in executable reporting capabilities until this deferred scope is explicitly promoted.
- Before promotion, document loan status enum mapping, loan transaction type enum mapping, repayment schedule semantics, arrears/PAR definitions, and office authorization rules.

## 2. `m_loan`

Purpose:

- Canonical loan account table.
- Stores borrower ownership, product link, loan lifecycle, terms, status, principal, derived balances, arrears-related state, write-off/charge-off state, and advanced loan configuration.

### 2.1 Identity And Ownership Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary loan id. Use for joins. |
| `account_no` | `character varying` | no | Loan account number. Sensitive business identifier. |
| `external_id` | `character varying` | yes | External loan identifier. Sensitive business identifier. |
| `client_id` | `bigint` | yes | Client borrower id. Join to `m_client.id`. |
| `group_id` | `bigint` | yes | Group borrower id. Join to `m_group.id` if group scope is active. |
| `glim_id` | `bigint` | yes | Group loan individual monitoring id. Out of initial loan activation unless GLIM is approved. |
| `product_id` | `bigint` | yes | Loan product id. Join to `m_product_loan.id`. |
| `fund_id` | `bigint` | yes | Fund id. Fund table must be mapped before use. |
| `loan_officer_id` | `bigint` | yes | Loan officer id. Join to `m_staff.id`. |
| `loanpurpose_cv_id` | `integer` | yes | Loan purpose code value id. Needs code-value mapping. |
| `iban` | `character varying` | yes | IBAN/account identifier. Sensitive; exclude by default. |

Relationship rules:

- `m_loan.client_id -> m_client.id` for client-owned loans.
- `m_loan.group_id -> m_group.id` for group-owned loans.
- `m_loan.product_id -> m_product_loan.id`.
- `m_loan.loan_officer_id -> m_staff.id`.
- `m_loan.id -> m_loan_transaction.loan_id`.
- `m_loan.id -> m_loan_repayment_schedule.loan_id`.
- `m_loan.id -> m_loan_charge.loan_id`.

Authorization rules to define before activation:

- Client-owned loans should authorize through `m_client.office_id`.
- Group-owned loans should authorize through `m_group.office_id` when group scope is active.
- Transaction-level loan reports may also use `m_loan_transaction.office_id`, but account ownership authorization must still be enforced.

### 2.2 Status And Lifecycle Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `loan_status_id` | `smallint` | no | Loan status enum. Must be mapped before use. |
| `loan_sub_status_id` | `smallint` | yes | Loan sub-status enum. Must be mapped before use. |
| `loan_type_enum` | `smallint` | no | Loan type enum. Must be mapped before use. |
| `submittedon_date` | `date` | yes | Loan submission date. |
| `approvedon_date` | `date` | yes | Loan approval date. |
| `approvedon_userid` | `bigint` | yes | Approval user id. Audit field; exclude unless operational audit scope is enabled. |
| `expected_disbursedon_date` | `date` | yes | Expected disbursement date. |
| `disbursedon_date` | `date` | yes | Actual disbursement date. |
| `disbursedon_userid` | `bigint` | yes | Disbursement user id. Audit field; exclude unless operational audit scope is enabled. |
| `expected_firstrepaymenton_date` | `date` | yes | Expected first repayment date. |
| `expected_maturedon_date` | `date` | yes | Expected maturity date. |
| `maturedon_date` | `date` | yes | Actual maturity date. |
| `closedon_date` | `date` | yes | Loan closure date. |
| `closedon_userid` | `bigint` | yes | Closure user id. Audit field; exclude unless operational audit scope is enabled. |
| `rejectedon_date` | `date` | yes | Rejection date. |
| `rejectedon_userid` | `bigint` | yes | Rejection user id. Audit field; exclude unless operational audit scope is enabled. |
| `rescheduledon_date` | `date` | yes | Reschedule date. Rescheduling scope only. |
| `rescheduledon_userid` | `bigint` | yes | Reschedule user id. Audit field; exclude unless operational audit scope is enabled. |
| `withdrawnon_date` | `date` | yes | Withdrawal date. |
| `withdrawnon_userid` | `bigint` | yes | Withdrawal user id. Audit field; exclude unless operational audit scope is enabled. |
| `writtenoffon_date` | `date` | yes | Write-off date. Write-off scope only. |
| `overpaidon_date` | `date` | yes | Overpaid date. Overpayment scope only. |
| `is_charged_off` | `boolean` | no | Charge-off flag. Charge-off scope only. |
| `charged_off_on_date` | `date` | yes | Charge-off date. Charge-off scope only. |
| `charge_off_reason_cv_id` | `bigint` | yes | Charge-off reason code value id. Needs code-value mapping. |
| `charged_off_by_userid` | `bigint` | yes | Charge-off user id. Audit field; exclude unless operational audit scope is enabled. |

Reporting rules:

- Do not use loan status fields until Fineract loan lifecycle enum mapping is documented.
- Do not mix application, approval, disbursement, maturity, closure, and write-off dates without capability-specific definitions.

### 2.3 Terms And Product Configuration Snapshot Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `currency_code` | `character varying` | no | Loan currency code. |
| `currency_digits` | `smallint` | no | Currency decimal digits. |
| `currency_multiplesof` | `smallint` | yes | Currency multiples-of value. |
| `principal_amount_proposed` | `numeric` | no | Proposed principal amount. |
| `principal_amount` | `numeric` | no | Principal amount. |
| `approved_principal` | `numeric` | no | Approved principal. |
| `net_disbursal_amount` | `numeric` | no | Net disbursal amount. |
| `nominal_interest_rate_per_period` | `numeric` | yes | Interest rate per period. |
| `annual_nominal_interest_rate` | `numeric` | yes | Annual nominal interest rate. |
| `interest_method_enum` | `smallint` | no | Interest method enum. Needs mapping. |
| `interest_calculated_in_period_enum` | `smallint` | no | Interest calculated-in-period enum. Needs mapping. |
| `term_frequency` | `smallint` | no | Loan term frequency. Needs frequency enum context. |
| `term_period_frequency_enum` | `smallint` | no | Loan term period enum. Needs mapping. |
| `repay_every` | `smallint` | no | Repayment frequency interval. |
| `repayment_period_frequency_enum` | `smallint` | no | Repayment period enum. Needs mapping. |
| `number_of_repayments` | `smallint` | no | Number of repayments. |
| `amortization_method_enum` | `smallint` | no | Amortization method enum. Needs mapping. |
| `days_in_month_enum` | `smallint` | no | Days-in-month enum. Needs mapping. |
| `days_in_year_enum` | `smallint` | no | Days-in-year enum. Needs mapping. |

Reporting rules:

- These fields are useful for loan setup/product analysis, not enough by themselves to compute full schedules.
- Do not recompute loan balances or repayment schedules from these fields in MVP.

### 2.4 Derived Balance And Performance Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `principal_disbursed_derived` | `numeric` | no | Derived disbursed principal. |
| `principal_repaid_derived` | `numeric` | no | Derived repaid principal. |
| `principal_writtenoff_derived` | `numeric` | no | Derived written-off principal. |
| `principal_outstanding_derived` | `numeric` | no | Derived outstanding principal. Key future metric. |
| `interest_charged_derived` | `numeric` | no | Derived charged interest. |
| `interest_repaid_derived` | `numeric` | no | Derived repaid interest. |
| `interest_waived_derived` | `numeric` | no | Derived waived interest. |
| `interest_writtenoff_derived` | `numeric` | no | Derived written-off interest. |
| `interest_outstanding_derived` | `numeric` | no | Derived outstanding interest. |
| `fee_charges_charged_derived` | `numeric` | no | Derived charged fees. |
| `fee_charges_repaid_derived` | `numeric` | no | Derived repaid fees. |
| `fee_charges_waived_derived` | `numeric` | no | Derived waived fees. |
| `fee_charges_writtenoff_derived` | `numeric` | no | Derived written-off fees. |
| `fee_charges_outstanding_derived` | `numeric` | no | Derived outstanding fees. |
| `penalty_charges_charged_derived` | `numeric` | no | Derived charged penalties. |
| `penalty_charges_repaid_derived` | `numeric` | no | Derived repaid penalties. |
| `penalty_charges_waived_derived` | `numeric` | no | Derived waived penalties. |
| `penalty_charges_writtenoff_derived` | `numeric` | no | Derived written-off penalties. |
| `penalty_charges_outstanding_derived` | `numeric` | no | Derived outstanding penalties. |
| `total_expected_repayment_derived` | `numeric` | no | Derived expected repayment total. |
| `total_repayment_derived` | `numeric` | no | Derived actual repayment total. |
| `total_expected_costofloan_derived` | `numeric` | no | Derived expected cost of loan. |
| `total_costofloan_derived` | `numeric` | no | Derived actual cost of loan. |
| `total_waived_derived` | `numeric` | no | Derived total waived. |
| `total_writtenoff_derived` | `numeric` | no | Derived total written off. |
| `total_outstanding_derived` | `numeric` | no | Derived total outstanding. Key future metric. |
| `total_overpaid_derived` | `numeric` | yes | Derived overpaid amount. Overpayment scope only. |

Reporting rules:

- These derived fields are candidates for snapshot loan metrics.
- Date-bounded loan movement should use `m_loan_transaction`, not lifetime derived fields.
- PAR/arrears metrics require separate approved definitions before use.

### 2.5 Advanced Columns Deferred From Initial Loan Activation

Examples of advanced fields that must remain deferred until separately approved:

- `is_floating_interest_rate`.
- `interest_rate_differential`.
- `interest_recalculation_enabled`.
- `loan_transaction_strategy_id`.
- `loan_transaction_strategy_code`.
- `loan_transaction_strategy_name`.
- `is_topup`.
- `is_equal_amortization`.
- `enable_down_payment`.
- `enable_installment_level_delinquency`.
- `enable_accrual_activity_posting`.
- `enable_income_capitalization`.
- `enable_buy_down_fee`.
- `enable_auto_repayment_for_down_payment`.
- `loan_schedule_type`.
- `loan_schedule_processing_type`.
- `charge_off_behaviour`.
- `supported_interest_refund_types`.

## 3. `m_product_loan`

Purpose:

- Canonical loan product table.
- Provides product-level dimensions and default loan configuration.

Key verified columns for future activation:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary loan product id. |
| `short_name` | `character varying` | no | Product short name. |
| `name` | `character varying` | no | Product display name. |
| `description` | `character varying` | yes | Product description. Free text; use carefully. |
| `currency_code` | `character varying` | no | Product currency code. |
| `principal_amount` | `numeric` | yes | Default principal amount. |
| `min_principal_amount` | `numeric` | yes | Minimum principal amount. |
| `max_principal_amount` | `numeric` | yes | Maximum principal amount. |
| `nominal_interest_rate_per_period` | `numeric` | yes | Default interest rate per period. |
| `annual_nominal_interest_rate` | `numeric` | yes | Annual nominal interest rate. |
| `interest_method_enum` | `smallint` | no | Interest method enum. Needs mapping. |
| `repay_every` | `smallint` | no | Repayment interval. |
| `repayment_period_frequency_enum` | `smallint` | no | Repayment period enum. Needs mapping. |
| `number_of_repayments` | `smallint` | no | Default repayment count. |
| `amortization_method_enum` | `smallint` | no | Amortization method enum. Needs mapping. |
| `accounting_type` | `smallint` | no | Accounting type. Exclude until accounting scope is approved. |
| `external_id` | `character varying` | yes | External product id. Sensitive/internal. |
| `start_date` | `date` | yes | Product start date. |
| `close_date` | `date` | yes | Product close date. |
| `loan_schedule_type` | `character varying` | no | Schedule type. Needs mapping. |
| `loan_schedule_processing_type` | `character varying` | no | Schedule processing type. Needs mapping. |

Reporting rules:

- Use `id`, `name`, `short_name`, and `currency_code` as primary product dimensions.
- Do not expose accounting fields until accounting/GL scope is approved.
- Do not use advanced product configuration without capability-specific definitions.

## 4. `m_loan_transaction`

Purpose:

- Canonical loan transaction table.
- Stores loan disbursement, repayment, adjustment, chargeback, refund, reversal, and other loan movement rows.

Key verified columns for future activation:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary loan transaction id. |
| `loan_id` | `bigint` | no | Loan id. Join to `m_loan.id`. |
| `office_id` | `bigint` | no | Transaction office id. Join to `m_office.id`. |
| `payment_detail_id` | `bigint` | yes | Payment detail id. Join to `m_payment_detail.id` if payment/channel scope is approved. |
| `is_reversed` | `boolean` | no | Reversal flag. Exclude reversed rows by default. |
| `external_id` | `character varying` | yes | External transaction id. Sensitive/internal. |
| `transaction_type_enum` | `smallint` | no | Loan transaction type enum. Must be mapped before use. |
| `transaction_date` | `date` | no | Business transaction date. Primary date for movement reports. |
| `amount` | `numeric` | no | Transaction amount. |
| `principal_portion_derived` | `numeric` | yes | Principal portion. |
| `interest_portion_derived` | `numeric` | yes | Interest portion. |
| `fee_charges_portion_derived` | `numeric` | yes | Fee portion. |
| `penalty_charges_portion_derived` | `numeric` | yes | Penalty portion. |
| `overpayment_portion_derived` | `numeric` | yes | Overpayment portion. |
| `outstanding_loan_balance_derived` | `numeric` | yes | Outstanding loan balance after transaction. |
| `submitted_on_date` | `date` | no | Submission date. Operational reporting only. |
| `manually_adjusted_or_reversed` | `boolean` | yes | Manual adjustment/reversal marker. Operational reporting only. |
| `reversed_on_date` | `date` | yes | Reversal date. Reversal reporting only. |
| `classification_cv_id` | `bigint` | yes | Classification code value id. Needs mapping. |

Reporting rules:

- Exclude reversed loan transactions by default using `is_reversed = false`.
- Use `transaction_date` for business date filters.
- Do not use `transaction_type_enum` until loan transaction type mapping is documented.
- Payment detail joins are conditional on payment/channel scope.

## 5. `m_loan_repayment_schedule`

Purpose:

- Stores scheduled installments and completed/waived/written-off amounts by installment.
- Needed for due, overdue, arrears, installment, and PAR-style reporting.

Key verified columns for future activation:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary repayment schedule row id. |
| `loan_id` | `bigint` | no | Loan id. Join to `m_loan.id`. |
| `fromdate` | `date` | yes | Period start date. |
| `duedate` | `date` | no | Installment due date. Key field for due/overdue reporting. |
| `installment` | `smallint` | no | Installment number. |
| `principal_amount` | `numeric` | yes | Scheduled principal. |
| `principal_completed_derived` | `numeric` | yes | Completed principal. |
| `principal_writtenoff_derived` | `numeric` | yes | Written-off principal. |
| `interest_amount` | `numeric` | yes | Scheduled interest. |
| `interest_completed_derived` | `numeric` | yes | Completed interest. |
| `interest_writtenoff_derived` | `numeric` | yes | Written-off interest. |
| `interest_waived_derived` | `numeric` | yes | Waived interest. |
| `fee_charges_amount` | `numeric` | yes | Scheduled fees. |
| `fee_charges_completed_derived` | `numeric` | yes | Completed fees. |
| `fee_charges_writtenoff_derived` | `numeric` | yes | Written-off fees. |
| `fee_charges_waived_derived` | `numeric` | yes | Waived fees. |
| `penalty_charges_amount` | `numeric` | yes | Scheduled penalties. |
| `penalty_charges_completed_derived` | `numeric` | yes | Completed penalties. |
| `penalty_charges_writtenoff_derived` | `numeric` | yes | Written-off penalties. |
| `penalty_charges_waived_derived` | `numeric` | yes | Waived penalties. |
| `completed_derived` | `boolean` | no | Installment completed flag. |
| `obligations_met_on_date` | `date` | yes | Date installment obligations were met. |
| `total_paid_in_advance_derived` | `numeric` | yes | Paid-in-advance amount. |
| `total_paid_late_derived` | `numeric` | yes | Paid-late amount. |
| `is_down_payment` | `boolean` | no | Down-payment installment flag. Down-payment scope only. |
| `is_re_aged` | `boolean` | no | Re-aged installment flag. Re-aging scope only. |

Reporting rules:

- Use only after due/overdue/arrears definitions are approved.
- Do not calculate PAR metrics until PAR definition is explicitly documented.
- Do not include re-aging/down-payment semantics until those scopes are approved.

## 6. `m_loan_charge`

Purpose:

- Stores charges applied to loans.
- Included in loan deferred scope but separate from savings charge reporting.

Key verified columns for future activation:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary loan charge id. |
| `loan_id` | `bigint` | no | Loan id. Join to `m_loan.id`. |
| `charge_id` | `bigint` | no | Charge definition id. Join to `m_charge.id`. |
| `is_penalty` | `boolean` | no | Penalty flag. |
| `charge_time_enum` | `smallint` | no | Charge timing enum. Needs mapping. |
| `due_for_collection_as_of_date` | `date` | yes | Charge due/collection date. |
| `charge_calculation_enum` | `smallint` | no | Charge calculation enum. Needs mapping. |
| `charge_payment_mode_enum` | `smallint` | no | Charge payment mode enum. Needs mapping. |
| `amount` | `numeric` | no | Charge amount. |
| `amount_paid_derived` | `numeric` | yes | Paid amount. |
| `amount_waived_derived` | `numeric` | yes | Waived amount. |
| `amount_writtenoff_derived` | `numeric` | yes | Written-off amount. |
| `amount_outstanding_derived` | `numeric` | no | Outstanding amount. |
| `is_paid_derived` | `boolean` | no | Paid flag. |
| `waived` | `boolean` | no | Waived flag. |
| `is_active` | `boolean` | no | Active flag. |
| `external_id` | `character varying` | yes | External charge id. Sensitive/internal. |
| `tax_amount` | `numeric` | yes | Tax amount. Exclude until tax scope is approved. |

Reporting rules:

- Keep loan charges separate from savings charges.
- Exclude tax fields until tax reporting is approved.

## 7. Required Before Promoting Loans To Approved Scope

Before enabling loan reporting capabilities, document:

- Loan status enum mapping for `m_loan.loan_status_id`.
- Loan sub-status mapping for `m_loan.loan_sub_status_id`.
- Loan transaction type mapping for `m_loan_transaction.transaction_type_enum`.
- Loan type mapping for `m_loan.loan_type_enum`.
- Repayment schedule and overdue semantics.
- PAR definition and exact formula.
- Charge-off/write-off semantics.
- Overpayment semantics.
- Re-aging and rescheduling semantics.
- Whether group/GLIM loans are in scope.
- PII/business identifier policy for loan `account_no`, `external_id`, and `iban`.
- Office authorization rules for client-owned vs group-owned loans.

## 8. Initial Loan Activation Candidate

When loans are promoted from deferred to approved scope, start with a narrow subset:

- Loan account snapshot by office/product/status.
- Disbursement totals by date range.
- Repayment totals by date range.
- Outstanding principal/total outstanding snapshot.

Candidate tables for first activation:

- `m_loan`.
- `m_product_loan`.
- `m_loan_transaction`.

Keep out of first activation:

- PAR/arrears.
- Repayment schedule analytics.
- Charge-off/write-off analytics.
- Re-aging/rescheduling.
- Loan charges.
- Accounting/GL reconciliation.
- Tax.
