# Reporting Capabilities

This document defines the reporting capabilities that the AI Reporting Service is allowed to execute against the Fineract read-only database.

Capabilities are the runtime contract between user intent, authorization, approved SQL, and response formatting. The service must not execute arbitrary AI-generated SQL.

> **Scope commitment.** The service commits to supporting all reasonable admin reporting questions over the in-scope reporting-data areas (savings, client, organization, group/center foundations) — currently as a mix of `implemented` and `planned` capabilities. Deferred domains (loan, accounting, tax, audit, custom-datatables) are inside the product commitment but not yet activated. For the full implemented-vs-planned-vs-deferred-vs-out-of-scope picture, see [`docs/capability-coverage-matrix.md`](./capability-coverage-matrix.md). This document details the capability contract; the matrix is the scoreboard.
>
> Capability shortage today is a known-gap, not an intended design. Every reasonable admin decision-support question is expected to fit somewhere in the coverage matrix — as `implemented`, `planned`, `deferred`, or (with an explicit reason) `out_of_scope`.

## 1. Capability Rules

Every reporting capability the service commits to must eventually declare:

- Capability id.
- Status.
- User intent it supports.
- Required API key scope.
- Allowed tables and joins.
- Required parameters.
- Optional parameters.
- Default filters.
- Output mode.
- Allowed output fields.
- PII behavior.
- Office authorization behavior.
- Approved query file path.

Runtime rules:

- A user request must map to one approved capability or be rejected/clarified.
- API key `allowed_capabilities` must contain the capability id.
- API key `allowed_office_ids` must be enforced on every Fineract query.
- Date ranges and limits must be validated before query execution.
- Reversed transactions must be excluded by default unless the capability explicitly analyzes reversals.
- Response output must use only declared fields.
- Raw SQL must come from approved query files, not from AI output.

## 2. Capability Statuses

The runtime YAML enum uses the string values below (see `knowledge/capabilities/**/*.yaml`). The doc-facing term for `approved_mvp` is **enabled capability** — going forward, docs describe this as "currently implemented" or "enabled", not "MVP". The YAML enum value stays `approved_mvp` for backward compatibility with the runtime loader; renaming it is a Rust follow-up, not a doc task.

| Runtime YAML value | Doc-facing term | Meaning |
| --- | --- | --- |
| `approved_mvp` | **enabled / currently implemented** | Capability is loadable at runtime and matchable by the classifier. Legacy YAML enum name; do not rename. |
| `candidate` | **planned** (documented) | Documented as a next capability but not executable yet. Maps to `planned` in the coverage matrix. |
| `deferred` | **deferred** | Not executable until its data scope and business semantics are activated. Domain-level approval required. |
| `rejected` | **out-of-scope** | Explicitly unsupported; will never build. |

Every capability referenced in this document must either (a) exist in `knowledge/capabilities/**/*.yaml` with `status: approved_mvp` and correspond to an `implemented` cell in the coverage matrix, or (b) be listed under §11 Planned Capabilities with a target milestone and matching `planned` matrix cell.

## 3. Common Parameters

These parameters are shared across the currently implemented savings capabilities and are expected to remain the common shape for planned savings capabilities.

| Parameter | Type | Required | Rule |
| --- | --- | --- | --- |
| `from_date` | `date` | yes | Inclusive business date lower bound. |
| `to_date` | `date` | yes | Inclusive business date upper bound. |
| `office_ids` | `array<bigint>` | no | Must be subset of API key `allowed_office_ids`. If omitted, use all allowed offices. |
| `currency_code` | `string` | no | Optional exact currency filter. |
| `product_ids` | `array<bigint>` | no | Optional savings product filter. |
| `limit` | `integer` | top/list only | Must be bounded by service max limit. |

Default validation:

- `from_date <= to_date`.
- Date range must not exceed the configured maximum range for the capability.
- `office_ids` must not broaden the caller's office scope.
- `limit` must be greater than zero and less than or equal to the configured max limit.

## 4. Common Savings Joins

Approved table family:

- `m_savings_account_transaction`.
- `m_savings_account`.
- `m_savings_product`.
- `m_client`.
- `m_office`.
- `m_group`, only if group/center scope is enabled.

Default join rules:

