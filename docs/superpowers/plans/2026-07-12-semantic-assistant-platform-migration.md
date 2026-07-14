# Semantic Assistant Platform Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the current partial assistant foundation into the full target semantic assistant brain using the existing three crates and added libraries: `rig-core`, `schemars`, `petgraph`, and `swiftide`.

**Architecture:** `crates/chat/src/assistant/**` becomes the assistant brain inside the existing three-crate workspace. `schemars` defines every boundary contract, `rig-core` powers structured LLM/tool interactions behind project traits, `petgraph` validates graph topology/transitions/checkpoints, and `swiftide` is limited to offline knowledge ingestion/indexing.

**Tech Stack:** Rust 2024, Axum, Tokio, SQLx/PostgreSQL, Redis live state, pgvector/catalog retrieval, `rig-core`, `schemars`, `petgraph`, `swiftide`, serde, validator.

**Source spec:** `docs/superpowers/specs/2026-07-12-semantic-assistant-platform-migration-design.md`  
**Source issue:** `docs/issues/active/002-semantic-assistant-platform-major-refactor.md`  
**Branch:** `semantic-assistant-platform-migration`

## Global Constraints

- [ ] Keep exactly three crates: `crates/app`, `crates/core`, `crates/chat`.
- [ ] Put new assistant code under `crates/chat/src/assistant/**` and wire through `crates/chat/src/chat/**`.
- [ ] Do not add commit/push/release steps.
- [ ] No LLM-generated SQL.
- [ ] No transactional Fineract rows in vector storage or Swiftide artifacts.
- [ ] Rust owns auth, policy, office scope, PII, approved SQL, DB writes, and execution.
- [ ] Runtime tactical shortcuts require deletion/quarantine gates.

---

## Phase 0 — Docs reality alignment

**Outcome:** Current docs stop claiming completion and point to this target plan.

**Files:**
- `docs/current/status.md`
- `docs/current/next-work.md`
- `docs/issues/active/002-semantic-assistant-platform-major-refactor.md`
- `docs/superpowers/specs/2026-07-12-semantic-assistant-platform-migration-design.md`
- `docs/superpowers/plans/2026-07-12-semantic-assistant-platform-migration.md`

**Tasks:**
- [ ] Replace completion claims with: foundation implemented, migration incomplete.
- [ ] Record current gaps: petgraph not control plane, rig-core not primary runtime boundary, source intent not preserved cleanly, scenario matrix incomplete, fallback glue still primary in places.
- [ ] Replace “final Phase 9 gate” with the implementation sequence below.
- [ ] Remove references to non-existent assistant crates/paths.

**Gate:**
- [ ] Content check finds no forbidden path references or completion claims.

---

## Phase 1 — Schema and migrations

**Outcome:** Durable assistant memory, graph checkpoints, LLM traces, and optional offline index metadata exist.

**Files:**
- `migrations/<next>_assistant_brain_memory.sql`
- `crates/chat/src/assistant/repositories/job_memory.rs`
- `crates/chat/src/assistant/repositories/session_memory.rs`
- `crates/chat/src/assistant/repositories/checkpoint.rs`
- `crates/chat/src/assistant/repositories/llm_trace.rs`
- `crates/chat/src/assistant/repositories/mod.rs`

**Tasks:**
- [ ] Add `assistant_job_memory` with graph state, terminal state, intent, source intent, evidence, selected capability/tool, policy decision, execution summary, structured response, warnings, revision, timestamps.
- [ ] Add `assistant_session_memory` with summary, active domain, entities, pending clarification including source intent, relevant jobs, revision.
- [ ] Add `assistant_graph_checkpoints` with job id, previous/current state, transition, memory revision, event metadata.
- [ ] Add `assistant_llm_traces` with provider/model/purpose/tokens/cost/latency/status/error kind.
- [ ] Add indexes for graph state, terminal state, intent domain, selected capability, traces by API key/time/provider/purpose.
- [ ] Add repositories with optimistic revision checks.

**Gate:**
- [ ] Migrations run on a fresh dev DB.
- [ ] Repository tests cover insert/update/revision conflict/readback.

---

## Phase 2 — Assistant contract types

**Outcome:** Schemars is the canonical contract layer for assistant boundaries.

