# Semantic Assistant Platform Migration Design

## Executive goal

Build the target assistant brain for chat-driven reporting and approved in-domain operational data access.

The assistant is semantic at the understanding boundary and deterministic at the execution boundary: LLMs may classify, extract, explain, and propose; Rust owns auth, policy, office scope, PII, approved SQL, tool validation, execution, DB writes, checkpoints, and response contracts.

This is not complete today. Current runtime has useful foundation pieces plus tactical patches. This document is the target architecture that replaces those bridges.

## Source issue

- `docs/issues/active/002-semantic-assistant-platform-major-refactor.md`

## Hard constraints

- Workspace stays exactly three crates: `crates/app`, `crates/core`, `crates/chat`.
- New assistant modules live under `crates/chat/src/assistant/**` and call existing chat modules under `crates/chat/src/chat/**`.
- No new `api`, `infra`, `runtime`, `knowledge`, `reporting`, or long-named crates.
- No LLM-generated SQL.
- No Fineract transactional rows in vector storage.
- No transactional Fineract rows stored in Swiftide nodes or pgvector embeddings.
- Approved SQL remains in `queries/**/*.sql`; capability metadata remains in `knowledge/**/*.yaml`.
- Office scope is enforced inside approved SQL through bound `office_ids`; never post-filtered in Rust.
- API key, bearer user, capability, office, and PII policy gates remain Rust-owned.
- Route → service → repository → database layering remains. Repositories own SQLx.
- HTTP responses keep the `{ success, data, error }` envelope and sanitized `ApiError`.
- Tactical deterministic shortcuts are migration-only and must have deletion gates before acceptance.

## Existing libraries and ownership

| Library | Version | Target role | Runtime ownership rule |
| --- | ---: | --- | --- |
| `schemars` | `1.2.1` | Canonical JSON schema layer for every LLM request/response, tool contract, graph state payload, structured response, golden fixture, and traceable contract. | All LLM/tool/response structs derive `JsonSchema` where serialized across assistant boundaries. |
| `rig-core` | `0.40.0` | Primary structured LLM, agent, and tool boundary. Used for intent extraction, constraint/entity extraction, clarification tie-breaks, safe prose grounded on structured data, and provider abstraction. | Node code depends on project traits wrapping rig; no direct provider calls in graph nodes. |
| `petgraph` | `0.8.3` | Graph topology/control plane: allowed transitions, terminal-state validation, checkpoint graph, scenario coverage map, and debug visualization. | Execution may remain match-based, but every transition must be validated against the petgraph topology. |
| `swiftide` | `0.32.1` | Offline knowledge ingestion/indexing pipeline for YAML/SQL/docs/capability snippets into normalized searchable artifacts. | Not runtime SQL execution, not transactional row indexing, not a policy bypass. Runtime reads curated indexes through repositories. |

## Target runtime flow

```text
POST /chat/sessions/{id}/jobs or /jobs/{id}/responses
  -> chat api handler validates auth + ValidatedJson
  -> JobService loads session/job/client context
  -> assistant::runtime::GraphRuntime starts/resumes job
     -> receive_message
     -> build_context_window
     -> route_intent                 (rig-core structured output + schemars)
     -> resolve_clarification         (if pending; preserves source intent)
     -> plan_retrieval
     -> retrieve_knowledge            (catalog/vector/FTS/relation metadata)
     -> evaluate_evidence
     -> plan_tool_or_capability
     -> guard_execution               (Rust policy/auth/PII/office scope)
     -> execute_tool_or_sql           (approved SQL or Rust tool only)
     -> build_structured_response     (schemars contract; optional rig prose)
     -> render_response               (Markdown derived from structure)
     -> complete_or_wait
  -> repositories persist memory/checkpoints/events/traces
  -> API returns envelope with structured response + rendered markdown
```

The graph is the only primary runtime path once migration is accepted. Old classifier-first and formatter-first paths may exist only in quarantined migration code until their deletion gate passes.

## Module layout

Only existing crates are used.

