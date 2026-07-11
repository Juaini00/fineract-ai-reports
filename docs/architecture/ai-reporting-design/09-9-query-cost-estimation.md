# AI Reporting Service Design: 9. Query Cost Estimation

Source: `docs-old/ai-reporting-design.md`

## 9. Query Cost Estimation

The system estimates whether a report is safe to execute directly.

Inputs:

1. Capability cost class.
2. Date range.
3. Output mode.
4. Limit.
5. Grouping dimensions.
6. Filter availability.
7. Estimated rows.
8. Optional `EXPLAIN`.
9. Runtime performance history.

Decision:

```text
estimated <= 5s -> direct execution
estimated 5s-60s -> async job
too large -> reject or ask for narrower filters
```