- `m_savings_account_transaction.savings_account_id -> m_savings_account.id`.
- `m_savings_account.product_id -> m_savings_product.id`.
- Client-owned account office path: `m_savings_account.client_id -> m_client.id -> m_client.office_id`.
- Group-owned account office path: `m_savings_account.group_id -> m_group.id -> m_group.office_id`, only if group scope is enabled.
- Transaction office path: `m_savings_account_transaction.office_id -> m_office.id`.

Office authorization rule:

- Transaction `office_id` must be constrained to the caller's authorized offices.
- Account ownership office should also be validated where practical to prevent mismatched joins or data quality issues from broadening access.

## 5. Currently Implemented Capabilities

The capabilities detailed below have `status: approved_mvp` in `knowledge/capabilities/**/*.yaml` and are executable end-to-end. Each corresponds to an `implemented` cell in the coverage matrix. Additional executable capabilities are summarised in §6.

### 5.1 `savings_deposit_total`

Status: `approved_mvp`. Coverage matrix status: `implemented`.

Purpose:

- Answer total savings deposits for a date range and authorized office scope.

Supported examples:

- `What is the total deposit this month?`
- `Total deposits from January to September 2026.`
- `How much savings deposit did we receive today?`

Output mode:

- `total`.

Required API key capability:

- `savings_deposit_total`.

Required parameters:

- `from_date`.
- `to_date`.

Optional parameters:

- `office_ids`.
- `currency_code`.
- `product_ids`.

Allowed tables:

- `m_savings_account_transaction`.
- `m_savings_account`.
- `m_savings_product`.
- `m_client`.
- `m_office`.
- `m_group`, only if group/center scope is enabled.

Required transaction filters:

- `m_savings_account_transaction.transaction_type_enum = 1` for `DEPOSIT`.
- `m_savings_account_transaction.is_reversed = false`.
- `m_savings_account_transaction.transaction_date BETWEEN from_date AND to_date`.
- `m_savings_account_transaction.office_id` constrained to authorized office ids.

Allowed metrics:

- `total_deposit_amount = SUM(m_savings_account_transaction.amount)`.
- `deposit_count = COUNT(*)`.

Allowed dimensions:

- `currency_code`.
- `office_id` and office display name, if grouped or included in metadata.
- `product_id` and product name, if grouped or filtered.

Output fields:

- `from_date`.
- `to_date`.
- `currency_code`.
- `total_deposit_amount`.
- `deposit_count`.
- `office_scope` summary.

PII behavior:

- No client names, account numbers, external ids, payment references, or app user fields.
- Does not require `can_view_pii=true`.

Planned approved query file path:

- `queries/savings/deposit_total.sql`.

### 5.2 `savings_deposit_top_n`

Status: `approved_mvp`. Coverage matrix status: `implemented`.

Purpose:

- Return the largest savings deposit transactions for a date range and authorized office scope.

Supported examples:

- `Who made the largest deposit today?`
- `Show the largest deposits this month.`
- `Top 10 savings deposits this week.`

Output mode:

- `top_n`.

Required API key capability:

- `savings_deposit_top_n`.

Required parameters:

- `from_date`.
- `to_date`.
- `limit`.

Optional parameters:

- `office_ids`.
- `currency_code`.
- `product_ids`.

Allowed tables:

- `m_savings_account_transaction`.
- `m_savings_account`.
- `m_savings_product`.
- `m_client`.
- `m_office`.
- `m_group`, only if group/center scope is enabled.

Required transaction filters:

- `m_savings_account_transaction.transaction_type_enum = 1` for `DEPOSIT`.
- `m_savings_account_transaction.is_reversed = false`.
- `m_savings_account_transaction.transaction_date BETWEEN from_date AND to_date`.
- `m_savings_account_transaction.office_id` constrained to authorized office ids.
- `ORDER BY m_savings_account_transaction.amount DESC, m_savings_account_transaction.transaction_date DESC`.
- `LIMIT limit`.

Allowed output fields without PII:

- `transaction_id`.
- `transaction_date`.
- `amount`.
- `currency_code`.
- `office_id`.
- `office_name`.
- `product_id`.
- `product_name`.

Conditionally allowed output fields with `can_view_pii=true`:

- `client_id`.
- `client_display_name`.

Still excluded even with `can_view_pii=true`:

- Savings `account_no`.
- Savings `external_id`.
- Transaction `external_id`.
- Transaction `ref_no`.
- Payment detail references.
- App user audit fields.

Planned approved query file path:

- `queries/savings/deposit_top_n.sql`.

