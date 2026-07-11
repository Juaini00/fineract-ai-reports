# Capability Coverage Matrix: B. Savings — transactions

Source: `docs-old/capability-coverage-matrix.md`

## B. Savings — transactions

Admin decisions supported here: cashflow direction and volume, throughput of the branch network, exceptional individual movements, and reversal risk. These are the questions asked most often on operating dashboards.

| Row | Aggregate total | By month | By week | By day | By N-day bucket | Top-N transactions | Top-N per month | Top-N per week | Individual list | Composite | Ranking (offices/products/staff) | Comparative |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Deposit | `implemented` (`savings_deposit_total`) | `implemented` (`savings_deposit_monthly_breakdown`) | `planned` (v0.2 — `<planned: savings_deposit_breakdown> bucket=week`) | `planned` (v0.2 — `bucket=day`) | `planned` (v0.3 — `bucket=N_days`) | `implemented` (`savings_deposit_top_n`) | `implemented` (`savings_deposit_monthly_top_n`) | `planned` (v0.2) | `planned` (v0.4 — `<planned: savings_activity_list>`) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) |
| Withdrawal | `implemented` (`savings_withdrawal_total`) | `implemented` (`savings_withdrawal_monthly_breakdown`) | `planned` (v0.2) | `planned` (v0.2) | `planned` (v0.3) | `implemented` (`savings_withdrawal_top_n`) | `implemented` (`savings_withdrawal_monthly_top_n`) | `planned` (v0.2) | `planned` (v0.4) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) |
| Deposit + withdrawal side by side | `planned` (v0.3 — composite planner) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) | — | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.3) |
| Net movement (deposit − withdrawal) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | — | — | — | — | — | `planned` (v0.4) | `planned` (v0.3) |
| Transaction count | `planned` (v0.2) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | — | — | — | — | — | `planned` (v0.4) | `planned` (v0.3) |
| Reversed transaction count | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) | — | — | — | — | `planned` (v0.4) | `planned` (v0.4) |
| Individual transaction list | — | — | — | — | — | — | — | — | `planned` (v0.4 — `<planned: savings_activity_list>`; new `list` output_mode + row-level PII gate) | — | — | — |
| Top-N per office / product | — | — | — | — | — | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) | — | — | `planned` (v0.3) | — |
| Top-N per custom bucket (bucket_days) | — | — | — | — | — | `planned` (v0.3) | — | — | — | — | — | — |
