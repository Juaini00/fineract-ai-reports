# 002 — Semantic assistant platform major refactor

Status: active — implementation complete in working tree, pending final workspace validation/acceptance
Severity: blocker
Area: chat | runtime | docs | catalog
Created: 2026-07-12
Resolved:

## 2026-07-13 reality check

Previous status text overstated completion. The migration has delivered useful foundation pieces and regression fixes, but the target semantic assistant platform is not complete. Core gaps remain: `petgraph` topology is not the runtime control plane, `rig-core` is not the primary agent/tool boundary, source intent is not persisted as part of clarification state, context carry-over still uses tactical glue in places, and the scenario matrix is not yet a complete acceptance suite. Keep this issue active until those gaps are closed.

## 2026-07-13 Phase 12 update

Working tree now wires guarded selected-capability tool execution, approved-catalog SQL only, office ids bound through policy, structured response as source of truth, markdown as derived rendering, no-router fail-closed behavior, and scenario/golden response-contract coverage. Keep this issue active until the full requested validation set passes in the workspace.

## 2026-07-13 Phase 9 update

Recent work added deterministic scenario coverage and documentation alignment attempts, but it did not complete the migration. The old classifier-first runtime is no longer the desired source of truth, yet the full target assistant brain still needs the architecture work tracked in the spec/plan. Live E2E rows remain gated behind `RUN_LIVE_SCENARIO_MATRIX=1`.

## Problem

The current chat/reporting system is too rigid for the intended product. It behaves like exact deterministic matching over approved cases instead of a semantic assistant that can understand natural language, session context, clarification history, and related in-domain questions.

This is not a small classifier bug. It is an architecture mismatch between the desired assistant behavior and the current runtime flow.

The system often behaves like:

```text
a = a
a != A
```

But the target system must understand that `a` and `A` are semantically the same value unless the business context says otherwise.

## Impact

- In-domain prompts can be rejected or misrouted because they do not match existing capability language closely enough.
- Clarification can loop or become hard for clients/frontends to interpret after multiple rounds.
- Session context is weak, so follow-up questions and clarification replies lose important meaning.
- Greeting and help prompts such as `Hi` or `kamu bisa apa aja?` are treated like unsupported report requests instead of assistant interactions.
- Valid exploratory data questions such as `ada gak nama Tony di client kita?` are not handled as first-class in-domain lookup intents.
- Out-of-domain questions such as `tau gak harga laptop?` are not clearly separated from in-domain assistant/help questions.
- Manual Markdown mapping is becoming a bottleneck for richer responses such as tables, cards, warnings, actions, and future response formats.
- Some documentation describes behavior that the runtime does not actually follow, while some old runtime shortcuts remain even when docs say they should be removed.

## Current behavior

- Job creation enters `classify_with_retrieval` directly before a first-class semantic assistant router.
- Classification still includes deterministic shortcuts and shape checks before broader semantic understanding.
- LQR and strict pipeline pieces exist, but the active runtime is not a coherent assistant graph.
- Clarification matching has improved from literal-only matching, but the deeper issue remains: clarification is not modeled as a semantic state transition with memory.
- Job state stores classification, plan, policy, and pending intent, but job memory is not a complete graph state.
- Session memory exists as raw durable messages, but there is no explicit session context window model with limits, summaries, and relevance selection.
- Responses are rendered mostly through manual Markdown/string formatting, with structured response support only in limited paths.

## Expected behavior

The system should be refactored into a semantic assistant platform for reporting and in-domain operational data access.

The target runtime should support:

- Semantic intent routing before capability execution.
- Clear distinction between greeting, help, report request, data lookup, clarification reply, follow-up, unsafe request, out-of-domain request, and unsupported in-domain request.
- Job memory for the active request graph.
- Session memory through a bounded session context window.
- Raw message history retained for audit/debug.
- Session context warnings when the context window is near or over limit.
- Retrieval-assisted classification and planning, not exact-string capability matching.
- Capability/tool execution guarded by approved catalog, SQL, PII, office scope, and API key permissions.
- Structured response as the source of truth, with Markdown only as one rendering target.
- A documented migration path from the current deterministic runtime to the new assistant platform runtime.

## Refactor goal

