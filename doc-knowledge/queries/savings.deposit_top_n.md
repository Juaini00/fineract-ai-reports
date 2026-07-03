---
type: Query
title: Top Savings Deposits
description: Largest N deposit transactions.
resource: ../../knowledge/queries/savings/deposit_top_n.yaml
tags: [sql, savings, fineract]
---

# Contract

- **Runtime id:** `savings.deposit_top_n`
- **Database:** `fineract` (read-only replica)
- **SQL file:** `queries/savings/deposit_top_n.sql`
- **Tables:** `m_savings_account_transaction`, `m_savings_account`, `m_savings_product`, `m_client`, `m_office`
- **Cost class:** medium — `timeout_ms: 3000`

# Guards

`select_only`, `single_statement`, `require_office_filter`. Office scope bound via `ANY($N::bigint[])` from `authorized_scope` — never Rust post-filter.

# Consumers

Bound to one capability of the same slug — see [../capabilities/savings.deposit_top_n](../capabilities/savings.deposit_top_n.md).
