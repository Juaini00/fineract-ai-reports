# Implementation Steps: Phase 14: Query Executor MVP

Source: `docs-old/implementation-steps.md`

## Phase 14: Query Executor MVP

Goal: execute approved SQL safely against Fineract read-only database.

Executor requirements:

1. Use parameter binding only.
2. Set statement timeout.
3. Enforce max rows.
4. Use read-only pool.
5. Return structured result.
6. Record latency and status.
7. Never concatenate user input into SQL.

Current status:

```text
PARTIALLY DONE

Implemented:
crates/chat/src/chat/executor.rs
Synchronous executor runs approved catalog SQL after policy_decision is allowed.
Approved SQL is selected through static `include_str!` bindings by query id, not runtime dynamic SQL strings.
Parameters are bound from execution_plan and policy_decision; user input is not concatenated into SQL.
Results are stored in chat_jobs.result_json and job status becomes completed.
Execution/policy errors are stored as sanitized chat_jobs.error_json and job status becomes failed.
Completion writes response_completed checkpoint and final event.
Failure writes job_failed checkpoint and error event.
Result/error payloads include latency_ms.

Still pending:
statement timeout enforcement
max row enforcement beyond SQL LIMIT metadata
background worker instead of synchronous create-job execution
```
