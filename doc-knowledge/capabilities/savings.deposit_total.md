---
type: Capability
title: Savings Deposit Total
description: Total savings deposits over a date range.
resource: ../../knowledge/capabilities/savings/deposit_total.yaml
tags: [savings, total, approved_mvp]
---

# Summary

- **Runtime id:** `savings_deposit_total` (status `approved_mvp`)
- **Query:** [savings.deposit_total](../queries/savings.deposit_total.md) → `queries/savings/deposit_total.sql`
- **Output mode:** `total`
- **Cost class:** low
- **PII posture:** aggregate only, no PII

# Parameters

Required: `from_date`, `to_date`. Optional: `office_ids` (bound from authorized_scope), `currency_code`, `product_ids`.

# Guards

`require_office_scope=true`, `max_date_range_days=366` (except summary), reversed transactions excluded by default. See [office_scope](../policies/office_scope.md) and [pii](../policies/pii.md).

# Related

- Metrics used → see the query concept
- Data areas: [savings_core](../data-areas/savings-core.md), [savings_transactions](../data-areas/savings-transactions.md), [organization_foundation](../data-areas/organization-foundation.md), [client_foundation](../data-areas/client-foundation.md)
