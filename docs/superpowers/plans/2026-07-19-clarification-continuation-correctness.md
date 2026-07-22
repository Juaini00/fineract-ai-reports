# Clarification Continuation Correctness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Correct clarification continuation when clients send both `option_id` and `message`, preserving intent and preventing repeated loops.

**Architecture:** Keep `source_message` as the semantic utterance and treat `selected_option_id` only as a typed control signal. Resolve missing-parameter continuation before generic option handling, and make `others` behavior depend on clarification kind. Preserve existing API and authorization boundaries.

**Tech Stack:** Rust, Axum, SQLx-backed chat state, Tokio tests.

## Global Constraints

- Preserve route → service → repository → database layering.
- Keep the existing request/response schema and same-job clarification endpoint.
- Chat auth and office-scope behavior must not change.
- English-only user-facing copy.
- Use TDD: each production behavior requires a regression test that fails first.

---

### Task 1: Runtime input and Others semantics

**Files:**
- Modify: `crates/chat/src/api/handlers/job.rs:186-215`
- Modify: `crates/chat/src/assistant/execution/runtime/mod.rs:195-320`
- Modify: `crates/chat/src/assistant/execution/runtime/clarification.rs:45-180`
- Test: `crates/chat/src/assistant/execution/runtime/tests.rs`

**Interfaces:**
- Consumes: `RuntimeUserInput { message, source_message, selected_option_id }` and `ClarificationPayload::is_missing_execution_parameters`.
- Produces: deterministic same-turn handling where `message` remains user text and `option_id` remains a discriminator.

- [ ] Add failing runtime tests for `others` with a meaningful new request, `others` with a missing-parameter reply, and a parameter reply longer than six words.
- [ ] Run each test and confirm failure demonstrates the current reset/reroute bug.
- [ ] Change handler/runtime normalization so literal `others` is never used as semantic query text.
- [ ] Resolve missing-parameter continuation before generic `FreeFormOther`; reroute meaningful capability-choice Others text in the same turn.
- [ ] Run focused runtime tests and confirm green.

### Task 2: Conflict, invalid-option, and loop protection

**Files:**
- Modify: `crates/chat/src/assistant/execution/runtime/clarification.rs`
- Modify: `crates/chat/src/assistant/execution/runtime/execution.rs`
- Modify: `crates/chat/src/assistant/execution/runtime/extraction.rs`
- Test: `crates/chat/src/assistant/execution/runtime/tests.rs`
- Test: `crates/chat/src/assistant/execution/tool/tests.rs`

**Interfaces:**
- Consumes: active clarification options, deterministic domain/metric extraction, and payload attempt.
- Produces: explicit mismatch/unavailable clarification with bounded retry; no unauthorized execution.

- [ ] Add failing tests for explicit capability/message conflict, invalid option, and repeated unresolved attempts.
- [ ] Verify the tests fail because the current code either silently selects the id or repeats attempt 1.
- [ ] Reject recognized domain/metric conflicts before execution, increment unresolved attempts, and enter free-text recovery at the bounded ceiling.
- [ ] Keep capability membership and authorization checks authoritative.
- [ ] Run focused runtime/tool tests and confirm green.

### Task 3: End-to-end regression and documentation

**Files:**
- Modify: `crates/chat/tests/chat_no_loop.rs`
- Modify: `docs/current/chat-client-integration.md:220-245`
- Modify: `docs/current/status.md`

**Interfaces:**
- Consumes: unchanged `POST /chat/jobs/{job_id}/responses` schema.
- Produces: transcript-level proof and documented dual-field semantics.

- [ ] Add an integration regression that posts both fields for Others and verifies the next assistant response is not the identical clarification.
- [ ] Add coverage showing the original `this month` range survives capability selection and only missing limit is requested.
- [ ] Run the integration regression, then `cargo test -p chat --lib`, `cargo test -p chat`, `cargo check`, and `cargo fmt --check`.
- [ ] Update integration and current-status documentation with the verified behavior.
