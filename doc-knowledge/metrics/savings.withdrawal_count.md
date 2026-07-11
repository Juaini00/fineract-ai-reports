---
type: Metric
title: Savings Withdrawal Count
description: Count of withdrawal transactions.
resource: ../../knowledge/metrics/savings/withdrawal_count.yaml
tags: [metric, savings]
---

# Definition

- **Runtime id:** `savings.withdrawal_count` — status `approved_mvp`
- **Expression:** `COUNT(*)`
- **Filters:** transaction_type_enum=2, is_reversed=false
- **Sensitivity:** `public_business`

Grouping compatible with `currency_code`, `office_id`, `product_id`.
Enforce office scope on the source table's `office_id` (see [office_scope](../policies/office-scope.md)).
