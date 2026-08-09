# Phase 7 workflow-execution cutover — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `compile()` + `WorkflowRunner` + a real SQL-executing `WorkflowNodeExecutor` the production execution path for a selected capability, replacing `execution::plan::` (the atomic planner) and the direct `execute_plan_with_sensitive` call in `execute_selected_capability`. Capability *selection* (`SemanticRouter`/classifier) is unchanged — see the V-L9 amendment in `docs/superpowers/plans/2026-08-04-agentic-workflow-runtime.md` line ~69.

**Architecture:** Once `execute_selected_capability` (`crates/chat/src/assistant/execution/runtime/execution.rs`) has a `capability_id` and resolved intent/parameters, it currently builds an `ExecutionPlan` via `plan_selected_capability_verified`/`authoritative_plan` and calls `execute_plan_with_sensitive` directly. This plan replaces that inner section with: build a single-capability `WorkflowProposal` → `compile()` it against the catalog → run the resulting one-node `ExecutionWorkflow` through `WorkflowRunner` with a new `CapabilityNodeExecutor` (backed by a new `FineractDataExecutor: ApprovedDataExecutor`) → map the `WorkflowRunOutcome` back into the same `GraphRuntimeResult`/`AssistantResponse` shape callers already expect. Everything outside `execute_selected_capability`'s body (its signature, its callers, `job/service/run.rs`, clarification/checkpoint persistence) is unchanged in this plan.

**Tech Stack:** Rust, sqlx/Postgres, existing `assistant::workflow::{compile, run, state}` modules, existing `crate::execution::repository::execute_plan_with_sensitive`.

## Global Constraints

