# AI Reporting Service Design: 2. Core Principles

Source: `docs-old/ai-reporting-design.md`

## 2. Core Principles

1. The AI must never execute arbitrary SQL against the Fineract database.
2. Runtime queries must come from approved capabilities and approved query definitions.
3. Rust is the main validator, planner, policy enforcer, and executor.
4. The LLM provider is used for AI planning fallback and response formatting, not as a SQL executor. Current/default provider: DeepSeek through an OpenAI-compatible chat-completions endpoint.
5. Vector search is used to find relevant knowledge, not to search transactional numbers. See `docs/rag-architecture.md` for the full RAG indexing and retrieval design.
6. Unsupported requests must be rejected safely.
7. Heavy reports must be rejected, clarified, or executed as asynchronous jobs.
8. Every important decision must be auditable: selected domain, selected capability, confidence score, query id, parameters, latency, and token usage.