## 6. Additional Currently Implemented Savings Capabilities

The machine-readable catalog is authoritative. These enabled capabilities are executable today in addition to the two detailed deposit sections above.

| Capability | Query | Output mode | Required filters |
| --- | --- | --- | --- |
| `savings_withdrawal_total` | `queries/savings/withdrawal_total.sql` | `total` | `transaction_type_enum = 2`, `is_reversed = false`, authorized `office_id` |
| `savings_withdrawal_top_n` | `queries/savings/withdrawal_top_n.sql` | `top_n` | same withdrawal filters plus bounded `limit` |
| `savings_deposit_monthly_breakdown` | `queries/savings/deposit_monthly_breakdown.sql` | `monthly_breakdown` | deposit filters grouped by `date_trunc('month', transaction_date)` |
| `savings_deposit_monthly_top_n` | `queries/savings/deposit_monthly_top_n.sql` | `monthly_top_n` | deposit filters plus `ROW_NUMBER()` per month |
| `savings_withdrawal_monthly_breakdown` | `queries/savings/withdrawal_monthly_breakdown.sql` | `monthly_breakdown` | withdrawal filters grouped by `date_trunc('month', transaction_date)` |
| `savings_withdrawal_monthly_top_n` | `queries/savings/withdrawal_monthly_top_n.sql` | `monthly_top_n` | withdrawal filters plus `ROW_NUMBER()` per month |
| `savings_balance_summary` | `queries/savings/balance_summary.sql` | `summary` | active client-owned accounts scoped by authorized office |

PII rule: `top_n` and `monthly_top_n` return client identity fields and require `can_view_pii=true`. PII gating is orthogonal to capability status — a `planned` capability that returns PII must still declare its PII contract before being implemented.

## 7. Candidate Savings Capabilities

Detailed contract candidates that are not yet executable. See §11 for the full `planned` roster.

### 7.1 `savings_charge_outstanding_summary`

Runtime YAML status: `candidate`. Coverage matrix status: `planned` (target v0.2).

Purpose:

- Outstanding savings charges by office/product/charge definition.

Primary data:

- `m_savings_account_charge.amount_outstanding_derived`.
- `m_charge`.

Reason not yet enabled:

- The Savings Charges And Fees data area is currently `frontier`. Charge enum mapping (`m_charge.charge_type_enum`, `m_charge.charge_calculation_enum`) must be documented and the `amount_outstanding_derived` running-balance semantics reviewed before activation.

## 8. Deferred Capabilities

Deferred until their data scope and business semantics are approved:

- Loan reporting capabilities.
- Accounting/GL reporting capabilities.
- Tax reporting capabilities.
- Custom datatable reporting capabilities.
- User audit/security reporting capabilities.
- Payment reference/channel reporting capabilities.
- Hold/lien reporting capabilities.
- Overdraft reporting capabilities.
- Transfer reporting capabilities.

## 9. Unsupported Requests

The service must reject or clarify requests that ask for:

- Arbitrary SQL or database exploration.
- Full Fineract schema search.
- Report fields not declared by the selected capability.
- Raw account numbers, external ids, payment references, tokens, passwords, command JSON, or command results.
- Loan / accounting / tax / audit / custom-datatable results before those deferred domains are activated.
- Office scopes outside the API key's `allowed_office_ids`.

## 10. Implementation Notes

The implementation uses static approved SQL bindings in Rust, with each executable capability mapped through catalog metadata to one reviewed query file and output contract.

Current static approved SQL bindings:

```text
savings_deposit_total -> queries/savings/deposit_total.sql
savings_deposit_top_n -> queries/savings/deposit_top_n.sql
savings_withdrawal_total -> queries/savings/withdrawal_total.sql
savings_withdrawal_top_n -> queries/savings/withdrawal_top_n.sql
savings_deposit_monthly_breakdown -> queries/savings/deposit_monthly_breakdown.sql
savings_deposit_monthly_top_n -> queries/savings/deposit_monthly_top_n.sql
savings_withdrawal_monthly_breakdown -> queries/savings/withdrawal_monthly_breakdown.sql
savings_withdrawal_monthly_top_n -> queries/savings/withdrawal_monthly_top_n.sql
savings_balance_summary -> queries/savings/balance_summary.sql
```

The classifier should only emit capability ids that exist in the registry. The policy guard should validate capability scope, parameters, office scope, PII behavior, and limits before SQL execution.

