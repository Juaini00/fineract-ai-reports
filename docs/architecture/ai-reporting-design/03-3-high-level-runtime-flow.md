# AI Reporting Service Design: 3. High-Level Runtime Flow

Source: `docs-old/ai-reporting-design.md`

## 3. High-Level Runtime Flow

```text
Client
  -> API Key Authentication
  -> API
  -> Request Understanding
  -> Knowledge Retrieval
  -> Planning
  -> Policy Guard
  -> Query Execution
  -> Result Formatting
  -> Audit / Metrics
```

Detailed flow:

```text
1. Receive user request.
2. Validate API key.
3. Build authenticated client context.
4. Normalize the text.
5. Detect unsupported intent early.
6. Match domain candidates.
7. Match capability candidates.
8. Detect output mode and extract parameters.
9. Build an execution plan.
10. Validate the plan using policy guards.
11. Estimate execution cost.
12. Execute approved query or create an async job.
13. Format the response.
14. Store audit logs and metrics.
```
