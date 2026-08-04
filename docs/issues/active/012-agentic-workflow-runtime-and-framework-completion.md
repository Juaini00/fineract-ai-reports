# 012 — Agentic workflow runtime and framework completion

Status: active — design required before implementation
Severity: blocker
Area: chat | runtime | catalog | datasets | retrieval | security | docs
Created: 2026-08-04
Resolved:

## Executive summary

The reporting assistant still treats the normal request as:

```text
one request -> one capability -> one atomic query -> one response
```

This makes SQL a terminal action instead of a safe resource the system may use at a bounded intermediate step. When a selected query needs a value that is not already present, the runtime usually asks the administrator for it before learning whether an authorized data probe could resolve it. The result is premature clarification, brittle exact-match capabilities, no general sequential or conditional execution, and no reusable data-aware entity resolution.

The installed technology stack can support the target architecture, but the runtime does not currently use it as intended:

- `rig-core` is present but is not the primary agent/tool loop.
- `petgraph` validates a static transition topology but is not the workflow control plane.
- `swiftide` is present but is not the actual indexing/query pipeline behind the class named `SwiftideIndexPipeline`.
- Dataset composition creates one approved statement; it does not participate in output-to-input workflow composition.

This issue is a major migration. It must deliver a single production runtime that uses the chosen frameworks honestly, supports bounded atomic/sequential/conditional/composite/iterative execution, asks only data-informed clarifications, persists and resumes workflows, and preserves all existing authorization, PII, approved-SQL, audit, timeout, and row-cap guarantees.

Completion requires **100% deletion of superseded legacy runtime paths and behaviors**. A permanent dual runtime, hidden fallback, framework marker integration, or feature-flagged legacy escape path is not an acceptable final state.

## Relationship to existing issues

This issue is the implementation successor and completion gate for the unfinished architecture goals in:

- Issue 002 — semantic assistant platform major refactor.
- Issue 003 — verified payload extraction.
- Issue 005 — unified clarification contract.
- Issue 009 — conversational drill-down.

Those issues remain useful historical contracts. This issue owns the cross-cutting runtime migration needed to make them coherent. It does not weaken their security or durability invariants.

## Problem

### SQL is treated as the final step

The current dominant flow is:

```text
route
  -> retrieve one capability
  -> require all defaultless execution parameters
  -> clarify if a parameter is absent
  -> guard
  -> execute one query
  -> render
```

The system cannot generally express:

```text
resolve an entity with an approved probe
  -> inspect zero/one/many matches
  -> bind a stable identifier automatically when unique
  -> clarify only when several valid choices remain
  -> continue to the requested report
```

A narrow post-query client ambiguity path exists, but it is hardcoded, not reusable for offices/accounts/products/charges, and its `SelectEntity` continuation can be intercepted by the generic capability-option handler.

### Required execution input is conflated with required user input

A query parameter can be required at execution time while being acquired from:

- authenticated authorization scope;
- an approved default;
- deterministic extraction;
- verified user text;
- an earlier workflow step;
- a safe entity selection;
- transient sensitive input.

The current contracts do not describe these acquisition semantics completely. The planner therefore treats some execution requirements as questions the administrator must answer.

The clearest example is `savings_account_activity_lookup`, whose current exact-match contract requires:

```text
search + product_name + latest_transaction_amount
```

That contract is appropriate only for a narrow fingerprint lookup. It is not appropriate for "show all savings account activities for Nathalie Doe". Product and latest transaction amount can often be derived, made optional, or avoided by resolving the client and accounts first.

### Retrieval ambiguity is converted into user work too early

The reranker may clarify when candidates are close, confidence is below the selection floor, structured decoding fails, or multiple measures appear plausible. This is appropriate for genuine semantic ambiguity. It is not enough to distinguish semantic ambiguity from data ambiguity.

Data ambiguity can only be known after an authorized bounded probe. A low confidence score must not automatically become an administrator question when catalog compatibility, deterministic facts, or a safe probe can resolve the uncertainty.

### The runtime is atomic-only

`ExecutionPlanType` currently exposes only `Atomic`, and each plan carries one capability and one query ID. The executor resolves one approved statement and then builds a response. There is no typed runtime representation for:

- sequential dependencies;
- result-dependent branches;
- bounded parallel composition;
- bounded iteration;
- per-step output bindings;
- pause/resume at a specific workflow node.

### Dataset composition stops at one statement

The dataset layer correctly constrains one reusable relational source through authored shapes, filters, projections, orderings, sensitivity, and parameters. However, the current catalog has only four dataset definitions:

- `organization.office_summary`;
- `savings.accounts`;
- `savings.account_activity`;
- `savings.account_charges`.

Most capabilities remain direct query contracts. The dataset composer emits one `ComposedSql`; its plan enumerates `shapes x order_by`, not a workflow of statements. Stable-ID filters, array/`IN` bindings, resolver shapes, cardinality metadata, and output-to-input contracts are incomplete.

Datasets must remain the safe definition of one data surface. Workflow orchestration belongs above them, but datasets must expose enough typed contracts to participate safely in workflows.

## Technology reality check

### Rig is installed but not used as the primary agent boundary

`rig-core = 0.40.0` supports the features needed for bounded agentic planning:

- tools and structured tool arguments;
- model -> tool -> model loops;
- `max_turns`;
- dynamic context/RAG;
- dynamic tool retrieval;
- multi-turn message history;
- agent-as-tool composition.

The current `RigLlmClient` instead owns a custom `reqwest` OpenAI-compatible transport. Its concrete Rig use is a compile-time `size_of::<rig_core::providers::openai::Client>()` marker. No production `.agent(...)`, `.tool(...)`, `.dynamic_tools(...)`, `.dynamic_context(...)`, `AgentRun`, or agent turn loop is wired.

This is not an acceptable completed integration. The migration must either make Rig the real LLM agent/tool boundary or remove/rename every component that claims Rig ownership. The target decision for this issue is to use Rig fully while retaining a narrow project adapter for provider configuration, tracing, retries, and sanitized errors.

### Petgraph is topology validation, not orchestration

`petgraph = 0.8.3` can represent directed graphs, validate DAGs, detect cycles, topologically sort dependencies, and traverse nodes. The current `AssistantGraphTopology` uses it only to validate allowed transitions. Runtime orchestration remains hand-written and linear, and some transition lists are reconstructed after behavior rather than driving behavior.

The migration must make a typed workflow graph the control plane. Petgraph owns graph structure and dependency validation; custom Rust owns node execution, security, persistence, retries, and job integration.

### Swiftide integration is nominal

`swiftide = 0.32.1` is suitable for offline ingestion and retrieval pipelines. The current `SwiftideIndexPipeline` performs custom filesystem walking/chunking/deduplication and references Swiftide through `type_name::<swiftide::indexing::loaders::FileLoader>()` only.

The released 0.32.1 dependency must not be assumed to provide the unreleased task/agent APIs shown in newer repository documentation. This issue must make an explicit choice:

1. use Swiftide 0.32.1 for a real supported offline indexing/query pipeline; or
2. retain the custom indexer, rename it truthfully, and remove the unused Swiftide dependency.

Swiftide is not the durable security-sensitive workflow engine.

### No new orchestration framework is required by default

Rig + Petgraph + PostgreSQL + the existing approved-SQL and policy layers are sufficient. This issue must not add LangGraph/LangChain or another agent orchestrator unless the design proves a requirement the selected stack cannot meet. Any such proposal must preserve the same PostgreSQL job ownership, clarification, authorization, and audit boundaries.

## Target operating model

### Authority split

```text
Rig LLM layer
  understand, retrieve metadata, propose a workflow, explain grounded results

Rust verifier/control plane
  validate facts, capabilities, graph, bindings, scope, policy, budgets

Petgraph
  workflow topology, dependencies, cycle detection, execution ordering

PostgreSQL
  durable workflow/job/clarification truth

Approved dataset/query repository
  only executable data access
```

The LLM is probabilistic. Authorization, SQL selection, parameter provenance, graph validity, and execution are deterministic.

### Target request pipeline

```text
authenticate and project principal
  -> build bounded context
  -> Rig structured understanding
  -> deterministic fact verification
  -> retrieve capability, dataset, resolver and workflow resources
  -> Rig proposes a typed workflow
  -> Rust compiles and verifies the workflow
  -> policy preflight
  -> execute bounded steps
  -> inspect structured results
       -> continue
       -> branch
       -> pause for clarification
       -> fail safely
  -> compose a policy-filtered structured response
  -> optional grounded Rig prose
  -> persist result and audit lineage
```

### Resource-first behavior

When a required fact is absent, the system must evaluate its acquisition contract in this order:

1. authorized principal/scope;
2. approved catalog default;
3. deterministic extraction;
4. verified user text;
5. safe prior workflow output;
6. approved bounded data probe;
7. clarification;
8. unsupported or policy-blocked result.