This issue exists to trigger a major refactor and migration, not a tactical patch.

The refactor must produce a new source-of-truth spec before implementation. That spec should map:

- Which existing docs remain authoritative.
- Which docs are superseded or deprecated.
- Which current runtime paths are removed, replaced, or preserved behind migration boundaries.
- Whether LangChain/LangGraph Rust, or another Rust AI orchestration framework, becomes part of the official architecture.
- How job memory and session memory are represented and persisted.
- How the assistant graph handles multi-turn flows.
- How response contracts support Markdown, tables, cards, warnings, and future formats.
- How behavior is tested through scenario matrices, not only unit tests.

## Non-goals

- Do not treat this as a small classifier fix.
- Do not frame this as handling only one narrow case first.
- Do not preserve the current deterministic path just because it is already implemented.
- Do not keep documentation as aspirational text if runtime behavior will not follow it.
- Do not let LLMs generate SQL or bypass policy, PII, office-scope, or approved capability boundaries.

## Required design topics

### Assistant routing

The system needs a semantic router that can classify user input into assistant-level intents before selecting executable capabilities.

Minimum intent classes:

- `greeting`
- `help`
- `report_request`
- `data_lookup`
- `clarification_reply`
- `follow_up`
- `unsafe_request`
- `out_of_domain`
- `unsupported_in_domain`

### Memory model

The refactor must define at least two memory levels now:

- Job memory: active request graph state, including parsed intent, route, evidence, clarification attempts, selected capability/tool, resolved params, execution result summary, and response draft.
- Session memory: raw messages plus a bounded context window used by assistant routing and LLM calls.

Global memory can remain out of scope until job and session memory are stable.

### Session context window

Each session needs a bounded context window so the system does not keep forcing the LLM to understand an unbounded chat history.

Expected behavior:

- Keep full raw history in PostgreSQL.
- Build active context from summary, recent turns, relevant job summaries, current clarification state, and selected entities.
- Emit `session_context_near_limit` warning metadata when close to the configured limit.
- Ask the user to start a new session when the hard limit is exceeded.

### Capability and tool execution

The assistant can understand broadly, but execution remains bounded.

Rules:

- LLM may parse, classify, summarize, and propose tool/capability intent.
- Rust validates all tool/capability calls.
- SQL remains catalog-approved only.
- Fineract transactional rows are not indexed into vector storage.
- Office scope, PII, and API key permissions remain mandatory execution gates.

### Client lookup

The target architecture must support in-domain lookup questions such as:

```text
ada gak nama Tony di client kita?
```

This should be modeled as an official `data_lookup` path with appropriate capability, policy, and PII rules, not as generic unsupported text.

### Response contract

Markdown should not be the source of truth.

The refactor must introduce a structured response contract that can represent:

- summary
- table
- metric cards
- clarification options
- unsupported/out-of-domain explanations
- warnings
- suggested next actions
- Markdown rendering

The frontend should be able to render rich responses without parsing Markdown.

### Framework evaluation

Because this is a RAG assistant with multi-turn state, memory, retrieval, and graph-like orchestration, the spec must evaluate whether Rust LangChain/LangGraph or another framework should become part of the official architecture.

The decision should consider:

- async Rust/Tokio compatibility
- graph/state-machine support
- checkpointing and resumability
- tool/capability calling support
- compatibility with PostgreSQL-backed job/session memory
- ability to preserve existing security and SQL guardrails
- operational maturity

## Hardcode removal matrix

The refactor must explicitly remove or relocate current hardcoded behavior. These are not optional cleanup items.

