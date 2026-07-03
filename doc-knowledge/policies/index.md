---
type: Category
title: Reporting Policies
description: Cross-cutting guards evaluated at plan-time by chat::policy::authorization. Runtime source under ../../knowledge/policies/.
tags: [policy, security]
---

# Policies

- [pii](./pii.md) — sensitivity classes, when identity fields may be returned
- [office_scope](./office-scope.md) — office scope enforcement (bound SQL parameter, never post-filter)
- [query_safety](./query-safety.md) — select-only, single-statement, parameterized, no runtime AI SQL
- [execution_limits](./execution-limits.md) — timeouts, row caps, date range caps
- [unsupported_requests](./unsupported-requests.md) — hard reject and clarification categories
