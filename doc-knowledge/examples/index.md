---
type: Category
title: End-to-End Traces
description: Canonical prompts (bilingual EN/ID) traced through the pipeline — classification, plan, policy, SQL, response.
tags: [examples, traces, e2e]
---

# Traces

Five canonical scenarios that together cover every runtime path. Each trace shows the user prompt, what the classifier extracts, the plan, the policy decision, the SQL that runs (parameter bindings only — full SQL lives under `queries/`), and the final response.

| # | Prompt shape | Capability | Path exercised |
|---|---|---|---|
| 1 | [Snapshot summary](./trace-01-balance-summary.md) | `savings.balance_summary` | Happy path, no date range, aggregate |
| 2 | [Aggregate over date range](./trace-02-deposit-total.md) | `savings.deposit_total` | Happy path, date-bounded, low PII risk |
| 3 | [Top-N with conditional PII](./trace-03-deposit-top-n.md) | `savings.deposit_top_n` | Two variants: `can_view_pii=true` vs `false` |
| 4 | [Missing date range → clarify](./trace-04-clarification.md) | `savings.withdrawal_total` | Same-job clarification loop |
| 5 | [Deferred domain → unsupported](./trace-05-unsupported.md) | (none) | Hard reject, sanitized template |

Read [../glossary](../glossary.md) and [../architecture/request-flow](../architecture/request-flow.md) first — traces assume both.
