# AI Reporting Service Design: 13. Client / Admin UI Design

Source: `docs-old/ai-reporting-design.md`

## 13. Client / Admin UI Design

The system also needs a client/admin interface, even if the first implementation focuses on backend AI integration.

The UI has two different purposes:

1. End-user reporting chat.
2. Admin/developer management for catalog, YAML knowledge, token usage, jobs, and observability.

### 13.1 End-User Chat UI

Features:

1. Chat input for reporting questions.
2. Display final answer.
3. Display structured report table when result is tabular.
4. Display selected period, filters, and source capability.
5. Display clarification prompts when request is ambiguous.
6. Display unsupported message when capability is unavailable.
7. Optional: export result to CSV.

### 13.2 Admin / Developer UI

Features:

1. Generate API keys.
2. Revoke API keys.
3. View API key scopes and last usage.
4. View domain knowledge files.
5. View capability catalog.
6. View query catalog.
7. Validate catalog files.
8. Run query smoke tests.
9. View execution logs.
10. View token usage.
11. View AI planner requests/responses.
12. View query latency and timeout statistics.
13. View vector index status.
14. Trigger reindexing of knowledge embeddings.
15. Manage enabled/disabled capabilities.

The first version does not need full editing support. Viewing and validation are enough for safety.

### 13.3 Suggested Frontend Stack

The frontend can be built later with:

```text
Next.js or Vite React
TypeScript
TanStack Query
Tailwind CSS or a component library
```

Initial pages:

```text
/chat
/admin/api-keys
/admin/catalog/domains
/admin/catalog/capabilities
/admin/catalog/queries
/admin/jobs
/admin/logs/executions
/admin/usage/tokens
/admin/vector-index
```