```text
crates/chat/src/assistant/
  mod.rs
  contracts.rs              # schemars contract re-exports and schema helpers
  intent.rs                 # AssistantIntent, entities, constraints, domains
  response.rs               # AssistantResponse, render targets, warnings, actions
  memory.rs                 # JobMemory, SessionMemory, PendingClarification
  context.rs                # ContextWindow and budget policy
  llm.rs                    # LlmClient trait, RigLlmClient, TracedLlmClient
  graph.rs                  # GraphState, TerminalState, transition topology
  runtime/mod.rs            # GraphRuntime, nodes, checkpoint loop
  runtime/nodes/*.rs        # one node per graph state
  retrieval.rs              # RetrievalPlan, Evidence, EvidenceEvaluator
  clarification.rs          # semantic resolver preserving source intent
  tool.rs                   # ToolRequest, ToolResult, typed tool contracts
  renderer.rs               # ResponseRenderer, MarkdownRenderer
  swiftide_index.rs         # offline indexing adapter only
  repositories/*.rs         # assistant memory/checkpoint/trace repos

crates/chat/src/chat/
  service/job.rs            # composition entrypoint into assistant runtime
  repository/*.rs           # existing chat persistence
  planner/executor/policy    # reused where execution is already guarded
```

## Schemars contracts

Every assistant boundary type derives `Serialize`, `Deserialize`, `JsonSchema`, and has golden fixtures:

- `AssistantIntent`
- `AssistantEntity`
- `AssistantConstraints`
- `Quantity`, date/currency/product filters
- `ContextReference`
- `SourceIntentSnapshot`
- `PendingClarification`
- `RetrievalPlan`, `Evidence`, `EvidenceDecision`
- `ToolRequest`, `ToolResult`
- `GraphState`, `TerminalState`, `GraphTransition`
- `AssistantResponse`, table/card/section/warning/action subtypes
- LLM trace request/response metadata

Schemas are generated in tests and compared against committed snapshots. Breaking schema changes require an intentional snapshot update.

## Rig-core boundary and tracing

Graph nodes do not call providers directly. They depend on:

```rust
pub trait LlmClient: Send + Sync {
    async fn structured<T: JsonSchema + DeserializeOwned>(
        &self,
        purpose: LlmPurpose,
        system: &str,
        user: &str,
    ) -> Result<LlmResponse<T>>;

    async fn embed(&self, purpose: LlmPurpose, text: &str) -> Result<EmbeddingResponse>;
}
```

`RigLlmClient` is the primary implementation. `TracedLlmClient` wraps it and records every call to `assistant_llm_traces` with job id, session id, API key id, graph state, purpose, provider, model, token usage, cost, latency, status, and sanitized error kind. Missing pricing produces `cost_usd = null`, not request failure.

Allowed LLM purposes:

- `route_intent`
- `extract_entities`
- `resolve_clarification`
- `summarize_session`
- `ground_response_prose`
- `embed_query`
- `embed_option`

LLM malformed output fails closed with a sanitized operational error. There is no keyword fallback in the accepted runtime.

## Petgraph graph model

`assistant::graph` owns a `petgraph::Graph<GraphState, TransitionRule>` plus maps for state lookup. Runtime transition requests are validated before memory/checkpoint writes.

### States

```text
receive_message
build_context_window
route_intent
resolve_clarification
plan_retrieval
retrieve_knowledge
evaluate_evidence
plan_tool_or_capability
guard_execution
execute_tool_or_sql
build_structured_response
render_response
complete_or_wait
```

### Terminal states

```text
completed
waiting_for_user_input
unsupported_in_domain
out_of_domain
blocked_by_policy
context_window_exceeded
failed_operational
```

### Transition rules

- `receive_message -> build_context_window`
- `build_context_window -> route_intent | context_window_exceeded`
- `route_intent -> resolve_clarification | plan_retrieval | out_of_domain | blocked_by_policy | failed_operational`
- `resolve_clarification -> plan_retrieval | waiting_for_user_input | route_intent | failed_operational`
- `plan_retrieval -> retrieve_knowledge`
- `retrieve_knowledge -> evaluate_evidence`
- `evaluate_evidence -> plan_tool_or_capability | waiting_for_user_input | unsupported_in_domain | out_of_domain`
- `plan_tool_or_capability -> guard_execution | waiting_for_user_input | unsupported_in_domain`
- `guard_execution -> execute_tool_or_sql | blocked_by_policy`
- `execute_tool_or_sql -> build_structured_response | failed_operational`
- `build_structured_response -> render_response`
- `render_response -> complete_or_wait`
- `complete_or_wait -> completed | waiting_for_user_input`

