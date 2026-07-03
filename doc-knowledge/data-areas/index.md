---
type: Category
title: Data Areas
description: Fineract data scope decomposition — which tables are in-scope, conditional, deferred, or rejected. Runtime source under ../../knowledge/data-scope/areas/.
tags: [data-scope, fineract]
---

# Data Areas

## In scope (MVP)

- [organization-foundation](./organization-foundation.md) — `m_office`, `m_staff`
- [client-foundation](./client-foundation.md) — `m_client`
- [savings-core](./savings-core.md) — `m_savings_account`, `m_savings_product`
- [savings-transactions](./savings-transactions.md) — `m_savings_account_transaction`

## Conditional / secondary

- [group-center-foundation](./group-center-foundation.md) — enable only for group lending deployments
- [savings-charges-fees](./savings-charges-fees.md) — secondary, waiting on charge semantics

## Deferred (no runtime use)

- [loans](./loans.md), [accounting-gl](./accounting-gl.md), [tax](./tax.md), [custom-datatables](./custom-datatables.md), [audit-users-operations](./audit-users-operations.md)
- Group wrapper: [deferred](./deferred.md)

## Out of scope (hard reject)

- [out-of-scope](./out-of-scope.md) — arbitrary SQL, full-schema search, identity/document reporting, Fineract writes