**Files:**
- `crates/chat/src/assistant/contracts.rs`
- `crates/chat/src/assistant/intent.rs`
- `crates/chat/src/assistant/context.rs`
- `crates/chat/src/assistant/memory.rs`
- `crates/chat/src/assistant/response.rs`
- `crates/chat/src/assistant/tool.rs`
- `crates/chat/src/assistant/mod.rs`
- `crates/chat/tests/assistant_contracts.rs`
- `tests/golden/assistant_scenarios.jsonl`

**Tasks:**
- [ ] Define `AssistantIntent`, domain/kind enums, entities, constraints, quantity, context references.
- [ ] Define `SourceIntentSnapshot` and `PendingClarification`.
- [ ] Define `ContextWindow`, context warnings, source snippets, relevant jobs.
- [ ] Define `JobMemory`, `SessionMemory`, memory deltas.
- [ ] Define `ToolRequest`, `ToolResult`, typed params, validation errors.
- [ ] Define `AssistantResponse` with table/card/section/options/warnings/actions/evidence references.
- [ ] Derive `JsonSchema` for all boundary structs.
- [ ] Add schema snapshot tests and golden fixture format.

**Gate:**
- [ ] `cargo test -p chat assistant_contracts` passes.

---

## Phase 3 — Rig-core provider boundary and traces

**Outcome:** Rig-core is the primary runtime LLM/structured/tool boundary behind project traits.

**Files:**
- `crates/core/src/config.rs`
- `crates/chat/src/assistant/llm.rs`
- `crates/chat/src/assistant/runtime/nodes/route_intent.rs`
- `crates/chat/src/assistant/runtime/nodes/resolve_clarification.rs`
- `crates/chat/src/assistant/runtime/nodes/build_structured_response.rs`
- `crates/chat/tests/assistant_llm_boundary.rs`

**Tasks:**
- [ ] Add LLM/embedding config: provider, model, base URL, API key, timeout, retries, pricing.
- [ ] Implement `LlmClient` trait.
- [ ] Implement `RigLlmClient` with structured output and embedding calls.
- [ ] Implement `TracedLlmClient` decorator writing `assistant_llm_traces` for success/malformed/timeout/error.
- [ ] Fail closed on malformed JSON; do not fall back to keyword routing.
- [ ] Add fake `LlmClient` for deterministic tests.

**Gate:**
- [ ] Tests prove traces are recorded for every call path.
- [ ] Grep finds no direct provider calls in graph nodes.

---

## Phase 4 — Petgraph topology and checkpointed runtime

**Outcome:** Petgraph validates the assistant graph topology and all runtime transitions.

**Files:**
- `crates/chat/src/assistant/graph.rs`
- `crates/chat/src/assistant/runtime/mod.rs`
- `crates/chat/src/assistant/runtime/nodes/mod.rs`
- `crates/chat/src/assistant/runtime/nodes/*.rs`
- `crates/chat/tests/assistant_graph_runtime.rs`

**Tasks:**
- [ ] Define `GraphState`, `TerminalState`, `TransitionRule`.
- [ ] Build a `petgraph::Graph<GraphState, TransitionRule>` with the spec transitions.
- [ ] Validate transitions before checkpoint writes.
- [ ] Implement match-based node execution with petgraph as control plane.
- [ ] Persist memory deltas and checkpoints after each accepted transition.
- [ ] Add resume-from-checkpoint test.
- [ ] Add illegal-transition test.

**Gate:**
- [ ] Runtime graph tests pass and render/debug topology can be inspected in test output or snapshot.

---

## Phase 5 — Source intent and clarification preservation

**Outcome:** Clarification keeps original constraints/entities/context and merges them with selected options.

**Files:**
- `crates/chat/src/assistant/clarification.rs`
- `crates/chat/src/assistant/runtime/nodes/resolve_clarification.rs`
- `crates/chat/src/assistant/runtime/nodes/evaluate_evidence.rs`
- `crates/chat/src/api/dto/job.rs`
- `crates/chat/src/api/handlers/job.rs`
- `crates/chat/tests/chat_full_flow.rs`