Every accepted transition writes a checkpoint and memory delta. Illegal transitions are programmer errors caught by tests and fail closed at runtime.

## Swiftide offline indexing

Swiftide is used only for offline ingestion/index preparation:

- Load `knowledge/**/*.yaml`, `queries/**/*.sql`, and selected docs.
- Chunk and normalize capability/query/domain/schema/metric/policy/response snippets.
- Attach metadata: source path, source type, capability id, query id, domain, data area, status, PII flags, office-scope requirement, schema version.
- Deduplicate and prepare embeddable documents.
- Write curated artifacts through repository code into existing knowledge index tables or a migration-approved assistant index table.

Swiftide never executes SQL, never sees live Fineract transactional rows, and never decides policy.

## Memory model

### Job memory

`assistant_job_memory` stores the active graph state for one job:

- job id, session id, revision
- current graph state and terminal state
- current user message metadata
- `AssistantIntent`
- `SourceIntentSnapshot`
- retrieval plan/evidence/decision
- selected capability/tool and params
- policy decision summary
- execution summary, never raw hidden PII
- `AssistantResponse`
- warnings and failure kind
- timestamps

### Session memory

`assistant_session_memory` stores derived session context:

- session id, revision
- rolling summary
- active domain/topic
- selected entities
- relevant prior job summaries
- pending clarification with source intent snapshot
- context budget warnings

Raw messages remain in `chat_messages` for audit. Session memory is derived and rebuildable.

### Context window

Built per request from:

- current user message
- session summary
- recent messages under budget
- pending clarification and source intent
- relevant prior job summaries
- client/API key/office capability scope
- retrieved catalog hints when appropriate

Soft budget emits `session_context_near_limit`; hard budget returns `context_window_exceeded` and asks for a new session. Limits come from config.

## Clarification and source-intent preservation

Clarification is a graph state, not a string matcher.

When the assistant asks a clarification, it persists:

```json
{
  "question": "Which report should I use?",
  "options": [],
  "source_intent": {
    "intent": "report_request",
    "domain": "client",
    "entities": [],
    "constraints": {
      "quantity": { "mode": "top_n", "value": 10 }
    },
    "context_reference": "none"
  }
}
```

On response, the resolver uses explicit `option_id` first, semantic similarity second, and rig structured tie-break third. It merges selected option + source intent constraints/entities/context. It never reconstructs a blank intent and never relies on token containment as the primary runtime decision.

Outcomes:

- `selected_option`
- `refined_constraints`
- `new_request`
- `free_form_other`
- `cancelled`
- `unresolved`

## Retrieval and evidence

Retrieval input is `AssistantIntent + ContextWindow`, not raw prompt alone.

Sources:

- capability metadata
- approved query metadata
- domain/data-area docs
- schema/metric definitions
- policies and PII rules
- response templates/contracts

Modes:

- vector search over curated knowledge artifacts
- Postgres FTS
- catalog relationship traversal
- metadata filters
- weighted merge

`EvidenceEvaluator` decides:

- strong evidence → selected capability/tool
- weak or conflicting evidence → clarification
- in-domain but missing approved capability → `unsupported_in_domain`
- outside product domain → `out_of_domain`
- unsafe/PII request → `blocked_by_policy` or `unsafe_request`

## Tool and capability execution

LLM output can propose a tool or capability. Rust validates all execution:

- capability exists and is approved
- API key is permitted
- bearer user matches API key owner where required
- office ids are expanded and bound into SQL
- PII access is allowed before fields are included
- params validate against schemars/validator contracts
- SQL comes from approved files only
- execution writes only through repository/service code with existing transaction rules

Client lookup is a first-class `data_lookup` path with approved capability/query metadata and ambiguity-aware responses.

## Structured response

`AssistantResponse` is authoritative; Markdown is derived.

Response kinds:

- `summary`
- `table`
- `metric_cards`
- `clarification`
- `help`
- `unsupported`
- `out_of_domain`
- `policy_blocked`
- `error`