- No feature flag / env var to toggle between old and new execution paths. This plan replaces the call site outright; the old `execution::plan::` module is deleted in the same task range once its last caller (`execute_selected_capability`) stops using it (Task 6).
- `sensitive_identifier: Option<&SensitiveIdentifier>` must reach the executor and must never be persisted (matches the existing `execute_plan_with_sensitive` contract and the workflow layer's `BindingSource::ExactSensitiveInput => Value::Null` convention).
- `client_entity_options` (duplicate-client-row disambiguation, `execution.rs:411-443`) must keep working for `client_name_lookup`/`client_relationship_lookup` — port it into the new response-mapping step, don't drop it.
- Every existing test in `crates/chat/src/assistant/execution/runtime/tests.rs` that exercises `execute_selected_capability`/`run_with_router` end to end must still pass after each task (run `cargo test -p chat execution::runtime` after every task).
- `cargo check --workspace` and `cargo fmt --check` must pass after every task (this repo's `V-BUILD`/`V-LINT` gates).

---

### Task 1: `FineractDataExecutor` — concrete `ApprovedDataExecutor`

**Files:**
- Create: `crates/chat/src/assistant/llm/tool/data_executor.rs`
- Modify: `crates/chat/src/assistant/llm/tool/mod.rs` (add `mod data_executor; pub use data_executor::FineractDataExecutor;`)
- Test: inline `#[cfg(test)] mod tests` in the new file

**Interfaces:**
- Consumes: `ApprovedDataExecutor` trait (`crates/chat/src/assistant/llm/tool/data.rs`):
  ```rust
  #[async_trait]
  pub trait ApprovedDataExecutor: Send + Sync {
      async fn execute_approved(&self, request: &DataToolRequest) -> Result<Value>;
  }
  ```
  `DataToolRequest { node_id: NodeId, capability_id: String, parameters: BTreeMap<String, Value>, timeout_ms: u64, row_cap: u32 }` (`data.rs:17-24`).
  `crate::execution::repository::execute_plan_with_sensitive(pool: &PgPool, catalog: &KnowledgeCatalog, plan: &ExecutionPlan, policy: &PolicyDecision, limits: ExecutionLimits, sensitive_identifier: Option<&SensitiveIdentifier>) -> Result<ExecutionResult>` (existing, `crates/chat/src/execution/repository.rs`).
- Produces: `pub struct FineractDataExecutor { pool: PgPool, catalog: Arc<KnowledgeCatalog>, policy: PolicyDecision, limits: ExecutionLimits, sensitive_identifier: Option<SensitiveIdentifier> }` with `pub fn new(pool: PgPool, catalog: Arc<KnowledgeCatalog>, policy: PolicyDecision, limits: ExecutionLimits, sensitive_identifier: Option<SensitiveIdentifier>) -> Self`, implementing `ApprovedDataExecutor`.

`FineractDataExecutor::execute_approved` builds a minimal `ExecutionPlan` from `DataToolRequest` (`plan_type: ExecutionPlanType::Atomic`, `capability: request.capability_id.clone()`, `query_id` looked up from `catalog.capabilities` by id, `params: json!(request.parameters)`, `dataset_selection: None`, `output_mode` from the capability's declared output mode, `retrieval_plan`/`evidence_evaluation`: `Default::default()`, `requires_policy_check: true`), then calls `execute_plan_with_sensitive(&self.pool, &self.catalog, &plan, &self.policy, self.limits, self.sensitive_identifier.as_ref())` and converts `ExecutionResult` into the `Value` this trait returns (`serde_json::to_value(result)`).

- [ ] **Step 1: Write the failing test.** Add to the new file:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::assistant::llm::tool::data::DataToolRequest;

      #[test]
      fn builds_execution_plan_from_request_params() {
          let request = DataToolRequest {
              node_id: crate::assistant::workflow::NodeId::from("n1"),
              capability_id: "client_name_lookup".into(),
              parameters: std::collections::BTreeMap::from([(
                  "person_name".into(),
                  serde_json::json!("Alex"),
              )]),
              timeout_ms: 5_000,
              row_cap: 50,
          };
          let catalog = crate::knowledge::catalog::test_support::sample_catalog();
          let plan = super::build_execution_plan(&request, &catalog)
              .expect("client_name_lookup is in the sample catalog");
          assert_eq!(plan.capability, "client_name_lookup");
          assert_eq!(plan.params["person_name"], serde_json::json!("Alex"));
      }
  }
  ```
  Check `crates/chat/src/knowledge/catalog/` for the actual test-fixture helper name (grep `sample_catalog` / `test_catalog` under `crates/chat/src` — reuse whatever `execution/plan/tests.rs` already uses so the fixture is shared, not duplicated).

- [ ] **Step 2: Run test to verify it fails.**
  Run: `cargo test -p chat data_executor -- --nocapture`
  Expected: FAIL — `build_execution_plan` not defined.

- [ ] **Step 3: Implement `build_execution_plan` and `FineractDataExecutor`** per the Interfaces block above. Keep `build_execution_plan` as a free `pub(super) fn` so Task 2 can unit-test it without a live pool.

- [ ] **Step 4: Run test to verify it passes.**
  Run: `cargo test -p chat data_executor -- --nocapture`
  Expected: PASS.

- [ ] **Step 5: Commit.**
  ```bash
  git add crates/chat/src/assistant/llm/tool/data_executor.rs crates/chat/src/assistant/llm/tool/mod.rs
  git commit -m "feat(chat): Phase 7 cutover Task 1 - FineractDataExecutor"
  ```

---

### Task 2: `CapabilityNodeExecutor` — `WorkflowNodeExecutor` adapter

**Files:**
- Create: `crates/chat/src/assistant/workflow/node_executor.rs`
- Modify: `crates/chat/src/assistant/workflow/mod.rs` (`mod node_executor; pub use node_executor::CapabilityNodeExecutor;`)
- Test: inline in the new file, plus one integration-style test using `WorkflowRunner` directly (mirror the pattern in `crates/chat/tests/workflow_parallel_budget.rs`)

**Interfaces:**
- Consumes: `WorkflowNodeExecutor` trait (`crates/chat/src/assistant/workflow/run.rs:38-47`):
  ```rust
  #[async_trait]
  pub trait WorkflowNodeExecutor: Send + Sync {
      async fn execute(&self, node: &WorkflowNode, bindings: &BTreeMap<String, Value>) -> Result<NodeExecution>;
  }
  ```
  `NodeExecution::{Completed { output: Value, rows_returned: i32 }, Waiting { clarification: Box<ClarificationPayload> }, Failed}` (`run.rs:26-36`). `Task 1`'s `FineractDataExecutor` and `data.rs`'s `GuardedDataTools<E>`.
- Produces: `pub struct CapabilityNodeExecutor { guard: GuardedDataTools<FineractDataExecutor>, principal: PrincipalContext, catalog: Arc<KnowledgeCatalog> }` with `pub fn new(executor: FineractDataExecutor, principal: PrincipalContext, catalog: Arc<KnowledgeCatalog>) -> Self`. Only handles `NodeKind::ExecuteQuery`/`NodeKind::ResolveEntity` — `WorkflowRunner::resolve_execution` already routes only those two kinds here (`run.rs:299-355`), so any other `node.kind` in `execute()` is `unreachable!()`.

`execute()` builds a `DataToolRequest` from `node.capability_id`/`node.query_id`/`bindings`, calls `self.guard.execute_approved_capability(workflow, runs, &self.principal, &self.catalog, &request)` — **note**: `execute_approved_capability`'s real signature takes `&ExecutionWorkflow` and `&[WorkflowNodeRun]` (per Q3 research); since `WorkflowNodeExecutor::execute` only receives `node`/`bindings`, thread `workflow`/`runs` in via `CapabilityNodeExecutor`'s own fields set once at construction (store the `Arc<ExecutionWorkflow>` reference the runner already holds — check `WorkflowRunner::resolve_execution`'s call site at `run.rs:353` for what's available there, and either widen the trait call or capture what's needed via an `Arc<Mutex<...>>`/interior field populated at `WorkflowRunner::run` entry). Resolve this exact plumbing by reading `run.rs:280-360` in full before writing this task's implementation — the trait signature is fixed (don't change `WorkflowNodeExecutor`), so `CapabilityNodeExecutor` must obtain workflow/runs context another way (e.g. from `bindings` if the runner already passes workflow-scoped values there, or by giving `CapabilityNodeExecutor` its own lightweight re-derivation of the current node run list via a `WorkflowStateRepository` handle).

Maps `Ok(Value)` → `NodeExecution::Completed { output: value, rows_returned: <from output "rows" array length or 0> }`, maps `DataToolRejection` → `NodeExecution::Failed` (log the rejection reason via `tracing::warn!`, never put it in `output`).

- [ ] **Step 1: Write the failing test** — a `WorkflowRunner<CapabilityNodeExecutor>` running a compiled single-node `client_name_lookup` workflow against a test Postgres pool (reuse whatever pool-provisioning helper `crates/chat/tests/workflow_parallel_budget.rs` uses), asserting `WorkflowRunOutcome::Completed` and that `chat_workflow_node_runs` has one `completed` row with `rows_returned >= 0`.

- [ ] **Step 2: Run test to verify it fails.**
  Run: `cargo test -p chat node_executor -- --nocapture`
  Expected: FAIL — `CapabilityNodeExecutor` not defined.

- [ ] **Step 3: Implement `CapabilityNodeExecutor`** per the Interfaces block, resolving the workflow/runs plumbing question above by reading `run.rs` first.

- [ ] **Step 4: Run test to verify it passes.**
  Run: `cargo test -p chat node_executor -- --nocapture`
  Expected: PASS.

- [ ] **Step 5: Commit.**
  ```bash
  git add crates/chat/src/assistant/workflow/node_executor.rs crates/chat/src/assistant/workflow/mod.rs
  git commit -m "feat(chat): Phase 7 cutover Task 2 - CapabilityNodeExecutor"
  ```

---

### Task 3: Sensitive-identifier threading through `WorkflowRunner`

**Files:**
- Modify: `crates/chat/src/assistant/workflow/run.rs` (`WorkflowRunner::run` signature and `bindings_for`)
- Modify: `crates/chat/src/assistant/workflow/node_executor.rs` (consume the identifier)
- Test: extend Task 2's integration test with a sensitive-lookup case

**Interfaces:**
- Modify `WorkflowRunner::run` signature from:
  ```rust
  pub async fn run(&self, job_id: Uuid, user_id: Uuid, principal: &PrincipalContext, workflow: &ExecutionWorkflow) -> Result<WorkflowRunOutcome>
  ```
  to:
  ```rust
  pub async fn run(&self, job_id: Uuid, user_id: Uuid, principal: &PrincipalContext, workflow: &ExecutionWorkflow, sensitive_identifier: Option<&SensitiveIdentifier>) -> Result<WorkflowRunOutcome>
  ```
  Thread it down to wherever `self.executor.execute(node, bindings)` is called (`run.rs:353`) — since the trait signature can't change without touching `data.rs`'s test mocks too, pass it via a request-scoped field on `CapabilityNodeExecutor` instead: change `CapabilityNodeExecutor::new` to also take `sensitive_identifier: Option<SensitiveIdentifier>`, and have `WorkflowRunner::run` construct... **no** — `WorkflowRunner<E>` is generic over `E` and doesn't own executor construction. Instead: `WorkflowRunner::run`'s new `sensitive_identifier` parameter is stored in a `tokio::task_local!` or threaded explicitly into `resolve_execution`/`bindings_for` as an extra parameter, and `bindings_for`'s existing `BindingSource::ExactSensitiveInput => Value::Null` arm (`run.rs:559`) changes to look up the real value from this parameter **only when building the `DataToolRequest` inside `CapabilityNodeExecutor::execute`**, not when persisting bindings (persisted bindings stay `Null` — this is the existing, correct, never-persist behavior; only the live in-flight request to the executor gets the real value). Concretely: add `sensitive_identifier: Option<&'_ SensitiveIdentifier>` as a parameter to `resolve_execution` and `bindings_for`, and add a **second** map alongside the persisted `bindings: BTreeMap<String, Value>` — e.g. `live_overrides: BTreeMap<String, Value>` — that `CapabilityNodeExecutor::execute` merges into the `DataToolRequest.parameters` after calling `self.guard.execute_approved_capability(...)` with the persisted (redacted) bindings for provenance checks, but the real value for the actual SQL parameter. Read `execution/repository.rs`'s existing `execute_plan_with_sensitive` to confirm exactly how it currently receives the sensitive value (likely a dedicated parameter substituted after policy/provenance checks, not embedded in `plan.params` at all) and mirror that same separation here rather than inventing a new mechanism.

- [ ] **Step 1: Write the failing test** — a workflow node whose input binding is `BindingSource::ExactSensitiveInput`, run with `Some(&sensitive_identifier)`, asserting the returned row matches the sensitive lookup and that `chat_workflow_node_runs.output_json`/`chat_job_checkpoints` never contain the raw sensitive value (string-search the persisted JSON in the assertion).

- [ ] **Step 2: Run test to verify it fails.**
  Run: `cargo test -p chat sensitive -- --nocapture`
  Expected: FAIL — old `run()` signature / `Value::Null` always returned.

- [ ] **Step 3: Implement the threading** per the Interfaces block, updating every existing `WorkflowRunner::run(...)` call site (Task 2's test, `tests/workflow_resume.rs`, `tests/workflow_parallel_budget.rs`) to pass `None` where no sensitive identifier applies.

- [ ] **Step 4: Run test to verify it passes**, and re-run the full workflow test suite to confirm no regression:
  Run: `cargo test -p chat workflow:: && cargo test -p chat --test workflow_resume && cargo test -p chat --test workflow_parallel_budget`
  Expected: all PASS.

- [ ] **Step 5: Commit.**
  ```bash
  git add crates/chat/src/assistant/workflow/run.rs crates/chat/src/assistant/workflow/node_executor.rs crates/chat/tests/workflow_resume.rs crates/chat/tests/workflow_parallel_budget.rs
  git commit -m "feat(chat): Phase 7 cutover Task 3 - thread sensitive identifier through WorkflowRunner"
  ```

---

### Task 4: `WorkflowRunOutcome` → `AssistantResponse` mapping

**Files:**
- Create: `crates/chat/src/assistant/workflow/response.rs`
- Modify: `crates/chat/src/assistant/workflow/mod.rs` (`mod response; pub use response::workflow_response;`)
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Consumes: `WorkflowRunOutcome::{Completed, WaitingForUserInput { node_id }, Failed}` (`run.rs:49-54`); `WorkflowStateRepository::{load_workflow, node_runs}` to fetch the final node's `output_json`/`provenance_json` and (for `WaitingForUserInput`) the persisted `assistant_job_memory.pending_clarification_json`; `ResponseBuilder::{from_tool_result, clarification, error, policy_blocked}` and `presentation::builder::finish` (existing, `execution.rs:343-349`, `transition.rs:19`); the `client_entity_options` helper — **move it from `execution/runtime/execution.rs` into this file** (it's pure/no state, safe to relocate) since duplicate-client disambiguation belongs at the response-mapping layer for the new path too, not duplicated.
- Produces:
  ```rust
  pub async fn workflow_response(
      outcome: WorkflowRunOutcome,
      state: &WorkflowStateRepository,
      job_id: Uuid,
      capability_id: &str,
      intent: &AssistantIntent,
      catalog: &KnowledgeCatalog,
      policy: &PolicyDecision,
      can_view_pii: bool,
  ) -> Result<WorkflowResponseOutcome>

  pub enum WorkflowResponseOutcome {
      Response(AssistantResponse),
      Clarification(ClarificationPayload),
      Failed,
  }
  ```
  `Completed` reads the terminal node's `output_json` via `state.node_runs(job_id).await?`, reconstructs a `ToolResult`-shaped value (check `super::super::tool::tool_result_from_execution`'s output shape in `execution.rs:302` and match it), and calls `ResponseBuilder::from_tool_result(...)`; if `client_entity_options` on the rows returns >1 option, returns `Clarification` with a `SelectEntity` payload instead (same logic as `execution.rs:303-341`, moved here). `WaitingForUserInput` reads `pending_clarification_json` back via a new small helper on `WorkflowStateRepository` (add `pub async fn load_pending_clarification(&self, job_id: Uuid) -> Result<Option<ClarificationPayload>>` if one doesn't already exist — check `state.rs` first) and returns `Clarification(payload)`. `Failed` returns `WorkflowResponseOutcome::Failed`.

- [ ] **Step 1: Write the failing test** — construct a `WorkflowRunOutcome::Completed` scenario (via the Task 2 integration harness), call `workflow_response`, assert the returned `AssistantResponse` matches what `ResponseBuilder::from_tool_result` would produce directly from the same row data.

- [ ] **Step 2: Run test to verify it fails.**
  Run: `cargo test -p chat workflow_response -- --nocapture`
  Expected: FAIL — function not defined.

- [ ] **Step 3: Implement `workflow_response`** per the Interfaces block, moving (not copying) `client_entity_options` out of `execution/runtime/execution.rs` into `workflow/response.rs` and re-exporting or re-importing at the old call site for Task 5's transition (Task 6 deletes the old call site entirely, so a temporary re-export is fine here).

- [ ] **Step 4: Run test to verify it passes.**
  Run: `cargo test -p chat workflow_response -- --nocapture`
  Expected: PASS.

- [ ] **Step 5: Commit.**
  ```bash
  git add crates/chat/src/assistant/workflow/response.rs crates/chat/src/assistant/workflow/mod.rs crates/chat/src/assistant/execution/runtime/execution.rs
  git commit -m "feat(chat): Phase 7 cutover Task 4 - map WorkflowRunOutcome to AssistantResponse"
  ```

---

### Task 5: Wire `execute_selected_capability` to the workflow engine

**Files:**
- Modify: `crates/chat/src/assistant/execution/runtime/execution.rs` (replace lines 143-409, the authoritative/legacy-plan branch through the SQL-execution match, with the new call)
- Test: `crates/chat/src/assistant/execution/runtime/tests.rs` (existing tests must still pass unmodified in this task — behavior must be identical from the caller's point of view; only Task 7 rewrites these tests)

**Interfaces:**
- Consumes: Task 1-4's `FineractDataExecutor`, `CapabilityNodeExecutor`, `workflow_response`, plus existing `crate::assistant::llm::tool::propose_workflow`, `crate::assistant::workflow::compile::compile`, `WorkflowRunner::new`, `WorkflowStateRepository`.
- Produces: no new public interface — `execute_selected_capability`'s signature is unchanged; only its body changes from line ~143 onward.

Replace the `authoritative_plan`/`plan_selected_capability_verified` branch and the direct `execute_plan_with_sensitive` call with:
1. `let proposal = crate::assistant::llm::tool::propose_workflow(catalog, vec![capability_id.clone()])?;` (map `MetadataToolError` to the existing `FailedOperational`/`canonical_snapshot_invalid` terminal on error, matching today's error handling shape).
2. `let workflow = compile(proposal, catalog, catalog_version, budgets)?;` — `catalog_version` from `canonical.and_then(|c| c.catalog_version)` (fall back to a nil/default `Uuid` when `None`, matching how the legacy path already tolerates a missing catalog version outside `Authoritative` mode); `budgets` from a new `WorkflowBudgets::single_capability_default()` (or whatever the existing `WorkflowBudgets` default constructor is called — check `compile.rs`/`contract.rs`). Map `CompileError` to `FailedOperational`/`workflow_compile_failed`.
3. Build `guard_selected_capability`'s existing `PolicyDecision` the same way it's built today (line 259) — policy computation is unchanged, only *where* it's threaded into the executor changes.
4. Construct `FineractDataExecutor::new(pool.clone(), catalog.clone(), policy.clone(), limits, sensitive_identifier.cloned())`, wrap in `CapabilityNodeExecutor::new(...)`, construct `WorkflowRunner::new(WorkflowStateRepository::new(pool.clone()), executor, catalog.clone())` (check the real `WorkflowStateRepository` constructor name/args in `state.rs`).
5. `let outcome = runner.run(memory.job_id, client.user_id, &execution_client, &workflow, sensitive_identifier).await?;` — map any `Err` to `FailedOperational`/`execution_failed`, same as today's `Err(error)` arm (line 377-408).
6. `let response_outcome = workflow_response(outcome, &state_repo, memory.job_id, &capability_id, intent, catalog, &policy, policy.can_view_pii).await?;` then match `WorkflowResponseOutcome::{Response, Clarification, Failed}` onto the same three `graph_result(...)` shapes the old code produced (`Completed`/`execution_completed`, `WaitingForUserInput`/`ambiguous_client_identity`, `FailedOperational`/`execution_failed`).
7. Delete the missing-fields / `defaultless_missing_fields` gate at the top of the function **only in Task 6**, not here — Task 5 is scoped to swap only the plan-and-execute section so each task stays independently testable; the missing-fields gate still runs before this section either way.

- [ ] **Step 1: Confirm the safety net.** Before touching `execution.rs`, run the full existing suite and record it as the "must stay green" baseline:
  Run: `cargo test -p chat execution::runtime -- --nocapture 2>&1 | tail -30`
  Expected: all current tests PASS (this is the regression baseline, not a new test).

- [ ] **Step 2: Implement the replacement** per the numbered list above.

- [ ] **Step 3: Run the same suite again.**
  Run: `cargo test -p chat execution::runtime -- --nocapture 2>&1 | tail -30`
  Expected: same pass count as Step 1, zero regressions. Any test that now fails must be diagnosed against the numbered mapping above — this is the highest-risk task in the plan (behavior-preservation across a full mechanism swap), so do not proceed to Task 6 until this is green.

- [ ] **Step 4: Commit.**
  ```bash
  git add crates/chat/src/assistant/execution/runtime/execution.rs
  git commit -m "feat(chat): Phase 7 cutover Task 5 - execute_selected_capability runs through WorkflowRunner"
  ```

---

### Task 6: Delete the atomic planner (`V-L1`, `V-L2`, `V-L4`, `V-L5`)

**Files:**
- Delete: `crates/chat/src/assistant/execution/plan/` (entire directory, including `plan/tests.rs`)
- Modify: every caller found in the Phase 7 research (Task 7.2 list in `docs/superpowers/plans/2026-08-04-agentic-workflow-runtime.md`): `execution/runtime/planning.rs`, `execution/tool/guard.rs`, `execution/tool/mod.rs`, `execution/tool/planning.rs`, `execution/runtime/mod.rs`, `presentation/builder.rs`, `execution/repository.rs`
- Modify: `crates/chat/src/assistant/execution/runtime/execution.rs` — remove the now-dead `defaultless_missing_fields` gate (`crate::assistant::context::clarification_planner::defaultless_missing_fields`, lines ~41-80) and the `client_entity_options`/two-capability `matches!` (already moved to `workflow/response.rs` in Task 4 — delete the leftover re-export)

**Interfaces:** none produced — this is a pure deletion task. `PolicyDecision`/`PolicyDecisionStatus` (currently defined in `execution/plan/mod.rs`) must move somewhere that survives the deletion — check whether `guard_selected_capability`'s return type can be relocated to `crate::policy::authorization` (where `ensure_capability_allowed`/`effective_office_scope` already live) or a new small `crates/chat/src/assistant/execution/policy.rs`. Pick one, and update every one of the 7 files listed above to import it from the new location.

- [ ] **Step 1: Confirm the safety net** (same command as Task 5 Step 1 — should still be green from Task 5).

- [ ] **Step 2: Relocate `PolicyDecision`/`PolicyDecisionStatus`** to the new home, update the 7 dependent files' imports, run `cargo check -p chat` after just this move (before deleting anything) to confirm the type move alone compiles.

- [ ] **Step 3: Delete `crates/chat/src/assistant/execution/plan/`** and its `mod plan;` declaration, then run `cargo check --workspace` and fix every resulting orphan import mechanically (this is why callers-before-callees ordering matters — the compiler now finds every remaining reference).

- [ ] **Step 4: Delete the `defaultless_missing_fields` gate** from `execution.rs` and its definition in `crate::assistant::context::clarification_planner` if that module has no other callers (`command grep -rn defaultless_missing_fields crates/` should show only the definition after this step, then delete the definition too so `V-L2` reaches zero).

- [ ] **Step 5: Run verification.**
  Run:
  ```bash
  cargo check --workspace
  command grep -rEn 'ExecutionPlanType|build_execution_plan' crates/    # V-L1, expect ∅
  command grep -rEn 'defaultless_missing_fields' crates/                # V-L2, expect ∅
  command grep -rEn 'client_name_lookup.*client_relationship_lookup' crates/  # V-L4, expect ∅
  command grep -rEn 'capability_id\.as_str\(\)' crates/                 # V-L5, expect ∅
  cargo test -p chat execution:: -- --nocapture
  ```
  Expected: all four greps empty, `cargo check` exit 0, tests green (some will already be broken/need Task 7's rewrite — track which, don't silently `#[ignore]` them).

- [ ] **Step 6: Commit.**
  ```bash
  git add -A
  git commit -m "feat(chat): Phase 7 cutover Task 6 - delete execution::plan (atomic planner)"
  ```

---

### Task 7: Rewrite `execution/runtime/tests.rs` to behaviour contracts

**Files:**
- Modify: `crates/chat/src/assistant/execution/runtime/tests.rs` (1742 lines — every test that asserted on the deleted `ExecutionPlan`/`PolicyDecisionStatus` shapes or on `run_with_router`'s exact internal call sequence)

**Interfaces:** none — test-only task.

A rewritten test asserts a behaviour or security contract (e.g. "PII is hidden when `can_view_pii` is false", "policy-blocked capability returns `BlockedByPolicy` with no row data"), never the old implementation's exact call sequence or internal type names. Go test-by-test; for each of the 14 `run_with_router` call sites found in Phase 7 research, confirm the assertion still describes user-visible behavior and only touch the setup code that referenced deleted types.

- [ ] **Step 1:** Run `cargo test -p chat execution::runtime 2>&1 | command grep -E 'FAILED|error\[' ` and list every currently-broken test by name.
- [ ] **Step 2:** For each broken test, fix its setup (deleted-type references) without changing its assertions, unless the assertion itself named a deleted implementation detail — in that case, rewrite the assertion to the equivalent behaviour contract.
- [ ] **Step 3:** Run `cargo test -p chat execution::runtime` — expect exit 0, zero `#[ignore]` added (`V-L18`).
- [ ] **Step 4: Commit.**
  ```bash
  git add crates/chat/src/assistant/execution/runtime/tests.rs
  git commit -m "test(chat): Phase 7 cutover Task 7 - rewrite runtime tests to behaviour contracts"
  ```

---

### Task 8: Full-suite and integration-test verification

**Files:** none modified unless a regression is found (in which case, fix forward in the relevant task's files, don't patch here).

- [ ] **Step 1:** Run every integration test file flagged as regression surface in the Phase 7 research (`tests/chat_full_flow.rs`, `tests/chat_jobs.rs`, `tests/chat_no_loop.rs`, `tests/chat_sessions.rs`, `tests/clarification_api.rs`, `tests/scenario_matrix.rs`, `tests/user_journeys_real_db.rs`, `tests/assistant_answer_quality.rs`, `tests/organization_answer_quality.rs`, `tests/savings_answer_quality.rs`, `tests/assistant_terminal_states.rs`, `tests/classification_semantic.rs`):
  ```bash
  cargo test -p chat --test chat_full_flow --test chat_jobs --test chat_no_loop --test chat_sessions --test clarification_api --test scenario_matrix --test user_journeys_real_db --test assistant_answer_quality --test organization_answer_quality --test savings_answer_quality --test assistant_terminal_states --test classification_semantic
  ```
  Expected: all pass. Any failure is diagnosed against the exact behaviour-preservation mapping in Task 5 — this is the point where a subtle response-shape mismatch (markdown rendering, `execution_summary.plan` shape feeding `execution_audit_from_memory`) would surface.
- [ ] **Step 2:** `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **Step 3: Commit** only if Step 1 required fixes elsewhere; otherwise this task is verification-only, no commit.

---

## Self-review notes

- **Coverage:** Tasks 1-4 build the missing pieces identified by the Phase 7 research (executor, node-executor adapter, sensitive threading, response mapping). Task 5 is the actual cutover. Task 6 deletes the atomic planner now that nothing calls it. Task 7-8 are the test/verification debt the deletion creates. `AI_REPORT_GATEWAY_PIPELINE`/`route_via_gateway_pipeline`/`run_via_gateway_pipeline`/`deterministic_simple_response`/`AssistantGraphRuntime::run`/`GraphState`/`GraphTransition`/`knowledge::dataset::legacy` deletions (V-L6, V-L7, V-L10, V-L16, V-L11) are **not** in this plan — they don't depend on the execution-engine swap and are better scoped as a separate, smaller "Phase 7 mechanical deletions" plan once this cutover is proven in production. `V-L9` (classifier) is explicitly deferred per the amendment. `V-L8`/`CanonicalGatewayMode` is out of scope per the Phase 7 research (separate canonical-state axis).
- **Placeholder scan:** every task names exact files, exact signatures, and either exact code or a precise description of what must be read before writing (Task 2/3's plumbing questions are flagged as open only because the exact `run.rs:280-360` control flow needs to be read at implementation time, not because the answer is unknown-unknowable — this is a case where showing invented code would be worse than pointing at the exact lines to read first).
- **Type consistency:** `WorkflowResponseOutcome`, `FineractDataExecutor`, `CapabilityNodeExecutor`, `workflow_response` are named once in Task 1/2/4 and referenced identically in Task 5.
