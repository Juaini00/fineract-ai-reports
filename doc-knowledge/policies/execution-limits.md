---
type: Policy
title: Execution Limits Policy
description: Timeouts, row caps, date range caps, and per-capability overrides for query execution.
resource: ../../knowledge/policies/execution_limits.yaml
tags: [policy, performance]
---

# Defaults

| Limit | Value |
|---|---|
| `query_timeout_ms` | 3000 |
| `max_rows` | 100 |
| `max_date_range_days` | 366 |
| `max_iterations` | 12 |
| `max_parallel_queries` | 3 |

Direct execution latency ceiling: 5s. Async execution ceiling: 60s.

# Overrides

Capability-specific overrides live under `capability_overrides`. Every `approved_mvp` capability must either declare an override or inherit defaults. `top_n` capabilities must declare `max_rows`; date-bounded capabilities must declare `max_date_range_days`.
