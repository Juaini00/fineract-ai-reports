# 05 — Structured LLM contracts via `schemars`

**Parent:** [Epic](./README.md) · **Priority:** P2 · **Effort:** S (~80 LoC)

## Problem

Router and other LLM calls (`crates/chat/src/assistant/router.rs`, `crates/chat/src/chat/llm.rs`) rely on freeform JSON strings sent as "rules" to the LLM plus manual JSON parsing. Schema drift between Rust types and prompt text is invisible until a query fails at runtime.

`schemars 1.2` is already a workspace dep (Cargo.toml:54) but only used implicitly.

## Proposed change

Every LLM structured call derives its schema from the Rust type:

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct AssistantIntent { ... }

let schema = schemars::schema_for!(AssistantIntent);
let response = llm::structured::<AssistantIntent>(client, purpose, system, user, Some(schema)).await?;
```

Amend `llm::structured` (if not already) to accept an optional schema and pass it to the provider's structured-output API (Deepseek/OpenAI JSON schema mode). Fall back to schema-less JSON mode for providers that don't support it.

Second win: replace the giant hand-written rules array in `router.rs:22-35` with a shorter prompt + schema. The schema encodes valid enum values; prompt only carries semantic guidance.

## Files

- `crates/chat/src/assistant/llm.rs` — extend `structured` signature.
- `crates/chat/src/assistant/router.rs` — trim rules, pass `schema_for!(AssistantIntent)`.
- `crates/chat/src/assistant/reranker.rs` (from issue 02) — same treatment for `RerankerDecision`.
- `crates/chat/src/assistant/mod.rs` — ensure `#[derive(JsonSchema)]` on `AssistantIntent`, `RequestShape`, `AssistantEntity`, related enums.

## Acceptance criteria

- Router prompt length reduces by ≥ 30% (rules → schema).
- Malformed LLM responses fail at the provider layer with a clear schema-violation error instead of at `serde_json::from_str`.
- All existing router tests pass.

## Test plan

- Unit: `schema_for!(AssistantIntent)` produces a schema with all required fields and enums correctly enumerated.
- Integration: `TestLlmClient` returns an invalid enum value → observed error mentions the offending field.

## Out of scope

- Migrating chat/llm.rs planner (`LlmPlannerDecision`) — do it in a follow-up if this pattern works well.

## Dependencies

- Best after issue 02 lands (reranker also benefits from schemars).
