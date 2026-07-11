---
type: Metric
title: Savings Account Balance (snapshot)
description: Snapshot balance per active savings account.
resource: ../../knowledge/metrics/savings/account_balance.yaml
tags: [metric, savings]
---

# Definition

- **Runtime id:** `savings.account_balance` — status `approved_mvp`
- **Expression:** `m_savings_account.account_balance_derived`
- **Filters:** account owner office ∈ authorized offices
- **Sensitivity:** `public_business`

Grouping compatible with `currency_code`, `office_id`, `product_id`.
Enforce office scope on the source table's `office_id` (see [office_scope](../policies/office-scope.md)).