| Current hardcode | Current location | Required replacement |
| --- | --- | --- |
| Greeting/help/out-of-domain keyword routing such as `hi`, `laptop`, `nama`, `client` | deterministic router shortcuts in chat assistant routing | LLM semantic router with JSON schema, catalog context, and deterministic validation of the router output. |
| Savings activity shortcut before semantic routing | `crates/chat/src/chat/service/job.rs::classify_savings_activity_list` | Normal assistant graph route: semantic router -> retrieval planner -> evidence evaluator -> capability/tool execution. |
| Prompt shape term matching | `crates/chat/src/chat/service/job.rs::capability_matches_prompt_shape` and `capability_matches_domain_terms` | Evidence evaluator based on router intent, retrieved catalog evidence, capability metadata, and output contract. |
| Deferred/off-domain cue heuristics | `context_overrides_capability`, `has_off_domain_cue`, and source-id string checks | Assistant router + catalog domain/data-area status evaluation. |
| Manual clarification option matching | `crates/chat/src/chat/pending_intent.rs` and `crates/chat/src/assistant/clarification_resolver.rs` | Semantic clarification graph state backed by LLM structured decision or embedding-assisted semantic matching. |
| Generic response string rendering | `crates/chat/src/chat/formatter/render.rs` | Structured response builder and renderer using `chat::assistant::response`. |
| Savings activity special-case renderer | `crates/chat/src/chat/formatter/activity.rs` | Structured response contract for grouped table/card/aggregation output. |
| Hardcoded context window limits | default context window policy in chat assistant context code | Runtime config under `core::config`, exposed to chat assistant context builder. |
| Exact catalog count assertions | `crates/chat/tests/catalog_endpoint.rs`, `crates/chat/tests/catalog_validation.rs` | Capability/category invariant tests or explicit snapshot tests that are updated through documented catalog versioning. |
| Initial-only assistant memory | `JobService::initial_assistant_memory` | Stage-by-stage assistant memory updates for route, retrieval, clarification, policy, execution, and response. |

## Required migration phases

The follow-up implementation plan must cover these phases explicitly:

1. Foundation boundaries: assistant modules under `crates/chat/src/assistant/**`, response contracts, graph state models.
2. Assistant router replacement: replace deterministic keyword router with LLM semantic router and validated output schema.
3. Memory migration: persist and update job memory across every graph stage; build session context window from actual session history and job summaries.
4. Retrieval and evidence migration: remove direct classifier-first path and replace it with assistant graph retrieval/evidence evaluation.
5. Clarification migration: replace literal/manual clarification resolution with semantic clarification graph state.
6. Capability/tool migration: formalize `data_lookup` and report capabilities as graph tools guarded by existing policy.
7. Response migration: replace generic and activity-specific Markdown generation with structured response builders and renderers.
8. Hardcode cleanup: delete or demote old shortcut functions after replacement paths pass behavior scenarios.
9. Documentation migration: mark superseded docs and keep current status/next work aligned with runtime.
10. Scenario migration: add E2E behavior tests for greeting, help, client lookup, report request, semantic clarification, out-of-domain, unsafe request, and long session context.

## Scenario matrix required before implementation

The future spec must define expected behavior for at least:

| Prompt | Expected intent | Expected behavior |
| --- | --- | --- |
| `Hi` | `greeting` | Friendly assistant response, no report execution. |
| `kamu bisa apa aja?` | `help` | Explain supported reporting/data access areas based on API key scope. |
| `ada gak nama Tony di client kita?` | `data_lookup` | Search/lookup client data if allowed; otherwise explain unavailable scope. |
| `total savings deposit this month` | `report_request` | Route to approved capability and execute guarded SQL. |
| `yang balance aja` after clarification | `clarification_reply` | Resolve semantically against prior options/context. |
| `sekarang tampilkan client aktif bulan ini` after a savings question | `follow_up` or new `report_request` | Detect whether this supersedes prior job or continues context. |
| `tau gak harga laptop?` | `out_of_domain` | Reject as outside product domain. |
| `show raw account numbers` | `unsafe_request` | Reject due to PII/security policy. |
| Very long session | context-window warning/hard cap | Warn or require new session depending on limit. |

## Proposed next document

Create a design spec after this issue is reviewed:

```text
docs/superpowers/specs/2026-07-12-semantic-assistant-platform-migration-design.md
```

That spec should become the source of truth for the major refactor and migration plan.

## Links

- `docs/issues/active/001-clarification-response-matching-must-be-semantic.md`
- `docs/superpowers/specs/2026-07-07-full-rag-blueprint-strict-design.md`
- `docs/superpowers/specs/2026-07-07-rag-lqr-overlay-design.md`
- `docs/current/status.md`
- `docs/current/next-work.md`
- `crates/chat/src/chat/service/job.rs`
- `crates/chat/src/chat/pipeline/`
- `crates/chat/src/chat/formatter/`