Note on numbering: earlier drafts of this document appended a "Classification thresholds" section that duplicated the §9 heading. That content has been folded into `docs/ai-reporting-design.md` §8 Decision Policy where thresholds are documented alongside the outcome mapping. Section numbers in this file are contiguous 1–12.

## 11. Planned Capabilities

Everything below is `planned` in the coverage matrix. The classifier must map matching user intent to the `planned_unimplemented` outcome — it must not synthesize SQL, and it must not silently downgrade to a nearby `implemented` capability. Working ids are placeholders until the YAML lands; use `<planned: id>` when linking from other docs.

### 11.1 Savings — balance & lifecycle (Category A)

| Working id | Shape | Target | Notes |
| --- | --- | --- | --- |
| `<planned: savings_balance_per_office>` | Snapshot | v0.2 | Portfolio balance grouped by office. |
| `<planned: savings_balance_per_product>` | Snapshot | v0.2 | Portfolio balance grouped by product. |
| `<planned: savings_account_status_summary>` | Snapshot | v0.2 | Active vs closed counts. |
| `<planned: savings_accounts_opened_breakdown>` | Aggregate by bucket | v0.3 | Openings per bucket. |
| `<planned: savings_accounts_closed_breakdown>` | Aggregate by bucket | v0.3 | Closures per bucket. |
| `<planned: savings_dormant_accounts_summary>` | Snapshot / Top-N | v0.3 – v0.4 | Dormancy per operator-defined threshold. |
| `<planned: savings_accounts_by_staff>` | Snapshot | v0.3 | Books per savings officer. |

### 11.2 Savings — transactions (Category B)

| Working id | Shape | Target | Notes |
| --- | --- | --- | --- |
| `<planned: savings_deposit_breakdown>` | Aggregate over deposit with `bucket ∈ {day, week, month, N_days}` | v0.2 (day/week), v0.3 (N_days) | Bucket-parametric; see `docs/ai-reporting-design.md` §18.1. Consolidates the near-duplicate `_weekly_` / `_daily_` capabilities. |
| `<planned: savings_withdrawal_breakdown>` | Same | v0.2 – v0.3 | Same pattern. |
| `<planned: savings_activity_breakdown>` | Aggregate over deposit + withdrawal + interest + fee, bucket-parametric | v0.3 | Single capability replaces four near-duplicates per bucket. |
| `<planned: savings_activity_list>` | Individual transaction list, paginated | v0.4 | Requires new `list` output_mode with row-level PII gate. |
| `<planned: savings_deposit_top_n_by_office>` | Top-N per office | v0.3 | Ranking. |
| `<planned: savings_deposit_top_n_by_product>` | Top-N per product | v0.3 | Ranking. |
| `<planned: savings_reversed_transaction_summary>` | Aggregate | v0.3 | Reversal-rate tracking. |
| `<planned: savings_transaction_count_breakdown>` | Aggregate by bucket | v0.3 | Volume, not amount. |
| `<planned: savings_net_movement_breakdown>` | Aggregate by bucket | v0.3 | Deposit − withdrawal. |

### 11.3 Savings — interest & fees (Category C)

| Working id | Shape | Target | Notes |
| --- | --- | --- | --- |
| `<planned: savings_charge_outstanding_summary>` | Snapshot | v0.2 | Frontier data area activation required — see §7.1. |
| `<planned: savings_charge_outstanding_breakdown>` | Aggregate by bucket | v0.3 | Uses `savings_activity_breakdown` pattern. |
| `<planned: savings_charge_assessed_total>` | Aggregate total | v0.3 | Charge assessment revenue view. |
| `<planned: savings_charge_paid_total>` | Aggregate total | v0.3 | Collection view. |
| `<planned: savings_charge_waived_total>` | Aggregate total | v0.3 | Waivers audit view. |
| `<planned: savings_hold_balance_summary>` | Snapshot | v0.3 | Depends on hold-type enum mapping. |
| `<planned: savings_hold_release_history>` | Individual list | v0.4 | Individual holds released within period. |
| `<planned: savings_interest_posting_total>` | Aggregate total | v0.3 | Interest expense view. |
| `<planned: savings_interest_posting_breakdown>` | Aggregate by bucket | v0.3 | |
| `<planned: savings_interest_posting_per_account_top_n>` | Top-N | v0.4 | Requires row-level PII gate. |