Clarification is a last-resort state transition, not the automatic consequence of a missing query parameter.

### Ambiguity classes

The design must distinguish:

- **semantic ambiguity** — the requested business meaning is unclear;
- **entity ambiguity** — several authorized entities match;
- **data cardinality** — zero, one, or many rows alter the next step;
- **parameter conflict** — two trusted sources disagree;
- **unsupported coverage** — no approved workflow can answer;
- **policy ambiguity is forbidden** — policy must fail closed, never ask the LLM.

## Required execution model

The final runtime must support:

### Atomic

```text
one approved capability -> one approved statement -> response
```

### Sequential

```text
step A output -> verified typed binding -> step B input
```

### Conditional

```text
probe -> zero/one/many branch -> continue or clarify
```

### Parallel/composite

```text
independent guarded plans -> bounded fan-out -> declared composition
```

### Iterative

```text
same approved plan over a bounded declared entity/period set
```

Iteration and parallelism must have explicit query, row, timeout, and concurrency budgets. The LLM may not create unbounded loops.

## Required workflow contracts

The design spec must define a serializable versioned contract equivalent to:

```text
ExecutionWorkflow {
  id, version, nodes, edges,
  shared_timeout_ms,
  shared_row_cap,
  max_query_count,
  max_parallel_queries,
  fail_policy,
  output_contract
}
```

Required node kinds:

- `ResolveEntity`;
- `ExecuteQuery`;
- `CardinalityBranch`;
- `ClarificationInterrupt`;
- `ComposeResult`;
- `Complete`.

Every node must declare:

- stable node ID;
- approved capability/query/dataset reference when applicable;
- typed inputs and outputs;
- allowed binding sources;
- policy requirements;
- sensitivity/PII contract;
- timeout and row contribution;
- idempotency behavior;
- retry behavior;
- audit event shape.

The verifier must reject:

- cycles outside an explicitly bounded iteration primitive;
- unknown capabilities or queries;
- type-incompatible bindings;
- data-dependent dynamic SQL identifiers;
- missing office-scope enforcement;
- plans exceeding budget;
- unsupported partial-result behavior;
- output composition that widens sensitivity.

## Required parameter acquisition model

Catalog contracts must distinguish at least:

```yaml
execution_required: true
user_required: false
binding_source: prior_step
resolution_strategy: authorized_data_probe
user_may_supply: false
```

Supported source/strategy concepts must include:

- `authorized_scope`;
- `catalog_default`;
- `verified_user_text`;
- `deterministic_extraction`;
- `exact_sensitive_input`;
- `safe_prior_selection`;
- `prior_step`;
- `authorized_data_probe`.

An execution-required parameter is not automatically a clarification field.

## Required dataset migration

Datasets remain single-data-surface contracts; they do not become workflow engines. They must be expanded so workflows can use them safely.

Required improvements:

1. Add reusable datasets for approved client, office, savings transaction, product, and charge-definition surfaces. Add loan/audit datasets only when those domains are approved.
2. Add safe resolver/probe shapes such as identity candidates, accounts by client, products by client, activity rows, distinct authorized charge definitions, and grouped account counts.
3. Add stable ID filters (`client_id`, `office_id`, `savings_account_id`, `product_id`) and bounded array/`IN` filters where needed.
4. Define entity keys, expected cardinality, safe clarification labels, and zero/one/many behavior.
5. Define typed output-to-input compatibility without exposing SQL expressions to the LLM.
6. Preserve field sensitivity and projection minimization for every shape.
7. Preserve source-level authorized office predicates; user filters may narrow but never widen scope.
8. Prefer one reviewed grouped query over N+1 fan-out when no result-dependent step requires separate queries.
9. Replace the fixture-specific account activity identity contract with general client/account/activity surfaces; retain latest transaction amount only as an optional specialized disambiguator.

## Required Rig integration

Rig must become the real boundary for the LLM functions it is intended to own.

### Understanding agent

- Structured, schema-validated intent/entity/request-shape extraction.
- Catalog-visible vocabulary only.
- LLM facts remain advisory until deterministically grounded.

### Planning agent

Expose metadata tools, not raw SQL:

- `search_catalog`;
- `inspect_capability`;
- `inspect_dataset`;
- `find_entity_resolver`;
- `find_compatible_next_steps`;
- `propose_workflow`.

Use bounded `dynamic_context`/`dynamic_tools` where measured retrieval quality supports them. All tool descriptions must come from approved catalog metadata.

### Data tools

Only guarded tools may reach data:

