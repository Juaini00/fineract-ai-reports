# Agentic workflow runtime — design spec

Issue: `docs/issues/active/012-agentic-workflow-runtime-and-framework-completion.md`
Status: design settled — implementation plan is `docs/superpowers/plans/2026-08-04-agentic-workflow-runtime.md` (to be written)
Created: 2026-08-04
Owner area: chat | runtime | catalog | datasets | retrieval | security

This spec settles contracts and migration boundaries. It does not restate issue 012's
motivation. Where the issue left a choice open, this spec closes it (§2). Where the
issue stated a requirement, this spec turns it into a typed contract and names the file
that must own it. §19 traces every one of issue 012's 13 done-conditions to the section
that discharges it.

### 0.1 Relationship to issues 002, 003, 005, 009

Those issues stay as historical contracts. This spec does not weaken any of their
security or durability invariants; it subsumes their unfinished runtime work.

| Issue | What it contributed | What this spec does with it |
| --- | --- | --- |
| 002 — semantic assistant platform refactor | `GraphState` pipeline, `AssistantGraphTopology`, retrieval/rerank split | The pipeline becomes a compiled workflow graph (§4). L16 retires the hand-written topology; §19 item 3 decides whether `GraphState` survives as job-level lifecycle vocabulary. |
| 003 — verified payload extraction | deterministic extraction + `ConstraintField` provenance | Kept intact and promoted to a named `BindingSource` (`DeterministicExtraction`, §4.2), one step in the acquisition order (§6). |
| 005 — unified clarification contract | `ClarificationPayload` / `ClarificationView`, versioning, `others` handling | Preserved verbatim under D4; extended additively with workflow/node fields (§11). Version stays `1`. |
| 009 — conversational drill-down | continuation-after-selection semantics | Generalised: drill-down becomes `ResolveEntity` → `CardinalityBranch` → `ClarificationInterrupt` → resume (§9.3–9.4), instead of the client-only hardcoded path (§1.7, L4). |

---

## 1. Measured current state

Everything in this section was read from the tree at commit `d99d898`, not inferred.
It is the baseline the legacy-deletion gate (§13) is diffed against.

### 1.1 Execution model

| Fact | Evidence |
| --- | --- |
| `ExecutionPlanType` has exactly one variant, `Atomic` | `crates/chat/src/assistant/execution/plan/mod.rs:15-17` |
| An `ExecutionPlan` carries one `capability` + one `query_id` + one optional `dataset_selection` | `plan/mod.rs:19-32` |
| `build_execution_plan` returns `None` unless classification produced exactly one matched capability | `plan/mod.rs:66-96` |
| `evaluate_policy` ignores the catalog argument (`_catalog`) and checks capability + office scope only | `plan/mod.rs:179-208` |
| Execution runs one statement then builds a response | `assistant/execution/runtime/execution.rs:284-363` |

There is no typed representation anywhere for a second step, a branch, a binding
between steps, or a resume point inside a multi-step run.

### 1.2 Petgraph

`AssistantGraphTopology` (`assistant/state/graph.rs`) holds four fields: a
`petgraph::Graph`, a `nodes` map, an `edges: HashSet<(GraphState, GraphState)>` and a
`terminal_edges: HashSet<_>`.

`validate_transition` (`graph.rs:218-245`) consults `self.edges` and
`self.terminal_edges`. `validate_sequence` (`graph.rs:247-268`) walks the same two
sets. **The `petgraph::Graph` field is written by `add_edge` and never read.** Petgraph
is not merely "topology validation, not orchestration" as issue 012 states — in the
current code it is not load-bearing at all. Removing the crate today would change no
behaviour.

The 13 `GraphState` values and 7 `TerminalState` values describe a fixed linear
pipeline; the transition list is a hand-maintained literal (`graph.rs:89-187`).

### 1.3 Rig

`rig-core = 0.40.0` (workspace `Cargo.toml:54`). The only reference to it in
`RigLlmClient` is:

```rust
let _ = std::mem::size_of::<rig_core::providers::openai::Client>();   // rig_client.rs:30
```

Everything else in that file is a hand-written `reqwest` OpenAI-compatible transport
(`rig_client.rs:104-245`) plus a genuinely useful retry policy
(`send_with_retry`, `is_transient_status`, `retry_delay`, `rig_client.rs:50-91`,
`272-301`) and a `json_object` fallback for providers that reject `json_schema`
(`rig_client.rs:139-153`, `256-262`).

`crates/chat/examples/phase0_rig_poc.rs` proves the `rig_core::tool::Tool` trait
round-trips, but it is an example binary; nothing in `src/` uses it.

No production `.agent()`, `.tool()`, `.dynamic_tools()`, `.dynamic_context()` or turn
loop exists.

### 1.4 Swiftide

`swiftide = 0.32.1` (workspace `Cargo.toml:56`). The only reference:

```rust
let _swiftide_loader = std::any::type_name::<swiftide::indexing::loaders::FileLoader>();  // knowledge/index/swiftide.rs:26
```

`SwiftideIndexPipeline` is 157 lines of `fs::read_dir` recursion, extension filtering,
a 1 800-character line-boundary chunker, and body-hash dedup. It never calls Swiftide.

### 1.5 Catalog and dataset counts

| Resource | Count | Location |
| --- | --- | --- |
| Capabilities | 41 (client 13, organization 10, savings 18) | `knowledge/capabilities/**` |
| Query contracts | 41 | `knowledge/queries/**` |
| Datasets | 4 | `knowledge/datasets/**` |
| Metrics | 17 | `knowledge/metrics/**` |
| Parameter inputs | 9 | `knowledge/parameters/**` |

The four datasets are `organization.office_summary`, `savings.accounts`,
`savings.account_activity`, `savings.account_charges`. `DatasetRecipe` produces one
`ComposedSql`; `DatasetKnowledge` has no cardinality, entity-key, resolver or
output-binding metadata (`knowledge/dataset/model.rs:11-44`).

### 1.6 Parameter acquisition — what already exists

`ParameterPolicy` (`knowledge/catalog/parameter_policy.rs:38-47`) already carries
`required`, `default: Option<DefaultExpr>`, `fill_when_missing`, `user_may_override`,
`hard_cap`. `DefaultExpr` is a closed allowlist including `AuthorizedScope`
(`parameter_policy.rs:24-36`, parsed at load time only, `:68-99`).

This is roughly half of issue 012's required acquisition model: it distinguishes
*defaulted* from *required*, but not *execution-required* from *user-required*, and it
has no notion of a value arriving from a prior step or a data probe. It is extended,
not replaced (§8).

Note: `CapabilityKnowledge.parameter_policies` is `#[serde(default, skip)]` and
populated by the loader (`knowledge/model.rs:354-355`) — capability YAML still carries
the legacy `required_parameters` / `optional_parameters` pair (`:335-339`).

