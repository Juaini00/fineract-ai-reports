---
type: Category
title: Approved SQL Queries
description: One concept per runtime query in ../../knowledge/queries/. Each maps 1:1 to a reviewed SQL file under ../../queries/.
tags: [queries, sql, fineract]
---

# Approved SQL Queries

All queries are `select_only`, `single_statement`, `parameterized_only`, bind `office_ids` from `authorized_scope`, and are validated at startup by `KnowledgeSyncService`.

## Savings

- [savings.balance_summary](./savings.balance_summary.md)
- [savings.deposit_total](./savings.deposit_total.md)
- [savings.deposit_top_n](./savings.deposit_top_n.md)
- [savings.deposit_monthly_breakdown](./savings.deposit_monthly_breakdown.md)
- [savings.deposit_monthly_top_n](./savings.deposit_monthly_top_n.md)
- [savings.withdrawal_total](./savings.withdrawal_total.md)
- [savings.withdrawal_top_n](./savings.withdrawal_top_n.md)
- [savings.withdrawal_monthly_breakdown](./savings.withdrawal_monthly_breakdown.md)
- [savings.withdrawal_monthly_top_n](./savings.withdrawal_monthly_top_n.md)
