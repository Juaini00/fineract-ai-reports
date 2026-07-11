# Implementation Steps: Phase 13: Execution Plan And Policy Guard

Source: `docs-old/implementation-steps.md`

## Phase 13: Execution Plan And Policy Guard

Goal: convert classifier result into validated execution plan.

Plan types:

```text
atomic
composite
iterative
```

MVP only needs:

```text
atomic
```

Policy checks:

1. Capability exists.
2. Query exists.
3. Required params are complete.
4. Date range is within max range.
5. Limit is within max limit.
6. API key can run capability.
7. API key can access requested office scope.

Current status:

```text
PARTIALLY DONE

Implemented:
crates/chat/src/chat/planner.rs
Matched classifier results are converted into a minimal atomic execution plan.
Execution plan is stored in chat_jobs.state_json.execution_plan when a job is created.
Current plan loads and validates the catalog, then maps the matched capability to its approved query id from catalog metadata.
The validated catalog is cached in ChatAppState and reused by job planning.
Policy decision is stored in chat_jobs.state_json.policy_decision when a job is created.
Current policy decision checks API key capability, effective office scope, and simple PII permission before any execution.

Still pending:
required parameter completeness validation against catalog metadata
date range and limit guard enforcement
output mode lookup from richer typed capability metadata instead of MVP naming heuristic
using policy_decision to block execution once a real executor exists
```
