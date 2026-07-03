---
type: Query
title: Savings Balance Summary
description: One-row snapshot of active savings balance.
resource: ../../knowledge/queries/savings/balance_summary.yaml
tags: [sql, savings, fineract]
---

# Contract

- **Runtime id:** `savings.balance_summary`
- **Database:** `fineract` (read-only replica)
- **SQL file:** `queries/savings/balance_summary.sql`
- **Tables:** `m_savings_account`, `m_client`
- **Cost class:** medium — `timeout_ms: 5000`

# Guards

`select_only`, `single_statement`, `require_office_filter`. Office scope bound via `ANY($N::bigint[])` from `authorized_scope` — never Rust post-filter.

# Consumers

Bound to one capability of the same slug — see [../capabilities/savings.balance_summary](../capabilities/savings.balance_summary.md).
