---
type: Trace
title: Trace 01 — Savings balance summary (snapshot)
description: Snapshot answer, no date range, no PII. Simplest happy path.
tags: [trace, snapshot, savings]
---

# User prompt (bilingual)

> **EN:** "What is the total savings balance right now?"
> **ID:** "Berapa saldo total tabungan aktif saat ini?"

# Auth context

```json
{
  "api_key_prefix": "kkm_a1b2",
  "allowed_office_ids": [1, 2, 5],
  "can_view_pii": false
}
```

# 1 · Classifier

Vector search over the catalog returns:

```json
{
  "intent": "portfolio_snapshot",
  "candidate_capabilities": ["savings_balance_summary"],
  "extracted_params": {},
  "output_mode_hint": "summary",
  "requires_date_range": false,
  "domain_hint": "savings"
}
```

# 2 · Planner

Exactly one approved match → rule-based selection.

```json
{
  "capability_id": "savings_balance_summary",
  "query_id": "savings.balance_summary",
  "output_mode": "summary",
  "bound_params": { "office_ids": "TBD (policy)" }
}
```

No `required_parameters` → skip clarification.

# 3 · Policy

- **office_scope** — no user-supplied `office_ids` → inject `[1, 2, 5]` from `allowed_office_ids`.
- **pii** — capability declares `returns_pii=false` → nothing to strip.
- **query_safety** — `savings.balance_summary` is in the approved catalog.
- **execution_limits** — cost class `medium`, `timeout_ms=5000`, no date range to bound.
- **unsupported_requests** — not out-of-scope.

Decision: **allow**.

# 4 · Executor

```sql
-- queries/savings/balance_summary.sql (parameter binding shown)
$1::bigint[] = ARRAY[1, 2, 5]          -- office_ids from authorized_scope
$2::text     = NULL                     -- currency_code
$3::bigint[] = NULL                     -- product_ids
```

Result (one row):

```json
{ "account_count": 12384, "total_balance": "8912450350.00", "average_balance": "719834.71", "max_balance": "48200000.00" }
```

# 5 · Formatter

Template: `summary`. Output:

```
Active savings portfolio: 12,384 accounts. Total balance IDR 8,912,450,350.00
(avg IDR 719,834.71, max IDR 48,200,000.00).
```

# 6 · Job state

- `chat_jobs.status = completed`
- `chat_job_events`: `queued → running → completed`
- Redis `chat_job:{id}:live_state = completed` (mirror, 1h TTL)