### 1.7 Clarification is raised before data is consulted

`execute_selected_capability` calls `defaultless_missing_fields` and returns a
`CollectFields` clarification **before any query runs**
(`runtime/execution.rs:41-76`). The only data-aware ambiguity handling in the tree is:

```rust
let entity_options = matches!(
    capability_id.as_str(),
    "client_name_lookup" | "client_relationship_lookup"
)                                                          // runtime/execution.rs:296-300
```

— hardcoded to two capability IDs, producing a `SelectEntity` payload whose
continuation is then handled by the generic option handler in
`runtime/mod.rs:463-515`, which treats the selected option ID as a capability ID
(`memory.selected_capability = Some(option_id.clone())`, `runtime/mod.rs:483`). An
option ID of the form `client:7` (built at `runtime/execution.rs:418`) is therefore
assigned to a field that everything downstream reads as a capability ID.

### 1.8 Alternate runtime paths in production code

| Path | Selector | Location |
| --- | --- | --- |
| Deterministic greeting/help shortcut | none — always first | `runtime/mod.rs:572-587` |
| Gateway pipeline (Layers 1–3) | `AI_REPORT_GATEWAY_PIPELINE=on` | `runtime/mod.rs:592-606` |
| Classifier + semantic router | default | `runtime/mod.rs:607-662` |
| Canonical state | `CHAT_CANONICAL_GATEWAY_MODE=disabled\|shadow\|authoritative` | `crates/core/src/config/mod.rs:136-151`, `:305` |
| `AssistantGraphRuntime::run` (no router) | called by tests only | `runtime/mod.rs:324-361` |

`run_via_gateway_pipeline`'s `Execute` arm **records the decision and returns
`Completed` without executing a query** (`runtime/mod.rs:245-268` — the doc comment at
`:182-184` says the DB call is deferred). Turning that flag on in production returns
successful-looking jobs with no data.

### 1.9 Durable job state

`chat_jobs` (`migrations/20260617130000_create_chat_tables.sql:25-47`) has
`current_step TEXT`, `resume_from_step TEXT`, `state_json JSONB`, `state_revision
BIGINT`. Both step columns carry a `CHECK` constraint enumerating 19 fixed step names
(`:45-46`) that mirror `GraphState`. `chat_job_checkpoints` (`:62-70`) has a
`checkpoint_type` CHECK over 10 fixed values. Pending clarification lives in
`assistant_job_memory.pending_clarification_json`
(`migrations/20260722120000_add_job_scoped_clarification.sql`).

Neither table can currently name a workflow, a workflow version, or a node.

---

## 2. Settled decisions

| ID | Decision | Rationale |
| --- | --- | --- |
| **D1** | **Rig becomes the real LLM boundary.** `RigLlmClient` is rewritten over `rig_core` provider clients + `.agent()` / `.tool()` / turn loops. The project keeps a thin adapter for provider config, tracing, retry, pricing and sanitized errors. | Issue 012 §"Required Rig integration" states this as the target. §1.3 shows the current integration is a `size_of` marker. |
| **D2** | **Swiftide is removed.** Drop `swiftide` from `Cargo.toml`; rename `SwiftideIndexPipeline` → `CatalogIndexPipeline` and `SwiftideKnowledgeDocument` → `CatalogDocument`; move `knowledge/index/swiftide.rs` → `knowledge/index/pipeline.rs`. | The indexer walks ~150 local YAML/SQL/MD files. Swiftide 0.32.1 adds a dependency surface and buys nothing here, and its unreleased task/agent APIs are not available on that release. Full removal, not a rename-and-keep. |
| **D3** | **Petgraph becomes load-bearing or goes.** The workflow compiler stores nodes/edges in a `petgraph::DiGraph`, and cycle detection (`is_cyclic_directed`), dependency order (`toposort`) and runnable-node selection read that graph. If the compiler ends up not needing it, the dependency is deleted rather than kept as a marker. | §1.2: the graph is currently write-only. D3 makes the read path real. |
| **D4** | **Response and clarification contracts are additive-only.** Existing fields (`version`, `id`, `revision`, `kind`, `question`, `options`, `fields`, `allow_free_text`) keep their current meaning and JSON names. Workflow fields are added as optional. `CLARIFICATION_VERSION_1` stays `1`. | Frontend continues to work unchanged; no compatibility projection to delete later. |
| **D5** | **Canonical state becomes mandatory.** `CanonicalGatewayMode` and `CHAT_CANONICAL_GATEWAY_MODE` are deleted; the workflow runtime always builds canonical state. `AI_REPORT_GATEWAY_PIPELINE` is deleted. | Two of the three alternate paths in §1.8 exist only as migration scaffolding, and one of them silently skips execution. |
| **D6** | **No new orchestration dependency.** Rig + petgraph + PostgreSQL + the existing approved-SQL layer. | Issue 012 §"No new orchestration framework is required by default". |
| **D7** | **One workflow, one job.** A workflow never spawns a second `chat_jobs` row. Clarification continues the same job via `POST /chat/jobs/{id}/responses`. | Existing `AGENTS.md` invariant; unchanged. |
| **D8** | **The LLM proposes; Rust decides.** The planner agent returns a workflow *proposal* referencing catalog IDs only. Compilation, verification, policy, binding and execution are deterministic Rust. A rejected proposal executes zero queries. | Issue 012 §"Authority split". |

---

## 3. Target architecture

```text
Rig agent layer          understand → propose workflow → (optional) grounded prose
        ↓ proposal (catalog IDs only, never SQL)
Workflow compiler        resolve IDs → type-check bindings → build DiGraph
        ↓ CompiledWorkflow
Verifier                 cycles, budgets, scope, sensitivity, unknown refs
        ↓ VerifiedWorkflow
Policy preflight         per-node capability / office / PII
        ↓
Runner                   execute runnable nodes, inspect cardinality, branch,
                         checkpoint, pause, resume
        ↓ NodeOutput*
Composer                 deterministic structured response
        ↓
PostgreSQL               durable truth: workflow, node outputs, provenance, audit
```

Module ownership (no new crates — workspace stays at `app` / `core` / `chat`):

| Concern | Module |
| --- | --- |
| Workflow contract types | `chat::assistant::workflow::contract` |
| Compiler (proposal → CompiledWorkflow) | `chat::assistant::workflow::compile` |
| Verifier | `chat::assistant::workflow::verify` |
| Graph (petgraph wrapper) | `chat::assistant::workflow::graph` |
| Runner | `chat::assistant::workflow::run` |
| Node executors | `chat::assistant::workflow::node::{resolve_entity, execute_query, branch, clarify, compose}` |
| Durable state | `chat::assistant::workflow::state` |
| Rig agents and tools | `chat::assistant::llm::agent::{understanding, planning, response}`, `chat::assistant::llm::tool` |

