---
type: Trace
title: Trace 03 — Top savings deposits (conditional PII)
description: Same capability, two API keys — with and without can_view_pii. Shows how the policy pass strips fields at plan time.
tags: [trace, top_n, pii, savings]
---

# User prompt

> **EN:** "Show the largest 5 deposits between March 1 and March 31 2026."
> **ID:** "Tampilkan 5 setoran terbesar antara 1 sampai 31 Maret 2026."

# 1 · Classifier

```json
{
  "intent": "top_transactions",
  "candidate_capabilities": ["savings_deposit_top_n"],
  "extracted_params": { "from_date": "2026-03-01", "to_date": "2026-03-31", "limit": 5 },
  "output_mode_hint": "top_n"
}
```

# 2 · Planner

Match on `savings_deposit_top_n`. All three required params present.

```json
{
  "capability_id": "savings_deposit_top_n",
  "query_id": "savings.deposit_top_n",
  "output_mode": "top_n",
  "bound_params": {
    "from_date": "2026-03-01",
    "to_date": "2026-03-31",
    "limit": 5
  }
}
```

# 3 · Policy — Variant A: `can_view_pii=true`

- **pii** — capability allows `client_id`, `client_display_name` when `can_view_pii=true`. Both retained.
- **execution_limits** — `limit=5 ≤ max_limit=100`, date range 30 days ✓.
- Decision: **allow with PII**.

# 3 · Policy — Variant B: `can_view_pii=false`

- **pii** — capability's `omitted_when_cannot_view_pii` = `[client_id, client_display_name]`. Both **stripped** from the output projection.
- Decision: **allow, projection reduced to public_business fields**.

# 4 · Executor

Same SQL runs in both variants (SQL doesn't branch on `can_view_pii`). The projection strip happens at the formatter — but the fields are also **not included** in the formatted output for Variant B.

```sql
$1::date='2026-03-01' $2::date='2026-03-31' $3::bigint[]=ARRAY[1,2,5]
$4::text=NULL $5::bigint[]=NULL $6::int=5
```

# 5 · Formatter

**Variant A output:**

| # | Date | Amount (IDR) | Office | Product | Client |
|---|---|---|---|---|---|
| 1 | 2026-03-14 | 25,000,000 | Cabang Solo | Tabungan Utama | Dewi Kartika |
| 2 | 2026-03-08 | 18,500,000 | Cabang Solo | Tabungan Utama | Budi Santoso |
| … | | | | | |

**Variant B output (same rows, PII stripped):**

| # | Date | Amount (IDR) | Office | Product |
|---|---|---|---|---|
| 1 | 2026-03-14 | 25,000,000 | Cabang Solo | Tabungan Utama |
| 2 | 2026-03-08 | 18,500,000 | Cabang Solo | Tabungan Utama |
| … | | | | |

# 6 · Never-return fields

Regardless of `can_view_pii`, these are **never** in the projection or output: `account_no`, `external_id`, `ref_no`, `payment_detail_id`. They are `secret_never_expose`. See [glossary → Money & data](../glossary.md#money--data).
