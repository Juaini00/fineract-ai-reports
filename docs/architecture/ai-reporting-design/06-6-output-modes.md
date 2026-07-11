# AI Reporting Service Design: 6. Output Modes

Source: `docs-old/ai-reporting-design.md`

## 6. Output Modes

The output mode determines which query and response contract should be used.

Initial output modes:

```text
total
list
top_n
daily_breakdown
monthly_breakdown
monthly_top_n
grouped_summary
comparison
```

Examples:

```text
"Total deposit from Jan-Sep" -> total
"Deposit per month from Jan-Sep" -> monthly_breakdown
"Largest deposit for each month from Jan-Sep" -> monthly_top_n
"List deposits this month" -> list
```

Rules:

1. `total` must use aggregate SQL.
2. `list` must use detail SQL with pagination or a strict limit.
3. `monthly_breakdown` must use SQL grouping by month.
4. `top_n` must use ordering and limit in SQL.
5. The service must not fetch a large list and sum it in Rust.
