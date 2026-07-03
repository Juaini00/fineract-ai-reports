---
type: Trace
title: Trace 02 — Savings deposit total (date-bounded)
description: Aggregate over a date range. Low cost, no PII.
tags: [trace, aggregate, savings, deposit]
---

# User prompt

> **EN:** "What is the total deposit from January 1 to March 31 2026?"
> **ID:** "Total setoran tabungan dari 1 Januari sampai 31 Maret 2026."

# Auth context

```json
{ "allowed_office_ids": [1, 2, 5], "can_view_pii": false }
```

# 1 · Classifier

```json
{
  "intent": "aggregate_over_period",
  "candidate_capabilities": ["savings_deposit_total"],
  "extracted_params": { "from_date": "2026-01-01", "to_date": "2026-03-31" },
  "output_mode_hint": "total",
  "requires_date_range": true
}
```

# 2 · Planner

Match on `savings_deposit_total`. Required params `from_date` + `to_date` are present.

```json
{
  "capability_id": "savings_deposit_total",
  "query_id": "savings.deposit_total",
  "output_mode": "total",
  "bound_params": { "from_date": "2026-01-01", "to_date": "2026-03-31" }
}
```

# 3 · Policy

- **office_scope** — inject `[1, 2, 5]`.
- **pii** — `returns_pii=false, behavior=aggregate_only`. No PII to strip.
- **execution_limits** — 89 days ≤ `max_date_range_days=366` ✓.

Decision: **allow**.

# 4 · Executor

```sql
-- queries/savings/deposit_total.sql (parameter binding)
$1::date     = '2026-01-01'   -- from_date
$2::date     = '2026-03-31'   -- to_date
$3::bigint[] = ARRAY[1,2,5]   -- office_ids
$4::text     = NULL           -- currency_code
$5::bigint[] = NULL           -- product_ids
-- required_filters enforced inside SQL:
--   transaction_type_enum = 1
--   is_reversed = false
--   transaction_date BETWEEN $1 AND $2
--   office_id = ANY($3)
```

Result:

```json
{ "from_date":"2026-01-01","to_date":"2026-03-31","total_deposit_amount":"2412500000.00","deposit_count":8123 }
```

# 5 · Formatter

```
Total deposits from 2026-01-01 to 2026-03-31: IDR 2,412,500,000.00 across 8,123 transactions.
```

# 6 · Notes

- **Reversed transactions excluded by default** — user didn't ask about reversals. If they had, no approved capability handles reversal analysis; would fall to unsupported.
- **Currency mixing** — if the Fineract deployment has multiple currencies and no `currency_code` filter, totals could mix. Formatter shows the first currency found; production deployments typically bind a single currency.