**Tasks:**
- [ ] Store `source_intent` in pending clarification payload.
- [ ] Accept and prioritize explicit `option_id` in `/responses`.
- [ ] Resolve labels/semantic replies with embeddings plus rig tie-break.
- [ ] Merge selected capability/tool with source intent constraints/entities/date/currency/quantity.
- [ ] Remove after-the-fact prompt parsing from primary runtime.
- [ ] Preserve same job id for clarification continuation.

**Gate:**
- [ ] Scenario preserves `limit=10`, dates, currency, and entities across clarification.
- [ ] “others then free text” does not loop identical options.

---

## Phase 6 — Session context window

**Outcome:** Runtime uses explicit bounded session context, not raw history or ad-hoc extraction.

**Files:**
- `crates/chat/src/assistant/context.rs`
- `crates/chat/src/assistant/runtime/nodes/build_context_window.rs`
- `crates/chat/src/assistant/repositories/session_memory.rs`
- `crates/chat/src/chat/service/job.rs`
- `crates/chat/tests/assistant_context_window.rs`

**Tasks:**
- [ ] Build context from summary, recent messages, relevant jobs, pending clarification, source intent, entities, and client scope.
- [ ] Add configurable soft/hard token or character budgets.
- [ ] Emit `session_context_near_limit` warning.
- [ ] Return `context_window_exceeded` at hard cap.
- [ ] Update session memory after completed jobs and clarification changes.

**Gate:**
- [ ] Tests cover soft warning, hard cap, relevant prior job inclusion, and source intent availability.

---

## Phase 7 — Swiftide offline knowledge pipeline

**Outcome:** Swiftide prepares curated knowledge artifacts offline; runtime remains repository-owned.

**Files:**
- `crates/chat/src/assistant/swiftide_index.rs`
- `crates/chat/src/knowledge/index/**`
- `crates/chat/tests/assistant_swiftide_index.rs`
- `docs/runtime/README.md`

**Tasks:**
- [ ] Ingest `knowledge/**/*.yaml`, `queries/**/*.sql`, and selected docs.
- [ ] Chunk, deduplicate, tag metadata, and prepare embeddable documents.
- [ ] Store only capability/query/domain/schema/metric/policy/response artifacts.
- [ ] Reject transactional row ingestion by design and test.
- [ ] Document rebuild command and failure behavior.

**Gate:**
- [ ] Offline pipeline test indexes fixtures and contains no Fineract row data.

---

## Phase 8 — Retrieval and evidence

**Outcome:** Router/context drive retrieval; evidence chooses select/clarify/unsupported/out-of-domain.

**Files:**
- `crates/chat/src/assistant/retrieval.rs`
- `crates/chat/src/assistant/runtime/nodes/plan_retrieval.rs`
- `crates/chat/src/assistant/runtime/nodes/retrieve_knowledge.rs`
- `crates/chat/src/assistant/runtime/nodes/evaluate_evidence.rs`
- `crates/chat/src/knowledge/catalog/**`
- `crates/chat/tests/assistant_retrieval_evidence.rs`

**Tasks:**
- [ ] Create `RetrievalPlan`, `Evidence`, `EvidenceDecision`.
- [ ] Query vector, FTS, catalog relationships, and metadata filters.
- [ ] Score strong/weak/conflicting/no evidence.
- [ ] Return clarification for weak evidence.
- [ ] Return `unsupported_in_domain` for missing approved capability.
- [ ] Return `out_of_domain` without SQL for unrelated requests.

**Gate:**
- [ ] Evidence tests cover all decisions.
- [ ] Legacy prompt-shape helpers are not used by the primary runtime.

---

## Phase 9 — Tool execution and client lookup

**Outcome:** Reports and data lookup execute only through guarded graph tool nodes.

**Files:**
- `crates/chat/src/assistant/runtime/nodes/plan_tool_or_capability.rs`
- `crates/chat/src/assistant/runtime/nodes/guard_execution.rs`
- `crates/chat/src/assistant/runtime/nodes/execute_tool_or_sql.rs`
- `crates/chat/src/assistant/tool.rs`
- `crates/chat/src/policy/authorization.rs`
- `knowledge/capabilities/client/name_lookup.yaml`
- `knowledge/queries/client/name_lookup.yaml`
- `queries/client/name_lookup.sql`
- `crates/chat/tests/chat_full_flow.rs`

