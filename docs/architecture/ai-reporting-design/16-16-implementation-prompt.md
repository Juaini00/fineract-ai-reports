# AI Reporting Service Design: 16. Implementation Prompt

Source: `docs-old/ai-reporting-design.md`

## 16. Implementation Prompt

Use this prompt as the English implementation brief for future development sessions:

```text
We are building a Rust-based AI Reporting Service for Apache Fineract data.

Important constraints:
- Do not modify Apache Fineract.
- Do not add Fineract plugins or APIs.
- Do not change the Fineract database schema.
- Read business data only from a read-only Fineract database connection or read replica.
- The AI must never execute arbitrary SQL.
- Runtime execution must use approved capabilities and approved SQL query files.
- Runtime SQL execution must use static approved query bindings; do not pass arbitrary runtime SQL strings to SQLx.
- Rust is responsible for validation, planning, policy enforcement, execution, audit logging, and result shaping.
- The LLM provider is used only for AI planning fallback and response formatting. DeepSeek is the current/default provider; other OpenAI-compatible providers can be configured with `LLM_*` environment variables.
- pgvector is used for semantic knowledge retrieval, not for transactional numeric data.
- Authentication uses application-managed API keys.
- Every protected request must include Authorization: Bearer <api_key> or X-API-Key.
- Raw API keys must be shown only once and never stored.
- Store only hashed API keys plus metadata/scopes.
- API keys must support revocation, expiration, capability scopes, office scopes, and PII visibility.

Build the backend with the current maintainable boundary:
- app: binary entrypoint and composition root.
- core: shared foundation containing config, DB pools, API primitives, auth, validation, response envelope, and API key ClientContext.
- chat: chat-driven reporting feature containing API routes/handlers/DTOs, sessions, messages, jobs, knowledge catalog/index usage, report policy helpers, checkpoints/events, and future pipeline orchestration.
- Do not add api/infra/runtime/knowledge/reporting or `ai_report_*` crates yet.

Currently enabled runtime slice:
- Domain: savings.
- Capabilities: savings_deposit_top_n and savings_deposit_total.
- Knowledge files under knowledge/.
- SQL files under queries/.
- App database: PostgreSQL database ai_reports with pgvector enabled.
- Fineract database: PostgreSQL read-only connection from environment variable FINERACT_DATABASE_URL.
- Auth bootstrap: API key creation is initially protected by AUTH_BOOTSTRAP_ADMIN_TOKEN.

Implement in this order:
1. Workspace/module structure.
2. Config loader from .env.
3. Health and readiness endpoints.
4. App DB and Fineract DB connection checks.
5. API key database table and API key generation endpoint.
6. API key authentication middleware.
7. Chat session/job migrations.
8. Chat job API foundation.
9. Catalog YAML loader and validator.
10. Catalog retrieval document/index persistence without embeddings.
11. Query SQL loader and validator.
12. Local classifier for savings deposit total/top_n.
13. Policy guard and execution plan types.
14. Capability/office/PII authorization guards based on API key scopes.
15. Query executor with parameter binding, statement timeout, and row limit.
16. Audit logging.
17. Template response formatter.
18. LLM client for later planner/formatter fallback.

Keep the implementation small and testable. Do not introduce dynamic SQL generation. If a request is unsupported, return a safe unsupported response.
```
