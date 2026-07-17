# 06 — Persist retrieval trace to `state_json`

**Parent:** [Epic](./README.md) · **Priority:** P1 · **Effort:** S (~50 LoC)

## Problem

Debugging today's "unsupported" responses required:
1. Adding `tracing::info!` calls in `runtime/mod.rs`.
2. Rebuilding + restarting the server.
3. Replaying the query while tailing logs.

The `chat_jobs.state_json` for the failed job only contains `input`, `client`, and `classification.runtime`. `retrieval_plan`, `intent`, `evidence`, `decision` all live in `JobMemory` fields but are not persisted to `state_json` for inspection after the job completes.

## Proposed change

After each retrieval pass, mirror the key mapping decisions into `state_json.retrieval_trace`:

```json
{
  "retrieval_trace": {
    "router_intent": { "intent": "...", "domain": "...", "request_shape": {...}, "confidence": 0.95 },
    "plan": { "query_text": "...", "allowed_capability_count": 26 },
    "candidates": [
      { "capability_id": "...", "cosine": 0.82, "shape_score": 0.6, "final": 0.71 }
    ],
    "decision": { "kind": "select|clarify|unsupported", "capability_id": "...", "confidence": 0.87, "reason": "..." }
  }
}
```

Write happens in `runtime/mod.rs` just before `graph_result(...)` is called for report_request paths. Reuse existing `memory.retrieval_plan`, `memory.retrieval_evidence`, `memory.evidence_decision` fields — flatten them into the persistable trace.

## Files

- `crates/chat/src/assistant/runtime/mod.rs` — build trace, assign to `state_json.retrieval_trace` via `JobMemory` helper.
- `crates/chat/src/assistant/mod.rs` (JobMemory) — add `retrieval_trace: serde_json::Value` field, wire into state serialization.
- `crates/chat/src/chat/repository.rs` — no schema change (state_json is JSONB).

## Acceptance criteria

- `GET /chat/jobs/{id}` returns `state_json.retrieval_trace` populated for every report_request.
- Debugging next unsupported response requires zero code changes — just read the trace.
- Trace size stays under 8KB p95 (top-10 candidates only).
- Sensitive PII from candidate descriptions is not leaked into trace (descriptions are catalog metadata, safe).

## Test plan

- Integration: submit a report_request, poll job, assert `state_json.retrieval_trace.decision.kind` matches expected.

## Out of scope

- Persisting full LLM prompts/responses. That's a separate compliance concern.
- UI to visualize traces. JSON is enough for backend debugging.

## Dependencies

- Landable in parallel with issue 04. Ideally before issue 02 so reranker output shape can be validated end-to-end without log tailing.
