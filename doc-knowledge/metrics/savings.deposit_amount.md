---
type: Metric
title: Savings Deposit Amount
description: Total deposit amount.
resource: ../../knowledge/metrics/savings/deposit_amount.yaml
tags: [metric, savings]
---

# Definition

- **Runtime id:** `savings.deposit_amount` — status `approved_mvp`
- **Expression:** `SUM(m_savings_account_transaction.amount)`
- **Filters:** transaction_type_enum=1, is_reversed=false
- **Sensitivity:** `public_business`

Grouping compatible with `currency_code`, `office_id`, `product_id`.
Enforce office scope on the source table's `office_id` (see [office_scope](../policies/office_scope.md)).
