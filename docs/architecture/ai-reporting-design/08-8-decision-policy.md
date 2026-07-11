# AI Reporting Service Design: 8. Decision Policy

Source: `docs-old/ai-reporting-design.md`

## 8. Decision Policy

The system must not make decisions based on vague intuition. It must use explicit score, threshold, margin, and rule-based indicators.

Example policy:

```yaml
decision_policy:
  unsupported_threshold: 0.85

  domain:
    accept_score: 0.75
    clarify_score: 0.45
    min_margin: 0.20

  capability:
    accept_score: 0.80
    clarify_score: 0.55
    min_margin: 0.15

  execution:
    direct_max_estimated_latency_ms: 5000
    async_max_estimated_latency_ms: 60000
```

Decision outcomes:

```text
high confidence -> execute
medium confidence -> ask clarification
low confidence -> unsupported
hard unsupported -> unsupported immediately
missing required params -> ask clarification
```
