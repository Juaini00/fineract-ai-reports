# Reporting Capabilities: 6. Additional Currently Implemented Savings Capabilities

Source: `docs-old/reporting-capabilities.md`

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
