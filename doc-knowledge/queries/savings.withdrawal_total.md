---
type: Query
title: Savings Withdrawal Total
description: Aggregate withdrawal total over date range.
resource: ../../knowledge/queries/savings/withdrawal_total.yaml
tags: [sql, savings, fineract]
---

# Contract

- **Runtime id:** `savings.withdrawal_total`
- **Database:** `fineract` (read-only replica)
- **SQL file:** `queries/savings/withdrawal_total.sql`
- **Tables:** `m_savings_account_transaction`, `m_savings_account`, `m_office`
- **Cost class:** low — `timeout_ms: 3000`

# Guards

`select_only`, `single_statement`, `require_office_filter`. Office scope bound via `ANY($N::bigint[])` from `authorized_scope` — never Rust post-filter.

# Consumers

Bound to one capability of the same slug — see [../capabilities/savings.withdrawal_total](../capabilities/savings.withdrawal_total.md).
