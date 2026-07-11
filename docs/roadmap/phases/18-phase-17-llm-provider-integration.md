# Implementation Steps: Phase 17: LLM Provider Integration

Source: `docs-old/implementation-steps.md`

## Phase 17: LLM Provider Integration

Goal: add AI only after the deterministic pipeline works.

Initial use cases:

1. Planner fallback for ambiguous requests.
2. Clarification question generation.
3. Natural-language response formatting for complex results.

Do not use the LLM provider for:

```text
raw SQL generation at runtime
unbounded schema exploration
large result computation
```

Current status:

```text
PARTIALLY DONE

Implemented:
LLM config is loaded from LLM_* environment variables, with legacy DEEPSEEK_* fallback for local compatibility.
crates/chat/src/chat/llm.rs provides a constrained OpenAI-compatible planner fallback client.
Current/default provider is DeepSeek (`LLM_PROVIDER=deepseek`, `LLM_MODEL=deepseek-chat`).
Other OpenAI-compatible providers can be used by changing `LLM_CHAT_COMPLETIONS_URL`, `LLM_MODEL`, and `LLM_API_KEY`.
JobService invokes the LLM only after deterministic/vector classification returns clarification with approved options.
The LLM may return only: one provided capability id, a clarification question, or unsupported.
Returned capability ids are checked against the provided approved options before planning.
Rust still extracts parameters, runs policy checks, and executes only static approved SQL bindings.

Verified 2026-07-02:
Ambiguous prompt "Show customer savings activity this week" returned clarification_required with source=llm_planner and did not execute SQL.

Still pending:
response formatting fallback for complex results
broader prompt context consumption beyond clarification options
```
