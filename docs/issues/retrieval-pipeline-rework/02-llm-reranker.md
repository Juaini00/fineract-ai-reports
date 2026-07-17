# 02 — Replace `EvidenceEvaluator` with LLM re-ranker

**Parent:** [Epic](./README.md) · **Priority:** P0 · **Effort:** M (~250 LoC new, ~150 LoC removed)

## Problem

`crates/chat/src/assistant/evidence.rs:102-140` picks a capability using rigid arithmetic:

```rust
if allowed.is_empty()         { UnsupportedInDomain }
else if has_conflict || allowed[0].score < 0.25   { Clarify }
else if allowed.len() == 1    { Select { ... } }
else if allowed[0].score - allowed[1].score <= 0.05 || !has_metric_entity { Clarify }
else                          { Select { ... } }
```

Thresholds are hand-picked. `has_metric_entity` requirement forces `Clarify` on legitimate queries where the metric is implicit. No use of natural language context — the evaluator has no idea whether "3 clients most savings account" semantically matches `client_top_n_by_savings_account_count`; it only sees numbers.

## Proposed change

Replace `EvidenceEvaluator` with a `LlmReranker` built on `rig-core`:

```rust
#[derive(Debug, JsonSchema, Deserialize)]
struct RerankerDecision {
    decision: RerankerVerdict,               // Select | Clarify | Unsupported
    capability_id: Option<String>,           // required when Select
    confidence: f32,                         // 0.0-1.0
    alternatives: Vec<String>,               // for Clarify: 2-4 ids to present
    reason: String,
}
```

Input to the LLM: the user message + top-K (K=8) candidates from `RetrievalEngine`, each with `id`, `display_name`, `description`, `examples`, `request_shape`. LLM picks the best match, requests clarification if genuinely ambiguous, or marks unsupported if no candidate fits semantically.

Structured output enforced via `schemars::JsonSchema` derive → `llm::structured::<RerankerDecision>()` (helper already exists at `crates/chat/src/assistant/llm.rs`).

Threshold logic collapses to: `decision == Select && confidence >= 0.6` → execute; else clarify with `alternatives`.

## Files

- New: `crates/chat/src/assistant/reranker.rs` — the LLM re-ranker.
- `crates/chat/src/assistant/runtime/mod.rs:486-548` — swap `EvidenceEvaluator.evaluate(...)` call.
- `crates/chat/src/assistant/evidence.rs` — keep `Evidence` struct, delete `EvidenceEvaluator` + `EvidenceDecision` (moved to reranker).
- `crates/chat/src/assistant/mod.rs` — re-export.
- `crates/chat/tests/assistant_retrieval_evidence.rs` — port tests to reranker (with `TestLlmClient` stub).

## Acceptance criteria

- All 3 failing queries from epic README resolve to correct capability (or `Clarify` with sensible options).
- Reranker never crashes on empty candidate list — returns `Unsupported`.
- Latency budget: +1 LLM call per report_request; measured at < 500ms p95 with Deepseek fast model.
- Structured output validation: malformed LLM response retries once, then fails safe to `Clarify`.

## Test plan

- Unit: fake `LlmClient` returns each decision variant, assert runtime routes correctly.
- Integration: replay the fixture queries from issue 07 (once that lands) — asserts top-1 accuracy ≥ 90%.
- Regression: existing `assistant_retrieval_evidence.rs` scenarios still pass.

## Out of scope

- Multi-turn reranker memory (uses current turn only).
- Caching (issue 07 or later).

## Dependencies

- Issue 01 (semantic-first retrieval) must land first — reranker needs a non-empty candidate list to work with.
- Issue 06 (retrieval trace) recommended before this to make debugging easier.