### 11.4 Client foundation (Category D)

| Working id | Shape | Target | Notes |
| --- | --- | --- | --- |
| `<planned: client_status_summary>` | Snapshot | v0.2 | Active / closed / pending counts. |
| `<planned: clients_by_office>` | Snapshot | v0.2 | Foundation lookup. |
| `<planned: client_demographics_summary>` | Snapshot | v0.2 | Age band, gender, employment distributions. Aggregate only unless `can_view_pii=true`. |
| `<planned: client_onboarding_breakdown>` | Aggregate by bucket | v0.3 | New client counts per period. |
| `<planned: clients_with_no_active_account>` | Snapshot | v0.3 | Retention / outreach gap detection. |
| `<planned: clients_with_multiple_accounts>` | Snapshot / Top-N | v0.3 – v0.4 | Cross-sell view. |

### 11.5 Organization foundation (Category E)

| Working id | Shape | Target | Notes |
| --- | --- | --- | --- |
| `<planned: office_directory>` | Snapshot | v0.2 | Flat office list scoped to caller. |
| `<planned: office_hierarchy>` | Snapshot | v0.2 | Parent/child tree. |
| `<planned: staff_directory>` | Snapshot | v0.3 | Aggregate; PII-gated for row-level. |
| `<planned: staff_assignment_history_summary>` | Snapshot / Aggregate | v0.3 – v0.4 | |
| `<planned: office_performance_summary>` | Aggregate + Ranking | v0.3 | Composite portfolio + transactions per office. |

### 11.6 Group / center foundation (Category F, conditional)

Only landable in tenants where `group_center_foundation` is enabled.

| Working id | Shape | Target | Notes |
| --- | --- | --- | --- |
| `<planned: group_directory>` | Snapshot | v0.3 | |
| `<planned: group_membership_counts>` | Snapshot | v0.3 | |
| `<planned: group_savings_portfolio>` | Snapshot / Aggregate | v0.3 – v0.4 | |
| `<planned: group_activity_summary>` | Aggregate | v0.4 | |

### 11.7 Cross-cutting composite (Category H)

| Working id | Shape | Target | Notes |
| --- | --- | --- | --- |
| `<planned: composite_report>` (batch planner) | Composite multi-metric | v0.3 | Planner returns `Vec<ExecutionPlan>`; executor runs concurrently; formatter concatenates. See `docs/ai-reporting-design.md` §18.2. |
| `<planned: comparative_period_over_period>` | Comparative | v0.3 | Two same-shape plans + delta rendering. |
| `<planned: top_n_offices_over_metric>` | Ranking | v0.3 | Top-N offices by chosen metric. |
| `<planned: top_n_products_over_metric>` | Ranking | v0.3 | |
| `<planned: top_n_staff_over_metric>` | Ranking | v0.4 | Requires staff attribution join. |

Non-goals of §11: capabilities for deferred domains (loan, accounting, tax, audit, custom-datatables) are not listed above by working id. See Category G in the coverage matrix — activation is domain-level approval, not capability-level YAML.

## 12. Non-Goals

The service will never build the following, even when explicitly asked. These are permanent rejections, not deferrals.

- Arbitrary AI-generated SQL execution.
- Schema exploration endpoints (e.g. list all tables, describe columns) exposed to end users.
- Write operations of any kind against Fineract — no INSERT, UPDATE, DELETE, MERGE, DDL, or copy-out.
- Raw account numbers (`m_savings_account.account_no`), external ids, payment references (`ref_no`, `payment_details`), or any field marked `secret_never_expose` in the PII policy — even with `can_view_pii=true`.
- Cross-tenant reads. Office scope broader than the API key's `allowed_office_ids` is always rejected inside SQL bindings.
- Reproducing raw AI planner output, prompts, or internal command JSON in end-user responses.
- Model training or fine-tuning over Fineract data.
- Exports of full Fineract tables regardless of filter (bulk pull is a data-warehouse concern, not a chat concern).
- Streaming diffs of Fineract data or change-data-capture feeds.
- Answering questions about individual staff app-user accounts, credentials, session tokens, or audit records for staff — these are covered by the deferred `audit-users-operations` data area and are separately access-gated at the Fineract layer.

If a user request maps here, the classifier emits `Unsupported` with reason `hard_reject`. The response is a fixed sanitized template. No candidate SQL is produced, no fallback capability is tried.
