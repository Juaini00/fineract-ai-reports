# Capability Coverage Matrix: Milestone map

Source: `docs-old/capability-coverage-matrix.md`

## Milestone map

| Milestone | Focus | Representative rows moving to `implemented` |
| --- | --- | --- |
| **v0.1 (current)** | Savings deposits + withdrawals aggregate/breakdown/top-N, portfolio balance snapshot | Nine `implemented` cells across Category A + B. |
| **v0.2** | Bucket-parametric breakdowns (week/day), foundation snapshots, first frontier activation | `savings_deposit_breakdown`, `savings_withdrawal_breakdown` (bucket=day/week), `savings_charge_outstanding_summary`, `client_status_summary`, `client_demographics_summary`, `office_directory`, `office_hierarchy`, balance per office / per product. |
| **v0.3** | Composite planner, comparative, ranking, per-bucket top-N, activity breakdown, hold snapshot | Composite `ExecutionPlanBatch`, comparative period-over-period, `savings_activity_breakdown`, `savings_hold_balance_summary`, dormant accounts, office performance, client onboarding breakdown. |
| **v0.4** | Individual-list output_mode, quarter bucket, cross-domain composites, interest & charge detail | `savings_activity_list` with `list` output_mode + row-level PII gate, quarterly and N-day buckets, staff and group activity, interest posting detail. |
| **v1.0** | Full savings + client + organization + group scope operational; one deferred domain (loan) begins activation | Group/center foundation rows go from `conditional planned` to `conditional implemented`; loan domain enters `frontier`. |
| **Backlog** | Loan, accounting/GL, tax, audit, custom-datatables activation | Category G becomes empty as domains one-by-one move out. |
