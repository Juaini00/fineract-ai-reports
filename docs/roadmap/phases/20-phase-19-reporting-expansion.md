# Implementation Steps: Phase 19: Reporting Expansion

Source: `docs-old/implementation-steps.md`

## Phase 19: Reporting Expansion

Goal: add more reporting capabilities after MVP.

Current status:

```text
PARTIALLY DONE (savings matrix + organization/client foundation summaries)

Slice 1 — withdrawal capabilities:
savings_withdrawal_total + savings_withdrawal_top_n capability + query YAML.
queries/savings/withdrawal_total.sql + withdrawal_top_n.sql (mirror deposit, transaction_type_enum=2).
savings.withdrawal_amount metric flipped to approved_mvp; savings.withdrawal_count added.

Slice 2 — monthly breakdown:
savings_deposit_monthly_breakdown capability + query YAML.
queries/savings/deposit_monthly_breakdown.sql (GROUP BY date_trunc('month', transaction_date)).
OUTPUT_MODES extended with "monthly_breakdown".
Generic catalog-driven formatter renders declared output fields and labels from response catalog.
Executor resolves SQL from QueryKnowledge.sql_file under queries/; no query-id match arms are required.

Routing: vector retrieval picks the right capability via embedding distance (no classifier
change needed). classify_retrieved_capability is generic on output_mode — top_n adds limit,
total and monthly_breakdown only need from_date/to_date. PII gate in planner is derived from
the selected query output_fields sensitivity, not output_mode naming.

Local savings keyword classifier was removed; runtime capability selection comes from vector/catalog retrieval plus approved clarification options.

Slice 3 — date-range parser upgrade (classifier.rs::date_range):
Added: yesterday/kemarin, this year / tahun ini / ytd / year-to-date, last year / tahun lalu,
last month / bulan lalu, last week / minggu lalu, relative counts ("last 7 days", "past 30 days",
"3 months ago", "3 bulan terakhir", "5 hari lalu"), bare year ("deposits in 2026"), and
month-range with default-current-year ("from January to September" → 2026-01-01 .. 2026-09-30).
date_range now lowercases internally so callers don't have to.
13 new unit tests cover each pattern, including January wraparound for "last month".

Slice 4 — monthly top-N capability:
savings_deposit_monthly_top_n capability + query YAML.
queries/savings/deposit_monthly_top_n.sql uses a CTE + ROW_NUMBER() OVER (PARTITION BY month
ORDER BY amount DESC) to pick top-N per month.
OUTPUT_MODES extended with "monthly_top_n".
Validator: SQL safety check now accepts queries that start with WITH (CTE) in addition to SELECT.
Validator: limit-bound check now accepts ROW_NUMBER() / RANK() as alternative to trailing LIMIT.
Classifier classify_retrieved_capability now treats any output_mode ending in "top_n" as the
top_n shape (adds `limit` param); monthly_top_n default limit is 1, atomic top_n stays at 10.
Planner PII gate checks selected query output field sensitivity, so monthly_top_n requires can_view_pii when client identity is included.
Generic formatter renders monthly_top_n rows from output contract fields.

Slice 5 — snapshot balance summary:
savings_balance_summary capability + query YAML.
queries/savings/balance_summary.sql aggregates m_savings_account.account_balance_derived over
active client-owned accounts, filtered by m_client.office_id ∈ allowed_office_ids.
OUTPUT_MODES extended with "summary".
Validator: approved capability with output_mode == "summary" may declare empty required_parameters
(no time/limit/etc. user inputs needed; office scope is implicit from API key).
Classifier classify_retrieved_capability skips date_range for output_mode == "summary".
savings.account_balance metric flipped to approved_mvp.
Generic formatter renders the summary from output contract fields and response labels.

Withdrawal monthly mirrors:
savings_withdrawal_monthly_breakdown capability + query YAML + SQL file + formatter support.
savings_withdrawal_monthly_top_n capability + query YAML + SQL file + formatter support.
queries/savings/withdrawal_monthly_breakdown.sql and withdrawal_monthly_top_n.sql mirror deposit monthly slices with transaction_type_enum=2.
Retrieval classification now maps query source rows back to owning capability ids before planning.
Postman-derived runtime matrix passed all 9 approved savings capabilities on 2026-07-02.

Organization/client foundation summaries:
organization_office_summary capability + query YAML + queries/organization/office_summary.sql.
client_lifecycle_summary capability + query YAML + queries/client/lifecycle_summary.sql.
Metrics added: organization.office_count, organization.active_staff_count, client.lifecycle_count.
Both SQL files were prepared/executed against FINERACT_DATABASE_URL locally and return non-PII aggregate output only.

Still pending:
group-owned savings balance summary (requires promoting group_center_foundation out of conditional).
loan_* and accounting_* capabilities — blocked until those domains move out of deferred.
```

Next capabilities (in priority order):

```text
loan_disbursement_total (requires loan domain promotion)
loan_repayment_total
```
