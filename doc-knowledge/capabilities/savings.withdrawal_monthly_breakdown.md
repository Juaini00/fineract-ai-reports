---
type: Capability
title: Savings Withdrawals by Month
description: Total withdrawals grouped by month.
resource: ../../knowledge/capabilities/savings/withdrawal_monthly_breakdown.yaml
tags: [savings, monthly_breakdown, approved_mvp]
---

# Summary

- **Runtime id:** `savings_withdrawal_monthly_breakdown` (status `approved_mvp`)
- **Query:** [savings.withdrawal_monthly_breakdown](../queries/savings.withdrawal_monthly_breakdown.md) → `queries/savings/withdrawal_monthly_breakdown.sql`
- **Output mode:** `monthly_breakdown`
- **Cost class:** medium
- **PII posture:** aggregate only, no PII

# Parameters

Required: `from_date`, `to_date`. Optional: `office_ids` (bound from authorized_scope), `currency_code`, `product_ids`.

# Guards

`require_office_scope=true`, `max_date_range_days=366` (except summary), reversed transactions excluded by default. See [office_scope](../policies/office_scope.md) and [pii](../policies/pii.md).

# Related

- Metrics used → see the query concept
- Data areas: [savings_core](../data-areas/savings-core.md), [savings_transactions](../data-areas/savings-transactions.md), [organization_foundation](../data-areas/organization-foundation.md), [client_foundation](../data-areas/client-foundation.md)