- `execute_approved_probe`;
- `execute_approved_capability`.

Their server-side implementation must perform capability, provenance, policy, PII, office-scope, timeout, row-cap, query-budget, and workflow-step validation before calling the repository. Rig never receives a raw-SQL tool.

### Turn and tool budgets

Rig `max_turns` is mandatory but insufficient. The runtime must also enforce workflow node, query, row, timeout, retry, and parallelism budgets.

### Response agent

Optional grounded prose generation may consume only policy-filtered structured fields. Deterministic structured response remains authoritative. The LLM must not receive hidden identifiers, raw SQL, secrets, or unrestricted transactional rows.

## Required Petgraph integration

Petgraph must drive, not merely validate after the fact:

- compile verified workflow nodes and edges;
- reject cycles;
- compute dependency order;
- identify runnable nodes;
- support bounded branch targets;
- expose deterministic runtime transitions;
- validate resume targets.

Custom Rust remains responsible for execution side effects and persistence. A graph library is not an authorization or checkpoint store.

## Durable workflow and clarification

A paused job must persist at least:

```text
workflow_id
workflow_version
current/runnable node IDs
workflow revision
completed node outputs or durable result references
parameter provenance
query/row/time budget consumed
pending clarification
principal/policy projection
```

Clarification response must compare job, workflow, node, clarification ID, and revision, then continue from the recorded node. It must not reroute the original request from the beginning or re-run completed probes without an explicit replay decision.

Zero/one/many policy:

- zero -> safe empty/not-found branch;
- one -> auto-bind a stable authorized ID;
- many -> clarify only if selection changes the answer;
- many + semantically valid "all" -> expose a typed `all` choice, never require `-` or a fake placeholder.

## Security invariants

The migration must preserve and test all of the following:

1. Chat authentication remains bearer session JWT plus admin role.
2. Office authorization is bound inside every approved SQL statement.
3. A user-supplied office/entity filter can narrow scope only.
4. LLMs never generate or edit SQL.
5. Only catalog-approved capability/query/dataset resources execute.
6. Every workflow node independently passes policy and PII checks; optional batch preflight cannot replace per-node enforcement.
7. Account numbers and equivalent sensitive selectors remain transient, exact-match, non-projected, sanitized, and rate-limited.
8. Probe outputs are minimal and bounded; safe labels must not disclose unauthorized PII.
9. Fineract transactional rows are not indexed into vector storage.
10. Query count, total rows, total timeout, retries, model turns, and parallelism are bounded.
11. Tool outputs are treated as untrusted content for prompt-injection purposes and as authoritative data only through their typed schema.
12. Client-facing errors contain no SQL, prompts, stack traces, credentials, or hidden identifiers.
13. Every execution and branch has durable audit lineage.
14. Partial composite results are forbidden unless a reviewed workflow contract explicitly permits and labels them.

## Mandatory legacy deletion

### Definition of legacy

Legacy means any superseded runtime, adapter, fallback, compatibility facade, hardcoded branch, duplicated state path, or framework marker that can affect production behavior after the new runtime is accepted. Historical migrations and persisted-data readers required for safe database compatibility are not production behavior fallbacks, but must have an explicit retirement date and deletion gate.

### Final-state rule

Implementation may use short-lived migration flags during development, but the issue cannot resolve while any production configuration can select the old path. At completion:

- one router;
- one planner/workflow compiler;
- one parameter acquisition model;
- one clarification continuation path;
- one workflow runner;
- one approved executor path;
- one structured response authority.

### Mandatory deletion inventory

The design and implementation plan must locate, replace, and delete at least:

1. Atomic-only planner assumptions and singular execution branches superseded by the workflow compiler.
2. Pre-query missing-parameter clarification logic that ignores acquisition strategy.
3. Generic clarification branches that treat entity option IDs as capability IDs.
4. Hardcoded client-only post-query ambiguity handling after generic resolver nodes ship.
5. Exact capability-ID match arms used to choose entity or continuation behavior.
6. Deterministic/keyword capability shortcuts superseded by the verified Rig understanding/planning boundary.
7. Classifier-first or no-router compatibility paths that can bypass the new runtime.
8. Legacy pipeline modules, facades, aliases, and feature flags not used by the accepted runtime.
9. Duplicate canonical versus legacy parameter-planning implementations.
10. Formatter-specific special cases superseded by structured response composition.
11. `RigLlmClient`'s raw transport implementation if Rig becomes the provider/agent boundary; otherwise the misleading name and unused Rig dependency must be deleted.
12. Compile-time-only Rig marker references.
13. Compile-time-only Swiftide marker references.
14. The misleading `SwiftideIndexPipeline` name if it remains a custom indexer.
15. Petgraph topology code that only validates reconstructed/canned transitions after the workflow runtime replaces it.
16. Deprecated clarification API compatibility projections once the frontend migration gate is satisfied.
17. Stale tests that assert legacy paths, fixed catalog counts, or obsolete response strings instead of behavior/security contracts.
18. Stale docs describing one-query terminal execution as the target architecture.

