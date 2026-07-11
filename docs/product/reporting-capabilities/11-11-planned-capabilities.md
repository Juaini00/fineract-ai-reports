# Reporting Capabilities: 11. Planned Capabilities

Source: `docs-old/reporting-capabilities.md`

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
