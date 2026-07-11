---
type: Metric
title: Savings Deposit Count
description: Count of deposit transactions.
resource: ../../knowledge/metrics/savings/deposit_count.yaml
tags: [metric, savings]
---

# Definition

- **Runtime id:** `savings.deposit_count` — status `approved_mvp`
- **Expression:** `COUNT(*)`
- **Filters:** transaction_type_enum=1, is_reversed=false
- **Sensitivity:** `public_business`

Grouping compatible with `currency_code`, `office_id`, `product_id`.
Enforce office scope on the source table's `office_id` (see [office_scope](../policies/office-scope.md)).
