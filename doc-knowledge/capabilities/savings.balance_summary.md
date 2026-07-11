---
type: Capability
title: Savings Balance Summary
description: Snapshot of active savings portfolio — account count, total/average/max balance — scoped to the caller's authorized offices.
resource: ../../knowledge/capabilities/savings/balance_summary.yaml
tags: [savings, summary, snapshot, approved_mvp]
---

# Intent

Answer questions like "berapa saldo total tabungan aktif saat ini?" or "show the savings portfolio summary" with one aggregated row — no per-account rows, no date range required.

# Runtime Contract

- **Capability id:** `savings_balance_summary` (status `approved_mvp`)
- **Query:** [savings.balance_summary](../../knowledge/queries/savings/balance_summary.yaml) → `queries/savings/balance_summary.sql`
- **Metric:** `savings.account_balance`
- **Output mode:** `summary` (single aggregate row, no date_range needed)
- **Domain:** savings
- **Data areas:** organization_foundation, client_foundation, savings_core

# Parameters

| Name | Required | Source |
|---|---|---|
| `office_ids` | yes | bound from `authorized_scope` — never user-supplied |
| `currency_code` | no | user filter |
| `product_ids` | no | user filter |

Defaults: `active_only=true`, `client_owned_only=true`.

# Output Fields

All fields classified `public_business` — no PII.

| Field | Type |
|---|---|
| `account_count` | bigint |
| `total_balance` | decimal |
| `average_balance` | decimal |
| `max_balance` | decimal |

# Guards

- `select_only`, `single_statement`, `snapshot_only`
- `require_office_filter=true` — office scope enforced inside the SQL via bound `office_ids`, not Rust post-filtering (per [reporting-pii-policy](../../docs/product/pii-policy/index.md) and repo invariant)
- `timeout_ms: 5000`, `cost_class: medium`

# Example Prompts

- "What is the total savings balance right now?"
- "Show the savings portfolio summary."
- "Current active savings account balance and count."
- "Berapa saldo total tabungan aktif saat ini?"
- "Ringkasan portofolio tabungan."

# Related

- Policy: [reporting-pii-policy](../../docs/product/pii-policy/index.md), [reporting-data-scope](../../docs/product/reporting-data-scope/index.md)
- Data areas: [savings-core](../../docs/reporting-data/savings-core.md), [client-foundation](../../docs/reporting-data/client-foundation.md), [organization-foundation](../../docs/reporting-data/organization-foundation.md)
