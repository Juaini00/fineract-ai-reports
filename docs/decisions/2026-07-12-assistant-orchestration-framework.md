# Assistant Orchestration Framework Decision

Status: active
Date: 2026-07-12

## Decision

Adopt targeted external libraries for the layers that benefit most from proven abstractions, and keep custom Rust for the layers where domain fit or security control matters more than a framework.

| Layer | Choice |
|---|---|
| LLM abstraction, agent loop, tool calling, structured output | `rig` |
| Retrieval indexing pipeline | Reject `swiftide` for runtime query path; keep current `KnowledgeRepository` |
| Graph orchestration | `petgraph` + a typed enum state machine (custom, ~500 LOC target) |
| Job memory, session memory, context window, response renderer | Custom Rust |

## Rejected Alternatives

The community `rust-langgraph` (published as `langgraph`, `langgraph-core`, `langgraph-checkpoint-postgres`) and `langchain-ai-rust` crates are rejected for this migration.

Reasons:

- Low maturity and unclear maintenance for a security-sensitive backend.
- Unstable APIs that would tie the runtime to upstream drift.
- LangGraph's dynamic graph-builder pattern does not translate cleanly to Rust; a typed enum + transition table over `petgraph` is smaller, faster, debuggable, and carries zero upstream-drift risk.

## Consequence

- The existing `chat` crate owns graph state contracts, memory contracts, context window contracts, and structured response contracts; the workspace remains exactly `app`, `core`, and `chat`.
- Phase 0 provider-agnostic transport uses the project `LLM_*` OpenAI-compatible chat-completions config; `rig` remains the intended framework boundary and is currently proven in the PoC through its `Tool` trait.
- Phase 0 rejected `swiftide` for runtime query retrieval because its node/metadata shape compiles but does not clearly beat `KnowledgeRepository`; it may be revisited for offline indexing only.
- No external orchestration library replaces the graph runtime. If a future replacement is proposed, it must use the existing PostgreSQL job/session memory tables and go through the same Node trait boundary.

## Guardrails (Unchanged)

- LLMs must not generate SQL.
- Fineract transactional rows must not be indexed into vector storage.
- API key capability scope, office scope, and PII checks remain mandatory before execution.
- Approved SQL is loaded only from `queries/*.sql`.
- All responses go through `AssistantResponse` + renderer trait — no direct Markdown authoring in runtime code.

## References

- Spec: `docs/superpowers/specs/2026-07-12-semantic-assistant-platform-migration-design.md`
- Plan: `docs/superpowers/plans/2026-07-12-semantic-assistant-platform-migration.md`
- Issue: `docs/issues/active/002-semantic-assistant-platform-major-refactor.md`
