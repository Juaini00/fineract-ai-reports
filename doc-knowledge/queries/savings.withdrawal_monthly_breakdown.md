---
type: Query
title: Savings Withdrawals by Month
description: Monthly withdrawal totals.
resource: ../../knowledge/queries/savings/withdrawal_monthly_breakdown.yaml
tags: [sql, savings, fineract]
---

# Contract

- **Runtime id:** `savings.withdrawal_monthly_breakdown`
- **Database:** `fineract` (read-only replica)
- **SQL file:** `queries/savings/withdrawal_monthly_breakdown.sql`
- **Tables:** `m_savings_account_transaction`, `m_savings_account`, `m_office`
- **Cost class:** medium — `timeout_ms: 5000`

# Guards

`select_only`, `single_statement`, `require_office_filter`. Office scope bound via `ANY($N::bigint[])` from `authorized_scope` — never Rust post-filter.

# Consumers

Bound to one capability of the same slug — see [../capabilities/savings.withdrawal_monthly_breakdown](../capabilities/savings.withdrawal_monthly_breakdown.md).