**Tasks:**
- [ ] Map evidence-selected capability to typed `ToolRequest`.
- [ ] Reuse existing policy guard before execution.
- [ ] Bind expanded office ids into SQL.
- [ ] Filter PII before response building.
- [ ] Add approved client name lookup capability/query/SQL.
- [ ] Return ambiguity-aware client lookup response.

**Gate:**
- [ ] Allowed request executes.
- [ ] Denied capability returns `blocked_by_policy`.
- [ ] Client lookup never leaks hidden PII.

---

## Phase 10 — Structured responses

**Outcome:** `AssistantResponse` is authoritative for every assistant output.

**Files:**
- `crates/chat/src/assistant/response.rs`
- `crates/chat/src/assistant/renderer.rs`
- `crates/chat/src/assistant/runtime/nodes/build_structured_response.rs`
- `crates/chat/src/assistant/runtime/nodes/render_response.rs`
- `crates/chat/src/api/dto/job.rs`
- `crates/chat/tests/assistant_response.rs`

**Tasks:**
- [ ] Build response from intent, evidence, policy, execution result, and warnings.
- [ ] Use rig only for grounded safe prose.
- [ ] Derive Markdown from structured response.
- [ ] Expose structured response in job DTOs.
- [ ] Delete/quarantine formatters that are source-of-truth string builders.

**Gate:**
- [ ] API tests assert structured JSON and rendered markdown.
- [ ] Hidden PII is absent from tables/cards/messages.

---

## Phase 11 — Deletion of legacy glue

**Outcome:** Primary runtime has no tactical deterministic bridges.

**Files:**
- `crates/chat/src/chat/service/job.rs`
- `crates/chat/src/chat/pending_intent.rs`
- `crates/chat/src/assistant/clarification_resolver.rs`
- `crates/chat/src/chat/formatter/**`
- `crates/chat/tests/**`
- `docs/superpowers/plans/2026-07-12-DELETIONS.md`

**Tasks:**
- [ ] Delete or quarantine deterministic router shortcuts.
- [ ] Delete prompt-shape/domain-term matching from primary runtime.
- [ ] Delete manual clarification scoring from primary runtime.
- [ ] Delete formatter-first response paths.
- [ ] Remove exact catalog count assertions; replace with invariant/snapshot tests.
- [ ] Record each removed shortcut in `DELETIONS.md`.

**Gate:**
- [ ] Content grep finds no primary runtime references to deleted identifiers.
- [ ] Quarantined migration-only code is feature-gated and not default.

---

## Phase 12 — Scenario and golden acceptance

**Outcome:** The target brain is proven by executable scenarios.

**Files:**
- `tests/golden/assistant_scenarios.jsonl`
- `crates/chat/tests/scenario_matrix.rs`
- `crates/chat/tests/chat_full_flow.rs`
- `.github/workflows/ci.yml` if CI already runs chat tests
- `docs/current/status.md`
- `docs/current/next-work.md`
- `docs/issues/active/002-semantic-assistant-platform-major-refactor.md`

**Tasks:**
- [ ] Cover greeting, help, client lookup, report request, ambiguous clarification, selected option, semantic reply, others/free-text, follow-up domain change, out-of-domain, unsafe request, soft/hard context limit.
- [ ] Add golden assertions for intent/domain/entities/constraints/response type.
- [ ] Add optional live LLM scenario mode gated by env.
- [ ] Add deterministic fake LLM scenario mode for CI.
- [ ] Update docs to say complete only after these gates pass.

**Gate:**
- [ ] `cargo fmt`
- [ ] `cargo check --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo test -p chat scenario_matrix`
- [ ] Golden accuracy floor passes.
- [ ] Docs match runtime state.

---

## Final completion checklist

- [ ] No forbidden crate/path references in docs.
- [ ] No docs claim completion before runtime acceptance.
- [ ] Rig-core is the primary LLM/tool boundary.
- [ ] Schemars contracts cover all assistant boundaries.
- [ ] Petgraph validates runtime transitions.
- [ ] Swiftide is offline ingestion only.
- [ ] Source intent survives clarification.
- [ ] Structured responses are authoritative.
- [ ] Rust-only execution guard remains intact.
- [ ] Legacy glue is deleted or quarantined outside default runtime.
- [ ] Scenario/golden acceptance passes.