`chat::assistant::execution::plan` and `chat::assistant::execution::runtime` are
deleted at Phase 7; `chat::execution::repository` (approved-SQL execution) survives and
is called by the `ExecuteQuery` node executor.

---

## 4. Workflow contract

Serialized into `chat_jobs.state_json.workflow` and versioned. `#[serde(deny_unknown_fields)]`
on every struct so a stale persisted workflow fails loudly rather than defaulting.

```rust
pub const WORKFLOW_CONTRACT_VERSION: u16 = 1;

pub struct ExecutionWorkflow {
    pub id: Uuid,
    pub contract_version: u16,      // WORKFLOW_CONTRACT_VERSION
    pub catalog_version: Uuid,      // ties the workflow to the catalog it compiled against
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    pub budgets: WorkflowBudgets,
    pub fail_policy: FailPolicy,
    pub output_contract: OutputContract,
}

pub struct WorkflowBudgets {
    pub shared_timeout_ms: u64,     // whole workflow, not per node
    pub shared_row_cap: u32,        // sum over all nodes
    pub max_query_count: u8,
    pub max_parallel_queries: u8,
    pub max_model_turns: u8,
    pub max_node_retries: u8,
}

pub enum FailPolicy { FailFast, ContinueLabelled }   // ContinueLabelled requires OutputContract::allows_partial

pub struct OutputContract {
    pub mode: OutputMode,           // table | scalar | comparison | grouped | not_found
    pub allows_partial: bool,       // default false
    pub max_sensitivity: Sensitivity,
}

pub struct WorkflowEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub condition: EdgeCondition,
}

pub enum EdgeCondition {
    Always,
    Cardinality(Cardinality),       // Zero | One | Many
    ClarificationAnswered,
}
```

`NodeId` is a newtype over a `String` matching `^[a-z][a-z0-9_]{0,47}$`, unique within
a workflow. It is stable across resume — the resume target is a `NodeId`, never an index.

### 4.1 Node kinds

```rust
pub struct WorkflowNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub inputs: Vec<NodeInput>,
    pub outputs: Vec<NodeOutputSlot>,
    pub policy: NodePolicy,
    pub budget: NodeBudget,
    pub idempotency: Idempotency,   // Pure | Replayable | ExecuteOnce
    pub retry: RetryPolicy,
}

pub enum NodeKind {
    ResolveEntity(ResolveEntityNode),
    ExecuteQuery(ExecuteQueryNode),
    CardinalityBranch(CardinalityBranchNode),
    ClarificationInterrupt(ClarificationInterruptNode),
    ComposeResult(ComposeResultNode),
    Complete(CompleteNode),
}
```

| Kind | Must declare | Executes SQL | Can pause |
| --- | --- | --- | --- |
| `ResolveEntity` | `dataset_id`, `resolver_shape_id`, `entity_kind`, `probe_row_cap` | yes (bounded probe) | no |
| `ExecuteQuery` | `capability_id` **or** (`dataset_id` + `shape_id`), `query_id` after compile | yes | no |
| `CardinalityBranch` | `source: NodeId`, arms for `Zero`/`One`/`Many` | no | no |
| `ClarificationInterrupt` | `clarification_kind`, `option_source: NodeId`, `resume: NodeId` | no | **yes** |
| `ComposeResult` | `sources: Vec<NodeId>`, `composition` (`single`/`comparison`/`grouped`) | no | no |
| `Complete` | `terminal: TerminalState` | no | no |

`NodePolicy` carries `required_capability: Option<String>`, `office_scope: Bound`
(always `Bound::AuthorizedIntersection`), `max_sensitivity: Sensitivity`,
`pii_required: bool`. Every node is checked independently — a workflow-level preflight
is an optimisation, never a substitute (§12 SI-6).

`NodeBudget` carries `timeout_ms`, `row_cap`, `query_cost: u8` (nodes that execute SQL
declare 1; iteration expands to N nodes at compile time, each costing 1).

### 4.2 Bindings

```rust
pub struct NodeInput {
    pub parameter: String,          // query/dataset parameter name
    pub kind: ParameterType,        // reuses knowledge::catalog::parameter_policy::ParameterType
    pub source: BindingSource,
}

pub enum BindingSource {
    AuthorizedScope,
    CatalogDefault,                 // DefaultExpr, evaluated in EvaluationContext
    DeterministicExtraction { field: ConstraintField },
    VerifiedUserText { field: ConstraintField },
    ExactSensitiveInput,            // transient, never persisted, never projected
    SafePriorSelection { clarification: NodeId },
    PriorStep { node: NodeId, slot: String },
    AuthorizedDataProbe { node: NodeId, slot: String },
}

pub struct NodeOutputSlot {
    pub name: String,
    pub kind: ParameterType,
    pub sensitivity: Sensitivity,
    pub cardinality: Cardinality,
}
```

A binding is type-valid iff `NodeOutputSlot.kind == NodeInput.kind` and the producing
node precedes the consuming node in topological order. `Sensitivity` may only narrow
along a binding: a `Pii` slot cannot feed a node whose `max_sensitivity` is
`PublicBusiness`.

### 4.3 Bounded iteration

There is no loop edge. `IterateOver` is a **compile-time expansion**: a proposal may
declare `iterate_over: { source: NodeId, slot: String, max: u8 }` on an `ExecuteQuery`
node; the compiler materialises up to `max` sibling nodes with the slot value bound as
a literal, then fails compilation if `max` exceeds `budgets.max_query_count` minus
nodes already spent. The runtime graph therefore stays acyclic, and §5 can reject every
cycle unconditionally.

**Grouped-query preference.** The compiler rejects an `iterate_over` expansion when the
target dataset declares a `grouped_by` shape covering the same slot — one reviewed
grouped query wins over N+1 (issue 012 dataset requirement 8). The rejection is a
compile error with the grouped shape ID in the message, not a silent rewrite.

---

## 5. Verifier

`verify(compiled: CompiledWorkflow, principal, catalog) -> Result<VerifiedWorkflow, VerifyError>`.

`VerifyError` is a closed enum; every variant maps to a sanitized client message and a
distinct audit reason code. Rejection is unconditional — there is no "warn and proceed".

