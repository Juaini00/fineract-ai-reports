---
type: Metric
title: Savings Withdrawal Amount
description: Total withdrawal amount.
resource: ../../knowledge/metrics/savings/withdrawal_amount.yaml
tags: [metric, savings]
---

# Definition

- **Runtime id:** `savings.withdrawal_amount` — status `approved_mvp`
- **Expression:** `SUM(m_savings_account_transaction.amount)`
- **Filters:** transaction_type_enum=2, is_reversed=false
- **Sensitivity:** `public_business`

Grouping compatible with `currency_code`, `office_id`, `product_id`.
Enforce office scope on the source table's `office_id` (see [office_scope](../policies/office_scope.md)).
