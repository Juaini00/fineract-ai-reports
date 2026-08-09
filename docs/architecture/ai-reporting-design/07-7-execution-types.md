# AI Reporting Service Design: 7. Execution Types

Source: `docs-old/ai-reporting-design.md`

## 7. Execution Types

> **Issue 012 (implemented):** these are now *shapes of the one workflow
> runtime*, not separate runtimes. The classifier-selected capability is
> compiled + verified into a typed workflow (petgraph control plane) and run by
> `WorkflowRunner`; atomic is the single-`ExecuteQuery`-node case. The legacy
> atomic-only planner was deleted in Phase 7.

### 7.1 Atomic Execution

One capability executes one approved query.

Example:

```text
savings_deposit_top_n -> queries/savings/deposit_top_n.sql
```

### 7.2 Composite Execution

Multiple approved capabilities are executed and combined in Rust.

Example:

```text
deposit vs withdrawal this month
  -> execute savings_deposit_total
  -> execute savings_withdrawal_total
  -> combine as comparison
```

### 7.3 Iterative Execution

One capability is executed multiple times over split periods.

Example:

```text
Largest deposit for each month from Jan-Sep
  -> Jan: execute deposit_top_n
  -> Feb: execute deposit_top_n
  -> ...
  -> Sep: execute deposit_top_n
```

Iterative execution must use bounded concurrency.

Example policy:

```yaml
max_iterations: 12
max_parallel_queries: 3
per_query_timeout_ms: 3000
total_timeout_ms: 10000
fail_policy: fail_fast
```
