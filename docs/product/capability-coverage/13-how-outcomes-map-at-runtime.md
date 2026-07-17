# Capability Coverage Matrix: How outcomes map at runtime

The current runtime is the semantic assistant graph, not the old classifier-first mapper.

```text
user message
  -> session context window
  -> semantic intent route
  -> retrieval plan
  -> knowledge evidence retrieval
  -> evidence evaluation
  -> clarification OR policy guard/tool execution
  -> structured assistant response + Markdown render
```

## Outcome mapping

| Runtime decision | Job/status behavior | User-facing response |
| --- | --- | --- |
| Greeting/help | Completes or waits without SQL execution. | Structured help/summary response. |
| Strong evidence for approved capability/tool | Runs policy guard, then approved catalog SQL only. | Structured table/cards/summary response with PII fields hidden when policy disallows them. |
| Weak or ambiguous evidence | Stores pending clarification in session context and waits on the same job. | Structured clarification options; replies are resolved semantically. |
| Unsupported in-domain request | No SQL execution. | Sanitized unsupported response. |
| Out-of-domain request | No SQL execution. | Sanitized out-of-domain response. |
| Unsafe/PII request without permission | Blocked before execution. | Sanitized policy-blocked response. |
| Semantic router unavailable in local/test config | Saves message/context and waits safely. | Operational clarification telling the user routing is not enabled. |

Coverage-matrix status still constrains execution: only active/approved catalog capabilities with approved SQL can execute. Deferred or rejected data areas may be understood by the assistant, but they do not bypass catalog, policy, office-scope, or PII gates.
