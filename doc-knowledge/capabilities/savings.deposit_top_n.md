---
type: Capability
title: Top Savings Deposits
description: Largest N deposit transactions.
resource: ../../knowledge/capabilities/savings/deposit_top_n.yaml
tags: [savings, top_n, approved_mvp]
---

# Summary

- **Runtime id:** `savings_deposit_top_n` (status `approved_mvp`)
- **Query:** [savings.deposit_top_n](../queries/savings.deposit_top_n.md) → `queries/savings/deposit_top_n.sql`
- **Output mode:** `top_n`
- **Cost class:** medium
- **PII posture:** conditional (`client_display_name`, `client_id`) requires can_view_pii

# Parameters

Required: `from_date`, `to_date`, `limit`. Optional: `office_ids` (bound from authorized_scope), `currency_code`, `product_ids`.

# Guards

`require_office_scope=true`, `max_date_range_days=366` (except summary), reversed transactions excluded by default. See [office_scope](../policies/office_scope.md) and [pii](../policies/pii.md).

# Related

- Metrics used → see the query concept
- Data areas: [savings_core](../data-areas/savings-core.md), [savings_transactions](../data-areas/savings-transactions.md), [organization_foundation](../data-areas/organization-foundation.md), [client_foundation](../data-areas/client-foundation.md)