### Proving 100% removal

The implementation plan must include a machine-checkable legacy inventory. Resolution requires:

- repository searches for every removed symbol/module/feature flag;
- no production call graph edge to a legacy entrypoint;
- no configuration key that enables a legacy runtime;
- no test fixture depending on the legacy behavior except explicit migration-data tests;
- dependency audit showing no framework retained only as a marker;
- architecture documentation matching the actual runtime;
- deletion diff reviewed separately from the new behavior diff.

"Deprecated but still reachable", "kept just in case", and "fallback if the agent fails" do not satisfy this issue. Failure must follow the new runtime's explicit safe failure states.

## Required migration phases

### Phase 0 — Source-of-truth design and inventory

- Write the design spec and implementation plan.
- Freeze security invariants and public response/clarification contracts.
- Build an exhaustive legacy symbol/module/flag inventory.
- Record the real current framework usage and catalog/dataset counts.

### Phase 1 — Honest framework boundaries

- Implement the real Rig provider/structured-agent boundary.
- Rename/remove false framework integrations.
- Decide and implement real Swiftide indexing or remove it.
- Preserve provider retry, pricing, tracing, and sanitized error behavior.

### Phase 2 — Resource and acquisition graph

- Add capability kinds: terminal, resolver, probe, composite support.
- Add typed consumes/produces/binding/acquisition metadata.
- Add resolver/probe dataset shapes and stable identifiers.

### Phase 3 — Workflow compiler and verifier

- Define versioned workflow contracts.
- Use Rig to propose from approved resources.
- Use Rust/Petgraph to validate and compile.
- Add policy preflight and all budgets.

### Phase 4 — Durable sequential and conditional runner

- Execute dependency steps.
- Inspect cardinality.
- Persist per-node state.
- Pause/resume clarification at a node.
- Prevent replay and N+1 behavior.

### Phase 5 — Composite and bounded iterative execution

- Add parallel plans, comparisons, grouped summaries, and bounded iteration.
- Add fail-fast cancellation and shared budgets.

### Phase 6 — Structured composition and grounded response

- Compose multi-step results deterministically.
- Add optional Rig prose over filtered structured results.
- Preserve frontend-compatible structured responses through an explicit migration window.

### Phase 7 — Legacy deletion

- Remove every inventory item.
- Remove all old feature flags/fallbacks.
- Remove misleading framework markers and unused dependencies.
- Update docs and tests to the one-runtime truth.

### Phase 8 — Acceptance and rollout

- Run scenario, security, restart/recovery, concurrency, and real-database suites.
- Measure clarification rate, query count, latency, and wrong-answer rate.
- Resolve only after the legacy-removal gate passes.

## Required acceptance scenarios

### Data-aware savings activity

```text
show all savings account activities for Nathalie Doe
```

Expected:

- client resolver probe executes within office scope;
- zero client -> not found;
- one client -> automatic continuation;
- duplicate clients -> safe client clarification;
- one account -> automatic activity execution;
- multiple accounts -> account options plus `all` when valid;
- no mandatory latest transaction amount;
- no raw account number exposure.

### Office existence and client portfolio

```text
if office X exists, show 10 clients and each client's loan and savings account counts
```

Expected:

- office resolver runs first;
- zero/one/many branch is data-aware;
- client selection is bounded to 10;
- counts use an approved grouped query or another bounded reviewed plan, not uncontrolled N+1;
- deferred loan coverage returns an honest planned/unsupported state until approved; it must not fabricate or substitute savings data.

### Charge type

A charge request without an exact charge name must use an authorized bounded charge-definition probe when the intent is otherwise clear. It clarifies only among actual safe choices or offers a valid all-types path.

### Composite comparison

```text
compare deposits and withdrawals this month
```

Expected:

- two approved plans share identical scope and temporal facts;
- policy passes for both before execution;
- bounded parallel execution;
- deterministic combined response;
- no partial unlabeled answer.

### Sensitive account lookup