Response fields include title, message, sections, table columns/rows, cards, options, warnings, actions, evidence references, and rendered markdown. Safe prose may use rig only when grounded on structured data and evidence. Hidden PII fields are removed before response assembly.

## Failure modes

| Failure | Behavior |
| --- | --- |
| LLM unavailable | Sanitized operational failure; no keyword fallback. |
| LLM malformed JSON | Fail closed, trace as malformed. |
| Illegal graph transition | Fail closed, checkpoint error event. |
| Weak evidence | Clarification or unsupported in-domain. |
| Out-of-domain request | Clear out-of-domain response; no SQL. |
| Unsafe/PII request | Policy blocked response; no SQL when unsafe. |
| Context hard limit | Ask user to start a new session. |
| Capability denied | Policy blocked. |
| Approved SQL error | Sanitized execution failure. |
| Swiftide/offline index unavailable | Runtime uses last valid index or fails retrieval gracefully; never executes unindexed SQL. |

## Deletion and quarantine matrix

| Legacy/tactical behavior | Required target | Gate |
| --- | --- | --- |
| Deterministic keyword router | `assistant::llm::RigLlmClient` + `AssistantIntent` schema | no primary runtime hits |
| Prompt-shape/domain-term capability matching | retrieval + evidence evaluator | scenario matrix passes |
| Off-domain cue string checks | router/evidence domain decision | golden out-of-domain rows pass |
| Manual clarification scoring | source-intent resolver | option-id and semantic reply tests pass |
| Reconstructing intent from selected option only | merge selected option with persisted source intent | quantity/date/entity preservation tests pass |
| Manual Markdown formatter as source of truth | `AssistantResponse` + renderer | API returns structured response |
| Hardcoded context limits | config-driven context policy | config tests pass |
| Initial-only assistant memory | transition-by-transition memory deltas | checkpoint/resume tests pass |
| Tactical fallback/manual glue | quarantine behind migration-only feature or delete | no accepted runtime dependency |

## Acceptance tests and scenario matrix

Required test layers:

1. Contract tests: schemars snapshots for all assistant boundary types.
2. Graph tests: every legal transition exists; illegal transitions fail; checkpoint/resume works.
3. LLM boundary tests: rig wrapper records traces and fails closed on malformed output.
4. Retrieval/evidence tests: strong, weak, unsupported, out-of-domain, unsafe.
5. Clarification tests: option id, label, semantic reply, free-form other, new request, source-intent merge.
6. Tool execution tests: policy pass, policy block, office-scope SQL params, PII filtering.
7. API scenario tests through chat endpoints.

Scenario matrix:

| Prompt | Expected |
| --- | --- |
| `Hi` | greeting/help response, no SQL. |
| `kamu bisa apa aja?` | help scoped to API key capabilities. |
| `ada gak nama Tony di client kita?` | `data_lookup` client path or scoped unavailable response. |
| `show 10 clients with the most savings accounts` | report request; limit `10` preserved through clarification. |
| `total savings deposit this month` | approved report execution. |
| Ambiguous client/savings prompt | `waiting_for_user_input` with actionable options. |
| `client_top_n_by_savings_balance` as `option_id` | same job continues and executes selected capability. |
| `yang balance aja` after clarification | semantic selected option without literal-only matching. |
| `others` then free text | does not repeat identical options forever. |
| Follow-up changing domain | detected as follow-up or new request using context. |
| `tau gak harga laptop?` | out-of-domain, no SQL. |
| `show raw account numbers` | unsafe/policy blocked. |
| Long session near soft limit | warning emitted. |
| Long session past hard limit | asks for new session. |

## Completion criteria

Migration is complete only when:

- Docs no longer claim completion before runtime matches this target.
- `schemars` contracts cover every assistant LLM/tool/response boundary.
- `rig-core` is the primary runtime structured LLM/tool boundary.
- `petgraph` validates graph topology and transitions/checkpoints.
- Swiftide is limited to offline knowledge ingestion/indexing.
- Clarification persists and merges source intent.
- Job/session/context memory is persisted and used by runtime.
- Retrieval/evidence selects capability/tool or asks clarification.
- All execution remains Rust-owned and approved-SQL-only.
- Structured responses are authoritative.
- Tactical fallback/manual glue is deleted or quarantined outside the primary runtime.
- Full scenario/golden acceptance suite passes.