| # | Rejection | Check |
| --- | --- | --- |
| V1 | Cycle | `petgraph::algo::is_cyclic_directed` over the compiled `DiGraph` |
| V2 | Unknown capability / query / dataset / shape / filter ID | catalog lookup, `status == approved_mvp` |
| V3 | Type-incompatible binding | §4.2 |
| V4 | Data-dependent SQL identifier | every SQL identifier traces to a catalog `FilterSlot.expr` / `OrderByOption.expr`; values bind as parameters only |
| V5 | Missing office scope | every SQL-executing node has `office_ids` bound from `AuthorizedScope` or a narrowing intersection of it |
| V6 | Budget exceeded | sum of `query_cost` ≤ `max_query_count`; sum of `row_cap` ≤ `shared_row_cap`; sum of `timeout_ms` ≤ `shared_timeout_ms`; fan-out width ≤ `max_parallel_queries` |
| V7 | Partial results not permitted | `fail_policy == ContinueLabelled` while `output_contract.allows_partial == false` |
| V8 | Sensitivity widening | any binding or composition whose output sensitivity exceeds the consuming node's or the workflow's `max_sensitivity` |
| V9 | Unreachable / orphan node | every node reachable from the entry node; exactly one entry; at least one `Complete` |
| V10 | Dangling resume | every `ClarificationInterrupt.resume` names an existing node reachable from it |
| V11 | Unbound required input | every `NodeInput` for a query-required parameter has a `BindingSource` |
| V12 | Capability not permitted for principal | `ensure_capability_allowed` per node |

V1, V6, V9 and V10 are the checks that make petgraph load-bearing (D3).

---

## 6. Parameter acquisition

`ParameterPolicy` (§1.6) gains three fields; nothing existing is removed, so already-migrated
capability YAML keeps parsing.

```rust
pub struct ParameterPolicy {
    // existing
    pub name: String,
    pub kind: ParameterType,
    pub required: bool,               // execution-required (query needs a value)
    pub default: Option<DefaultExpr>,
    pub fill_when_missing: bool,
    pub user_may_override: bool,
    pub hard_cap: Option<i64>,
    // new
    pub user_required: bool,          // default false — may this become a question?
    pub resolution: Vec<ResolutionStrategy>,   // ordered; default [] = policy order below
    pub probe: Option<ProbeRef>,      // dataset + resolver shape for AuthorizedDataProbe
}

pub enum ResolutionStrategy {
    AuthorizedScope,
    CatalogDefault,
    DeterministicExtraction,
    VerifiedUserText,
    SafePriorSelection,
    PriorStep,
    AuthorizedDataProbe,
    Clarify,
}
```

**Acquisition order** (issue 012 §"Resource-first behavior"), applied by the compiler
when a required value is absent. `resolution` may reorder or restrict but never
introduce a strategy the catalog has not declared for that parameter:

1. `AuthorizedScope` → 2. `CatalogDefault` → 3. `DeterministicExtraction` →
4. `VerifiedUserText` → 5. `SafePriorSelection` → 6. `PriorStep` →
7. `AuthorizedDataProbe` → 8. `Clarify` → 9. unsupported / policy-blocked.

**The rule that changes behaviour:** `required: true` no longer implies a question.
A `Clarify` node is emitted only when `user_required == true` **or** every earlier
strategy is exhausted. `defaultless_missing_fields`'s pre-query gate
(`runtime/execution.rs:41-76`) is deleted; the compiler decides instead, and it decides
after it knows whether a probe exists.

**Catalog validation additions** (`knowledge/catalog/validator.rs`):

- a parameter with `user_required: true` must have a `ParameterInputKnowledge` entry (it must be askable);
- a parameter whose `resolution` contains `AuthorizedDataProbe` must have `probe`, and that probe's dataset+shape must exist and declare an output slot of the parameter's type;
- `office_ids` may not list `VerifiedUserText` or `Clarify`;
- a parameter with `resolution: []` and no default and `user_required: false` is a load error — it can never be filled.

**YAML key mapping.** Issue 012 names the catalog keys `execution_required`,
`user_required`, `binding_source`, `resolution_strategy`, `user_may_supply`. The
existing `ParameterPolicy` already owns two of those concepts under different names, and
renaming a loaded field breaks every migrated capability YAML for no behavioural gain.
The authored keys are therefore:

| Issue 012 name | Authored key | Note |
| --- | --- | --- |
| `execution_required` | `required` | already exists (`parameter_policy.rs:42`) |
| `user_required` | `user_required` | new |
| `user_may_supply` | `user_may_override` | already exists (`:45`) |
| `resolution_strategy` | `resolution` | new, ordered list |
| `binding_source` | — | not authored; it is *derived* per workflow node by the compiler (`BindingSource`, §4.2). A catalog cannot know which prior node will supply a value. |