An account-number request never enumerates candidates. It requires exact transient input, enforces office scope, masks output, and gives indistinguishable safe responses for unauthorized and nonexistent identifiers.

### Recovery

A workflow paused after a successful probe resumes at the waiting node after process restart. It does not reroute or rerun the probe unless replay policy explicitly requires it.

### Adversarial planning

LLM proposals containing unknown tools, cycles, raw SQL, scope widening, type-invalid bindings, excess nodes, or hidden PII projections are deterministically rejected and execute no query.

## Required tests and evidence

The implementation plan must include:

- pure workflow schema/verifier tests;
- Petgraph cycle/topological-order tests;
- Rig structured/tool-loop contract tests with fake models;
- parameter acquisition precedence tests;
- zero/one/many resolver tests;
- same-job clarification node resume tests;
- duplicate/stale clarification tests;
- restart and replay/idempotency tests;
- per-node policy and office-scope tests;
- PII minimization tests;
- query/model/row/timeout budget tests;
- composite cancellation tests;
- dataset validation and output-binding tests;
- end-to-end real-database scenarios;
- source searches proving legacy deletion;
- dependency checks proving Rig/Swiftide are real integrations or absent.

## Operational success metrics

Capture before/after baselines for:

- clarification requests per completed job;
- clarification requests resolved by a safe probe;
- repeated clarification rate;
- incorrect capability selection rate;
- unsupported rate for in-scope prompts;
- average/max queries per job;
- N+1 detection count;
- workflow timeout/cancellation rate;
- model turns and tool calls per job;
- workflow restart recovery success;
- policy-blocked tool proposals;
- PII redaction violations;
- response grounding failures.

A lower clarification rate is not success if wrong-answer or unauthorized-data rates increase.

## Non-goals

- LLM-generated SQL.
- Arbitrary table/column access.
- Unbounded autonomous agents.
- Replacing PostgreSQL durable truth with in-memory Rig or graph state.
- Indexing transactional Fineract rows into vector storage.
- Promoting deferred loan/audit/tax/accounting domains without their own approved knowledge and policy work.
- Splitting every large reviewed SQL into multiple queries; one grouped approved query remains preferable when no data dependency requires a workflow.
- Keeping a legacy runtime as a production fallback.

## Definition of done

This issue is resolved only when all are true:

1. The production request path is one verified workflow runtime.
2. Atomic, sequential, conditional, composite, and bounded iterative contracts exist and are tested.
3. Data-aware probes precede clarification whenever the acquisition contract permits them.
4. Clarification resumes the exact persisted workflow node.
5. Rig is the real structured agent/tool boundary, not a marker dependency.
6. Petgraph drives validated workflow dependencies and runtime scheduling decisions.
7. Swiftide is either genuinely used for its supported release features or removed/renamed truthfully.
8. Dataset contracts support the required resolver/probe/output-binding surfaces.
9. Every node preserves approved SQL, authorization, office scope, PII, budgets, and audit lineage.
10. Required acceptance scenarios pass against production-like databases.
11. The complete legacy inventory is deleted and machine-checkable searches are clean.
12. No production feature flag, fallback, alias, facade, or compatibility path can re-enable legacy behavior.
13. Current architecture/status/runtime documentation describes the implemented system exactly.

## Required follow-up documents

After this issue is reviewed, create:

```text
docs/superpowers/specs/2026-08-04-agentic-workflow-runtime-design.md
docs/superpowers/plans/2026-08-04-agentic-workflow-runtime.md
```

The spec must settle contracts and migration boundaries. The plan must contain deletion gates and verification commands; it must not treat legacy cleanup as optional follow-up work.

## Links

- `docs/issues/active/002-semantic-assistant-platform-major-refactor.md`
- `docs/issues/active/003-verified-payload-extraction.md`
- `docs/issues/active/005-unified-agentic-clarification-contract.md`
- `docs/issues/active/009-conversational-drill-down.md`
- `docs/decisions/2026-07-12-assistant-orchestration-framework.md`
- `docs/superpowers/specs/2026-07-31-dataset-model-design.md`
- `docs/architecture/ai-reporting-design/17-18-planned-architecture-changes.md`
- `crates/chat/src/assistant/llm/rig_client.rs`
- `crates/chat/src/assistant/state/graph.rs`
- `crates/chat/src/assistant/execution/plan/mod.rs`
- `crates/chat/src/assistant/execution/runtime/execution.rs`
- `crates/chat/src/knowledge/dataset/`
- `crates/chat/src/knowledge/index/swiftide.rs`
- `knowledge/datasets/`