**`savings_account_activity_lookup` migration** (issue 012's named example): `search`
keeps `user_required: true`; `product_name` and `latest_transaction_amount` become
`required: false, user_required: false` and move to an optional disambiguator role. The
capability's normal path becomes resolve-client → resolve-accounts → activity.

---

## 6A. Ambiguity classification and the retrieval clarification gate

Issue 012's core complaint is that uncertainty of *any* kind becomes a question. §6 fixes
that for missing parameters. This section fixes it for retrieval.

### 6A.1 Measured current behaviour

`LlmReranker` coerces a `Select` into a `Clarify` when the model's confidence is below
`MIN_SELECT_CONFIDENCE` (`assistant/retrieval/reranker.rs:15`, `:208-215`) and when the
model's structured output is malformed (`:196-201`). Both paths call
`RerankerDecision::clarify(alternative_ids(candidates))` — a list of capability IDs shown
to the administrator. Neither path has consulted data, and the malformed-output path has
not even consulted the model successfully.

### 6A.2 The five classes

Every uncertainty the runtime can hold is exactly one of these. The class determines who
resolves it, and only one class may become a question without further work.

| Class | Meaning | Resolver | May clarify directly |
| --- | --- | --- | --- |
| **Semantic** | the requested business meaning is unclear | catalog compatibility check, then the administrator | yes — after 6A.3 |
| **Entity** | several authorized entities match a name | `ResolveEntity` probe → `CardinalityBranch` | only on `Many`, and only per §9.4 |
| **Data cardinality** | zero / one / many changes the next step | `CardinalityBranch` | never — it is a branch, not a question |
| **Parameter conflict** | two trusted sources disagree on one value | acquisition precedence (§6), higher rank wins; conflict recorded in provenance | only when both sources rank equal, which the catalog forbids |
| **Unsupported coverage** | no approved workflow can answer | `Complete { terminal: Unsupported }` | never — an honest refusal, not a question |

**Policy ambiguity does not exist.** A policy question is a failure, not an uncertainty:
the guard fails closed with `BlockedByPolicy`. The LLM is never asked whether something
is permitted, and no policy decision is a tool the planning agent can call.

### 6A.3 The gate

A low reranker confidence is **evidence**, not a decision. Before any semantic
clarification is emitted, the compiler must have established all three:

1. **Catalog compatibility is exhausted** — no single approved capability/dataset covers
   the extracted request shape and metrics. If exactly one does, low confidence is
   overridden and that resource is selected, with the override recorded in audit.
2. **Deterministic facts are exhausted** — every fact in §6 steps 1–6 has been applied;
   the alternatives still differ after applying them.
3. **No probe can discriminate** — no `role: resolver` shape exists whose bounded output
   would separate the surviving alternatives.

Only then may a `ClarificationInterrupt` be emitted, and its options are the surviving
alternatives' catalog display names — never bare capability IDs.

**Malformed structured output is not ambiguity.** It is an operational failure. It
retries per the provider retry policy (§8.1); on exhaustion the job terminates
`FailedOperational`. It must never surface as a list of capabilities for the
administrator to choose between, which is what `reranker.rs:196-201` does today.

### 6A.4 What this deletes

`MIN_SELECT_CONFIDENCE`-triggered clarification and the malformed-output clarification
fallback are removed from `reranker.rs`; the reranker returns a ranked, scored candidate
list and the compiler owns the clarify/no-clarify decision. Recorded as **L21** in §13.

---

## 7. Dataset migration

Datasets stay single-surface contracts. They gain the metadata workflows need.

### 7.1 New dataset fields

```yaml
entity:                       # present only on datasets that can resolve an entity
  kind: client                # client | office | savings_account | product | charge_definition
  id_field: client_id         # must be an output_field of type bigint, sensitivity public_business
  label_fields: [display_name, office_name, status_label]   # safe clarification labels
  label_fallback: "Client {client_id}"                      # used when can_view_pii is false

shapes:
  - id: identity_candidates
    role: resolver            # terminal | resolver | probe   (default terminal)
    expected_cardinality: many
    row_cap: 25
    produces:                 # typed output-to-input contract; no SQL leaves this file
      - slot: client_id
        type: integer
        sensitivity: public_business
        cardinality: many
```

`role: resolver` shapes are the only ones a `ResolveEntity` node may reference.
`role: probe` shapes are readable by the planner agent's `find_entity_resolver` tool
but may not be a workflow's terminal output.

### 7.2 Stable-ID and array filters

Every entity dataset must declare an `eq` filter on its `id_field` and an `in` filter
for bounded array binding. `FilterOperator` gains `In`
(`knowledge/dataset/model.rs:134-143`); the composer expands it to `= ANY($n)` with a
bound array — never interpolated text — and the validator caps array length at the
dataset's `row_cap`.

### 7.3 Required new datasets

| Dataset | Purpose | Resolver shapes |
| --- | --- | --- |
| `client.identity` | client resolution by name/id within office scope | `identity_candidates` |
| `organization.offices` | office resolution by name/id | `office_candidates` |
| `savings.accounts` (extend) | accounts by client | `accounts_by_client` |
| `savings.transactions` | activity rows by account | `activity_rows` |
| `savings.products` | products held by a client | `products_by_client` |
| `savings.charge_definitions` | distinct authorized charge types | `charge_type_candidates` |
| `client.portfolio_counts` | grouped loan/savings counts per client | `counts_by_client` (grouped, not N+1) |

Loan and audit datasets are **not** added — those domains remain deferred
(`knowledge/data-scope/areas/deferred.yaml`). A workflow that needs loan data returns
an honest `unsupported` terminal; it must never substitute savings data (issue 012
acceptance scenario 2).

Every new dataset keeps source-level authorized office predicates, per-field
`sensitivity`, and `core` projection minimization — user filters narrow, never widen.

---

## 8. Rig integration

### 8.1 Provider adapter

`RigLlmClient` is rewritten as `LlmProvider`, wrapping `rig_core` provider clients.
What must survive the rewrite, because it exists for reasons the tree records:

- transient-status retry with backoff, jitter and a whole-call budget (`rig_client.rs:50-91`, `272-301`);
- the `json_object` fallback with the schema restated in the prompt — DeepSeek rejects `json_schema` and 400s on `json_object` without a literal "JSON" (`rig_client.rs:139-153`, `247-262`);
- provider/model metadata, token usage and `llm_pricing` cost attribution (`rig_client.rs:169-190`);
- custom `chat_completions_url` support (`rig_client.rs:93-101`).

If `rig_core` 0.40.0 cannot express one of these, the adapter keeps that piece and the
plan records which — but the agent/tool loop must be Rig's, not hand-rolled.

### 8.2 Agents

| Agent | Output | Constraints |
| --- | --- | --- |
| Understanding | schema-validated intent / entities / request shape | catalog-visible vocabulary only; facts advisory until grounded |
| Planning | `WorkflowProposal` (catalog IDs, node kinds, bindings) | metadata tools only; `max_turns` enforced |
| Response (optional) | prose | consumes policy-filtered structured fields only |

### 8.3 Tools

Metadata tools — no data access:
`search_catalog`, `inspect_capability`, `inspect_dataset`, `find_entity_resolver`,
`find_compatible_next_steps`, `propose_workflow`.

Data tools — guarded:
`execute_approved_probe`, `execute_approved_capability`.

Every data tool's server-side implementation validates, in this order, before touching
the repository: workflow-step membership → capability → parameter provenance → policy →
PII → office scope → timeout → row cap → query budget. Rig never receives a raw-SQL
tool. Tool descriptions are generated from approved catalog metadata, never hand-written
prose that could drift from the SQL (this is the failure mode recorded in
`reranker-judges-prose-not-sql`).

Tool **outputs** are untrusted for prompt-injection purposes: they enter the model as
data, and they are authoritative only through their typed schema.

**`dynamic_tools` / `dynamic_context`.** Issue 012 permits these "where measured
retrieval quality supports them". They are **off by default**. The planning agent starts
with the six static metadata tools and a fixed context budget. Turning either on requires
a measured improvement on `crates/chat/tests/retrieval_eval.rs` against the current
baseline, recorded in the plan. Neither may ever expose a data tool dynamically: the two
guarded data tools are always statically registered, so the tool surface an
adversarial proposal can reach never depends on retrieval output.

### 8.4 Budgets

`max_turns` is necessary and insufficient. The runtime independently enforces
`max_query_count`, `shared_row_cap`, `shared_timeout_ms`, `max_node_retries`,
`max_parallel_queries` and `max_model_turns` from §4. A proposal that would exceed any
of them fails verification (V6) rather than being truncated mid-run.

---

## 9. Runner and durable state

### 9.1 Migration

```sql
-- migrations/2026xxxx_workflow_runtime.sql
ALTER TABLE chat_jobs
    ADD COLUMN workflow_id UUID NULL,
    ADD COLUMN workflow_contract_version SMALLINT NULL,
    ADD COLUMN workflow_revision BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN current_node_id TEXT NULL;

CREATE TABLE chat_workflow_node_runs (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id) ON DELETE CASCADE,
    workflow_id UUID NOT NULL,
    node_id TEXT NOT NULL,
    attempt SMALLINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL CHECK (status IN ('runnable','running','completed','failed','skipped','waiting')),
    output_json JSONB NULL,          -- typed slots only; never raw rows for pii slots
    provenance_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    rows_returned INT NOT NULL DEFAULT 0,
    duration_ms INT NULL,
    started_at TIMESTAMPTZ NULL,
    finished_at TIMESTAMPTZ NULL,
    UNIQUE (job_id, workflow_id, node_id, attempt)
);
```

The `chat_jobs.current_step` / `resume_from_step` CHECK constraints (§1.9) are replaced
with the workflow-runtime step set in the same migration. Existing rows are migrated,
not dropped — historical jobs stay readable (issue 012's carve-out for persisted-data
readers), and that reader carries an explicit retirement note.

`chat_job_checkpoints.checkpoint_type` gains `node_started`, `node_completed`,
`workflow_paused`, `workflow_resumed`.

### 9.2 Paused state

A paused job persists: `workflow_id`, `contract_version`, `workflow_revision`,
`current_node_id`, runnable node IDs, completed node outputs (or durable references),
parameter provenance per bound value, budget consumed (queries / rows / ms), the
pending clarification, and the principal projection used.

### 9.3 Resume

`POST /chat/jobs/{id}/responses` matches on **five** values — job ID, workflow ID, node
ID, clarification ID, and workflow revision. A mismatch on any is a stale-clarification
rejection, not a reroute. On match the runner:

1. binds the answer through `SafePriorSelection` into the waiting node's input;
2. marks the `ClarificationInterrupt` node completed;
3. resumes at its `resume` node.

Completed nodes are **not** re-run. A node with `Idempotency::ExecuteOnce` that already
has a `completed` row is a hard error if the runner reaches it again. Replay is only
possible for `Idempotency::Replayable` nodes and only when an explicit replay decision
is recorded on the job.

### 9.4 Zero / one / many

| Cardinality | Behaviour |
| --- | --- |
| Zero | `Complete { terminal: NotFound }` — a safe empty answer, never a question |
| One | auto-bind the stable ID; continue with no user interaction |
| Many | clarify **only if** the selection changes the answer; otherwise bind all |
| Many + valid "all" | expose a typed `all` option — never `-` or a placeholder string |

"Selection changes the answer" is a declared property, not a heuristic: the
`CardinalityBranch` node's `Many` arm either points at a `ClarificationInterrupt` or at
an `ExecuteQuery` whose input binds the full ID array through an `In` filter. The
compiler picks the array path whenever the downstream shape declares an array-capable
filter on that ID.

### 9.5 Concurrency

`max_parallel_queries` bounds fan-out. A failed node under `FailPolicy::FailFast`
cancels in-flight siblings via a shared `CancellationToken` and the workflow terminates;
no partial result is composed. Under `ContinueLabelled` (only legal when
`output_contract.allows_partial`), surviving branches compose with an explicit
per-branch status label.

---

## 10. Response composition

`ComposeResult` merges node outputs deterministically. The LLM is not in this path.

- `single` — one source node's rows.
- `comparison` — N sources with identical scope and temporal facts (the compiler
  verifies fact identity; differing facts is a compile error, not a runtime warning).
- `grouped` — one source, grouped by a declared key.

Sensitivity is enforced at composition: a field whose `Sensitivity` exceeds the
principal's projection is dropped before the response is built, and dropping is
recorded in audit rather than silently applied.

The structured response keeps its current JSON shape (D4). Optional additive fields:
`workflow.id`, `workflow.node_id`, `workflow.steps_executed`, `workflow.partial`.

---

## 11. Clarification contract (additive)

`ClarificationPayload` and `ClarificationView`
(`assistant/context/clarification.rs:71-104`, `:155-179`) gain optional fields only:

```rust
pub workflow_id: Option<Uuid>,
pub node_id: Option<String>,
pub resume_node_id: Option<String>,
pub entity_kind: Option<String>,   // "client" | "office" | ...
```

`ClarificationKind` keeps its four variants. `SelectEntity` gains a real meaning: its
options carry `entity_kind` + a stable ID, and the runtime binds the selection as a
**parameter value**, not as a capability ID. The `memory.selected_capability =
Some(option_id)` assignment at `runtime/mod.rs:483` is deleted — that is the bug in
§1.7, and its fix is structural rather than a guard at the call site.

`OTHER_CLARIFICATION_OPTION_ID` behaviour is unchanged.

---

## 12. Security invariants and their enforcement points

Each invariant names the single place that enforces it. "Enforced in two places" means
one of them is decoration and will drift.

| # | Invariant | Enforced at |
| --- | --- | --- |
| SI-1 | Bearer session JWT + admin role | `AuthenticatedChatClient` extractor (unchanged) |
| SI-2 | Office scope bound inside approved SQL | `chat::execution::repository`, `office_ids` bound parameter |
| SI-3 | User filters narrow only | compiler binding rules + V5 |
| SI-4 | LLMs never generate or edit SQL | tool surface (§8.3) — no raw-SQL tool exists |
| SI-5 | Only catalog-approved resources execute | V2 |
| SI-6 | Per-node policy and PII | `run::execute_node` calls the policy guard per node; preflight is an optimisation |
| SI-7 | Sensitive selectors transient, exact-match, non-projected, rate-limited | `BindingSource::ExactSensitiveInput` is never written to `chat_workflow_node_runs.output_json` |
| SI-8 | Probe outputs minimal and bounded | dataset `row_cap` + `label_fields` allowlist; `label_fallback` when `can_view_pii == false` |
| SI-9 | Fineract rows never indexed into vectors | `CatalogIndexPipeline::reject_client_rows` (kept from `swiftide.rs:122-133`) |
| SI-10 | Query / row / timeout / retry / turn / parallelism bounds | V6 + runner budget ledger on the job row |
| SI-11 | Tool outputs untrusted for injection | agent prompts treat tool results as data; only typed schema fields are authoritative |
| SI-12 | No SQL / prompts / stack traces / hidden IDs in client errors | `ApiError` + closed `VerifyError` → sanitized message mapping |
| SI-13 | Durable audit lineage per execution and branch | `chat_workflow_node_runs` + `chat_job_events` |
| SI-14 | No unlabelled partial results | V7 + `FailPolicy` |

---

## 13. Legacy inventory — machine-checkable

### 13.1 Final-state rule

Migration flags may exist inside a phase. At Phase 7 the tree must contain exactly one of
each of these, and the count is asserted, not asserted-by-reading:

| Exactly one | Owner after migration |
| --- | --- |
| router | Rig understanding agent (§8.2) |
| planner / workflow compiler | `workflow::compile` (§3) |
| parameter acquisition model | `ParameterPolicy` + acquisition order (§6) |
| clarification continuation path | `workflow::run` resume (§9.3) |
| workflow runner | `workflow::run` |
| approved executor path | `chat::execution::repository` |
| structured response authority | `workflow::node::compose` (§10) |

"Deprecated but still reachable", "kept just in case" and "fallback if the agent fails"
do not satisfy this. Failure follows the runtime's own terminal states.

### 13.2 Inventory

Every row must be gone at Phase 7. The "verification" column is the exact command whose
empty output is the gate.

| # | Legacy item | Location | Verification |
| --- | --- | --- | --- |
| L1 | `ExecutionPlanType::Atomic`, `ExecutionPlan`, `build_execution_plan`, `evaluate_policy` | `assistant/execution/plan/` | `rg -n 'ExecutionPlanType\|build_execution_plan' crates/` |
| L2 | Pre-query missing-parameter gate | `runtime/execution.rs:41-76`, `context/clarification_planner.rs::defaultless_missing_fields` | `rg -n 'defaultless_missing_fields' crates/` |
| L3 | Option ID assigned to `selected_capability` | `runtime/mod.rs:483`, `:445` | `rg -n 'selected_capability = Some\(option_id' crates/` |
| L4 | Hardcoded client-only ambiguity | `runtime/execution.rs:296-300`, `client_entity_options` | `rg -n 'client_name_lookup". *\|" *client_relationship_lookup' crates/src` |
| L5 | Capability-ID match arms driving behaviour | grep for `matches!(capability_id` / `match capability_id` | `rg -n 'capability_id\.as_str\(\)' crates/*/src` |
| L6 | Deterministic keyword shortcut | `runtime/mod.rs:572-587`, `deterministic_simple_response` | `rg -n 'deterministic_simple_response' crates/` |
| L7 | `AI_REPORT_GATEWAY_PIPELINE` flag + `run_via_gateway_pipeline` + `route_via_gateway_pipeline` | `runtime/mod.rs:154-319`, `:592-606` | `rg -n 'AI_REPORT_GATEWAY_PIPELINE\|run_via_gateway_pipeline' .` |
| L8 | `CanonicalGatewayMode`, `CHAT_CANONICAL_GATEWAY_MODE`, `job/service/shadow.rs` | `core/src/config/mod.rs:136-151`, `:305` | `rg -n 'CanonicalGatewayMode\|CHAT_CANONICAL_GATEWAY_MODE' .` |
| L9 | Classifier + semantic router path | `understanding/classifier/`, `assistant/llm/router.rs`, `runtime/semantic.rs` | `rg -n 'SemanticRouter\|ClassificationResult' crates/*/src` |
| L10 | `AssistantGraphRuntime::run` (routerless stub) | `runtime/mod.rs:324-361` | `rg -n 'AssistantGraphRuntime' crates/` |
| L11 | Duplicate parameter planning | `execution/tool/parameters.rs` vs `dataset/legacy.rs` | `rg -n 'knowledge::dataset::legacy' crates/` |
| L12 | `RigLlmClient` raw transport | `assistant/llm/rig_client.rs:104-245` | `rg -n 'reqwest' crates/chat/src/assistant/llm/` |
| L13 | Rig compile-time marker | `rig_client.rs:30` | `rg -n 'size_of::<rig_core' crates/` |
| L14 | Swiftide marker + dependency | `knowledge/index/swiftide.rs:26`, `Cargo.toml:56`, `crates/chat/Cargo.toml:36` | `rg -n 'swiftide' .` |
| L15 | `SwiftideIndexPipeline` / `SwiftideKnowledgeDocument` names | `knowledge/index/swiftide.rs` | `rg -n 'Swiftide' crates/` |
| L16 | `AssistantGraphTopology`, `GraphState`, `GraphTransition` if superseded | `assistant/state/graph.rs` | `rg -n 'AssistantGraphTopology' crates/` |
| L17 | `phase0_rig_poc` example | `crates/chat/examples/phase0_rig_poc.rs` | `ls crates/chat/examples/` |
| L18 | Tests asserting legacy paths / fixed catalog counts | `runtime/tests.rs`, `execution/tool/tests.rs`, `plan/tests.rs` | `cargo test -p chat` after deletion |
| L19 | Docs describing one-query terminal execution as target | `docs/architecture/ai-reporting-design/`, `AGENTS.md`, `CLAUDE.md` | manual doc diff in the deletion PR |
| L20 | Formatter special cases superseded by structured composition | `assistant/presentation/` | `rg -n 'capability_id\|output_mode ==' crates/chat/src/assistant/presentation/` |
| L21 | Reranker confidence-floor and malformed-output clarification fallbacks (§6A.4) | `assistant/retrieval/reranker.rs:15`, `:196-201`, `:208-215` | `rg -n 'MIN_SELECT_CONFIDENCE\|RerankerDecision::clarify' crates/` |

**L20 scope, measured.** A sweep of `assistant/presentation/` found no capability-ID
match arms — the only hits are one message template naming `{capability_id}`
(`builder.rs:183`) and one literal `output_mode: "list"` (`builder.rs:759`). L20 is
therefore expected to be a near-empty deletion; it stays in the inventory so the
Phase 7 gate proves that rather than assuming it.

**Issue 012 inventory item 16 is not applicable.** "Deprecated clarification API
compatibility projections" cannot accrue under D4: the contract is additive-only, so no
projection layer is ever built and there is no frontend migration gate to satisfy. If
any phase introduces one, D4 has been violated and the phase gate fails.

Additional gates:

- `cargo tree -p chat | rg 'swiftide'` returns nothing.
- No production config key selects a runtime: `rg -n 'env::var' crates/*/src` returns only `core/src/config/mod.rs` helpers.
- The deletion diff is reviewed as its own commit, separate from the new-behaviour diff.

---

## 14. Phases and gates

Each phase ends with a gate that must pass before the next begins.

| Phase | Content | Gate |
| --- | --- | --- |
| 0 | This spec + plan + inventory §13 frozen | inventory commands run and counts recorded |
| 1 | Rig provider/agent boundary (D1); Swiftide removal + rename (D2) | `cargo tree` clean; retry/fallback/pricing tests still green |
| 2 | Capability kinds, acquisition metadata (§6), resolver dataset shapes (§7) | catalog validator rejects each of the four new invalid shapes |
| 3 | Workflow contract, compiler, verifier, petgraph (§4, §5); ambiguity gate (§6A) | V1–V12 each have a rejecting test; zero queries executed on rejection; malformed reranker output terminates `FailedOperational` instead of clarifying |
| 4 | Durable sequential + conditional runner (§9) | restart-mid-workflow resume test passes against real Postgres |
| 5 | Composite + bounded iteration (§4.3, §9.5) | cancellation test; N+1 rejection test |
| 6 | Structured composition + optional prose (§10) | FE contract snapshot test unchanged (D4) |
| 7 | Legacy deletion (§13) | every §13 command empty; deletion diff reviewed separately |
| 8 | Acceptance + rollout (§15) | all §15 scenarios pass against production-like DB; metrics baselined |

Short-lived migration flags are permitted inside a phase. None may survive Phase 7.

---

## 15. Acceptance scenarios

Each is an end-to-end test against a production-like Fineract database, asserting the
node trace, not just the final text.

**A1 — Data-aware savings activity.** `show all savings account activities for Nathalie Doe`
→ `ResolveEntity(client)` runs inside office scope; zero → `NotFound`; one → auto-bind;
duplicates → `SelectEntity` with safe labels; one account → auto-continue; many accounts
→ options plus a typed `all`. No `latest_transaction_amount` is required. No raw account
number appears in any option or output.

**A2 — Office existence and portfolio.** `if office X exists, show 10 clients and each client's loan and savings account counts`
→ `ResolveEntity(office)` first; zero/one/many is data-aware; client selection capped at
10; counts come from `client.portfolio_counts` (one grouped query, `query_cost == 1`);
loan coverage returns an honest planned/unsupported label and no savings substitution.

**A3 — Charge type.** A charge request without an exact name runs the
`charge_type_candidates` probe and clarifies only among real authorized choices, or
offers a valid all-types path.

**A4 — Composite comparison.** `compare deposits and withdrawals this month` → two plans
with identical scope and temporal facts, both policy-passed before either executes,
bounded parallel run, deterministic combined response, no unlabelled partial.

**A5 — Sensitive account lookup.** Never enumerates candidates; requires exact transient
input; enforces office scope; masks output; unauthorized and nonexistent identifiers
produce indistinguishable responses.

**A6 — Recovery.** A workflow paused after a successful probe resumes at the waiting node
after process restart, with the probe **not** re-run.

**A7 — Adversarial planning.** Proposals containing unknown tools, cycles, raw SQL, scope
widening, type-invalid bindings, excess nodes or hidden PII projections are rejected by
V1–V12 and execute zero queries. Asserted by counting queries, not by reading the error.

---

## 16. Tests

Pure / fast: workflow schema round-trip; V1–V12 rejection cases; petgraph cycle and
toposort; acquisition precedence (one test per §6 step); zero/one/many branch selection;
budget arithmetic; sensitivity narrowing; ambiguity classification (§6A.2 — one test per
class asserting who resolves it, plus one asserting policy uncertainty never reaches the
model); the three-part clarification gate (§6A.3).

Integration: same-job node resume; duplicate and stale clarification (all five match
values); restart and replay idempotency; per-node policy and office scope; PII
minimization on probe labels; composite cancellation; dataset validation and
output-binding.

Contract: Rig tool-loop tests with a fake model — the fake returns proposals, never
inter-stage structs (per `tests-must-not-forge-inter-stage-values`); FE response and
clarification JSON snapshots proving D4.

Live: A1–A7 against a real database, plus the per-capability example sweep — a
capability is not done until its own YAML example runs end to end (per
`verify-the-path-not-the-artifact`).

Deletion: §13 commands as a shell test in CI.

Per `no-full-test-suite-runs`, phase gates run targeted `cargo test -p chat <name>`
selections; the full sweep runs only at the Phase 8 gate.

---

## 17. Metrics

Baseline before Phase 1, re-measure at Phase 8: clarifications per completed job;
clarifications resolved by a probe; repeated-clarification rate; incorrect capability
selection rate; unsupported rate for in-scope prompts; average/max queries per job; N+1
detections; timeout/cancellation rate; model turns and tool calls per job; restart
recovery success; policy-blocked proposals; PII redaction violations; grounding failures.

A lower clarification rate is not success if wrong-answer or unauthorized-data rates rise.
The Phase 8 gate fails on any increase in the last two regardless of the others.

---

## 18. Non-goals

LLM-generated SQL; arbitrary table access; unbounded agents; replacing PostgreSQL
durable truth with in-memory Rig or graph state; indexing Fineract rows into vectors;
promoting deferred loan/audit/tax/accounting domains; splitting reviewed grouped SQL
into multiple queries; keeping any legacy runtime as a production fallback.

---

## 19. Definition-of-done traceability

Issue 012's 13 done-conditions, each mapped to what discharges it. A condition with no
row here would be a spec gap.

| # | Done-condition | Discharged by | Proven by |
| --- | --- | --- | --- |
| 1 | One verified workflow runtime on the production path | §3, D5 | L7, L8, L9, L10 gates empty |
| 2 | Atomic / sequential / conditional / composite / bounded-iterative contracts exist and are tested | §4, §4.3 | §16 pure tests; A4 |
| 3 | Data-aware probes precede clarification where the contract permits | §6, §6A.3 | A1, A3; clarification-per-job metric (§17) |
| 4 | Clarification resumes the exact persisted node | §9.2, §9.3 | A6; stale-clarification tests (§16) |
| 5 | Rig is the real agent/tool boundary | D1, §8 | L12, L13, L17 empty; `cargo tree` |
| 6 | Petgraph drives dependencies and scheduling | D3, §5 (V1/V6/V9/V10) | petgraph cycle + toposort tests |
| 7 | Swiftide genuinely used or removed | D2 | L14, L15 empty; `cargo tree -p chat` |
| 8 | Datasets support resolver / probe / output-binding surfaces | §7 | dataset validation tests; A1–A3 |
| 9 | Every node preserves approved SQL, authz, scope, PII, budgets, audit | §12 SI-1…SI-14 | per-node policy + budget tests |
| 10 | Acceptance scenarios pass on production-like databases | §15 A1–A7 | Phase 8 gate |
| 11 | Legacy inventory deleted, searches clean | §13 | all §13.2 commands empty |
| 12 | No flag / fallback / alias / facade can re-enable legacy | §13.1, D5 | `rg env::var` returns only config helpers |
| 13 | Architecture docs describe the implemented system | L19 | doc diff reviewed in the deletion PR |

---

## 20. Open items for the plan

1. Whether `rig_core` 0.40.0's provider client can carry the custom
   `chat_completions_url` and the `json_object` fallback, or whether the adapter keeps
   the transport for those two cases specifically (§8.1).
2. Exact `chat_jobs.current_step` value set replacing the 19-name CHECK constraint, and
   the backfill mapping for historical rows (§9.1).
3. Whether `GraphState`/`TerminalState` survive as the job-level lifecycle vocabulary
   while node-level state moves to `chat_workflow_node_runs` (L16 is conditional on this).
