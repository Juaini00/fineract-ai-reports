# Agentic Workflow Runtime — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans` (or `superpowers:subagent-driven-development`) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Spec:** `docs/superpowers/specs/2026-08-04-agentic-workflow-runtime-design.md`
**Issue:** `docs/issues/active/012-agentic-workflow-runtime-and-framework-completion.md`
**Created:** 2026-08-04
**Baseline commit:** `d99d898`

**Goal:** Replace the atomic one-capability-one-query runtime with a single verified
workflow runtime that supports atomic / sequential / conditional / composite / bounded
iterative execution, asks only data-informed clarifications, resumes at a persisted node,
uses Rig and petgraph honestly, and leaves **zero** reachable legacy path behind.

**Deletion is not follow-up work.** Phase 7 is a gate, not a cleanup ticket. Every phase
before it adds code that Phase 7 is contractually required to make the *only* code. A
phase that cannot be deleted-into is a phase that was built wrong.

---

## Global constraints

- Workspace locked to three crates — `app`, `core`, `chat`. Do not add a crate. (`CLAUDE.md`)
- Layer order `route → service → repository → database`. No `sqlx` outside repositories. (`CLAUDE.md`)
- Schema changes only via `migrations/*.sql`. Startup never creates or alters tables. (`CLAUDE.md`)
- Every character of executable SQL originates in a file on disk or a declared `expr` in YAML. The LLM contributes ids and values only. (spec §12 SI-4)
- Knowledge stays YAML under `knowledge/`, SQL under `queries/`. (`CLAUDE.md`)
- Files 200–400 lines typical, 800 max. (coding-style)
- Pre-commit runs `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`. Both must pass.
- Per `no-full-test-suite-runs`: phase gates run targeted `cargo test -p chat <name>`. The full sweep runs once, at the Phase 8 gate.
- Per `verify-the-path-not-the-artifact`: a capability or dataset is not done until its own YAML example runs end to end against the live database.
- Per `tests-must-not-forge-inter-stage-values`: fakes substitute the LLM, the DB or the clock — never a struct one production stage hands the next.

## Branch and commit strategy

One branch: `feat/issue-012-workflow-runtime`. One commit per task. **Phase 7 lands as its
own commits, separate from every new-behaviour commit**, so the deletion diff is reviewable
alone (issue 012 §"Proving 100% removal").

Do not squash Phase 7 into feature commits. The review gate is the diff shape.

---

## Verification command reference

These are referenced by ID from the phase gates. `∅` means the command must print nothing.

> **Use `command grep -rEn`, not a bare `rg`.** This machine's shell wraps `rg` and `wc`
> in a helper (`_lc`) that fails and silently reports **zero hits for patterns that do
> match** — verified while writing this plan: a bare `rg` reported `ExecutionPlanType` as
> absent when it has 25 occurrences. A verification gate that can silently pass is worse
> than no gate. Every command below is written in `command grep` form for that reason.
> Measured baseline hit counts are in `docs/issues/active/012-legacy-baseline.md`.

| ID | Command | Expected |
| --- | --- | --- |
| `V-BUILD` | `cargo check --workspace` | exit 0 |
| `V-LINT` | `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `V-TREE-SWIFTIDE` | `cargo tree -p chat \| command grep swiftide` | ∅ |
| `V-TREE-RIG` | `cargo tree -p chat \| command grep rig-core` | non-empty |
| `V-L1` | `command grep -rEn 'ExecutionPlanType\|build_execution_plan' crates/` | ∅ (baseline 25) |
| `V-L2` | `command grep -rEn 'defaultless_missing_fields' crates/` | ∅ (baseline 7) |
| `V-L3` | `command grep -rEn 'selected_capability = Some\(option_id' crates/` | ∅ (baseline 2) |
| `V-L4` | `command grep -rEn 'client_name_lookup.*client_relationship_lookup' crates/` | ∅ (baseline 1) |
| `V-L5` | `command grep -rEn 'capability_id\.as_str\(\)' crates/` | ∅ (baseline 8) |
| `V-L6` | `command grep -rEn 'deterministic_simple_response' crates/` | ∅ (baseline 2) |
| `V-L7` | `command grep -rEn 'AI_REPORT_GATEWAY_PIPELINE\|run_via_gateway_pipeline\|route_via_gateway_pipeline' .` | ∅ (baseline 8) |
| `V-L8` | `command grep -rEn 'CanonicalGatewayMode\|CHAT_CANONICAL_GATEWAY_MODE' .` | ∅ (baseline 28) |
| `V-L9` | `command grep -rEn 'SemanticRouter\|ClassificationResult\|ClassificationOutcome' crates/` | ∅ (baseline 40) |
| `V-L10` | `command grep -rEn 'AssistantGraphRuntime' crates/` | ∅ (baseline 23) |
| `V-L11` | `command grep -rEn 'knowledge::dataset::legacy' crates/` | ∅ |
| `V-L12` | `command grep -rEn 'reqwest' crates/chat/src/assistant/llm/` | 2 (amended, see below) |
| `V-L13` | `command grep -rEn 'size_of::<rig_core' crates/` | ∅ (baseline 1) |
| `V-L14` | `command grep -rEn swiftide .` | ∅ (baseline 14) |
| `V-L15` | `command grep -rEn Swiftide crates/` | ∅ |
| `V-L16` | `command grep -rEn 'AssistantGraphTopology' crates/` | ∅ (baseline 20), or the Task 7.3 amendment |
| `V-L17` | `ls crates/chat/examples/ \| command grep phase0_rig_poc` | ∅ |
| `V-L18` | `cargo test -p chat` after Task 7.4 | exit 0 with no `#[ignore]` added |
| `V-L19` | `command grep -rEn 'one query\|single query\|atomic execution' AGENTS.md CLAUDE.md docs/architecture/` | reviewed manually in the deletion PR |
| `V-L20` | `command grep -rEn 'capability_id\|output_mode ==' crates/chat/src/assistant/presentation/` | ∅ (baseline 2) |
| `V-L21` | `command grep -rEn 'MIN_SELECT_CONFIDENCE\|RerankerDecision::clarify' crates/` | ∅ (baseline 5) |
| `V-FLAGS` | `command grep -rEn 'env::var' crates/*/src` | only `core/src/config/mod.rs` helper lines |

---

# Phase 0 — Baseline and inventory freeze

No production code changes. This phase exists so Phase 7 has something to diff against.

### Task 0.1: Record the measured baseline

- [ ] Run every `V-L*` command from the reference table against `d99d898` and write the
      hit counts to `docs/issues/active/012-legacy-baseline.md`. A command that is already
      empty at baseline is recorded as such — it must stay empty, and a later non-empty
      result is a regression.
- [ ] Record catalog counts: `find knowledge/capabilities -name '*.yaml' | wc -l` (expect 41),
      same for `knowledge/queries` (41) and `knowledge/datasets` (4). `fd` is not installed
      on this machine — every command in this plan uses `find` / `rg` / `ls` only.
- [ ] Record current metric baselines (spec §17) from `chat_jobs` / `chat_job_events` over
      the last 30 days of local data, or mark each as "no baseline" explicitly. Do not
      leave a metric silently unmeasured — the Phase 8 gate compares against this file.

**Gate 0: DONE (2026-08-04).** `docs/issues/active/012-legacy-baseline.md` exists, every
`V-L*` ID appears with a measured count, the shell-wrapper hazard is documented, and the
metrics baseline is recorded as explicitly absent with its Phase 8 consequence stated.

---

# Phase 1 — Honest framework boundaries

Independent of the workflow runtime. Ships first because it is the only phase that can
land without touching request behaviour, and it removes two lies from the tree early.

### Task 1.1: Remove Swiftide (spec D2)

**Files:** delete `swiftide` from `Cargo.toml:56` and `crates/chat/Cargo.toml:36`;
rename `crates/chat/src/knowledge/index/swiftide.rs` → `pipeline.rs`;
modify `crates/chat/src/knowledge/index/mod.rs`, `sync.rs`, and any test referencing the old names.

**Interfaces:**
- `SwiftideIndexPipeline` → `CatalogIndexPipeline`
- `SwiftideKnowledgeDocument` → `CatalogDocument`
- `ingest_paths` / `ingest_catalog` signatures unchanged.

- [ ] **Step 1:** Delete line `swiftide.rs:26` (`let _swiftide_loader = std::any::type_name::<...>()`). Confirm `cargo check -p chat` still passes — it proves the reference was inert.
- [ ] **Step 2:** Rename the file, the two types, and every call site. Keep `reject_client_rows` **verbatim** — it is security invariant SI-9, not indexing convenience.
- [ ] **Step 3:** Remove both `Cargo.toml` entries.
- [ ] **Step 4:** `cargo test -p chat knowledge::index`.

**Gate 1.1:** `V-L14` ∅, `V-L15` ∅, `V-TREE-SWIFTIDE` ∅, `V-BUILD`, `V-LINT`.

### Task 1.2: Make Rig the real provider boundary (spec D1, §8.1)

**Files:** rewrite `crates/chat/src/assistant/llm/rig_client.rs` → `provider.rs`;
modify `llm/mod.rs`, `llm/traced_client.rs`.

**What must survive the rewrite** — each of these exists because of a recorded failure:
1. transient-status retry with backoff, jitter, whole-call budget (`rig_client.rs:50-91`, `272-301`);
2. `json_object` fallback restating the schema and the literal word "JSON" — DeepSeek 400s otherwise (`rig_client.rs:139-153`, `247-262`);
3. provider/model metadata, token usage, `llm_pricing` cost (`:169-190`);
4. custom `chat_completions_url` (`:93-101`).

- [ ] **Step 1: Prove the tests pin behaviour, not implementation.** Run the eight existing tests in `rig_client.rs:351-516`. Any test that asserts on `reqwest` types rather than observable behaviour is rewritten *before* the implementation changes.
- [ ] **Step 2: Spike.** Determine whether `rig_core` 0.40.0's provider client supports (4) custom completion URL and (2) a per-request response-format override. Record the answer in the spec's §20 open item 1. **If it cannot**, the adapter keeps a `reqwest` transport for exactly those cases and `V-L12` is amended in this plan with the surviving line range and a one-line reason — it is not silently left in.
- [ ] **Step 3:** Implement `LlmProvider` over `rig_core`. All eight retained tests pass unchanged.
- [ ] **Step 4:** Delete `crates/chat/examples/phase0_rig_poc.rs` — its purpose (proving the `Tool` trait round-trips) is now discharged by production code.

**V-L12 amendment (recorded 2026-08-04):** `provider.rs` (the production Rig boundary) has
zero `reqwest` references — the spike was unnecessary because Rig's provider client covers
both the custom completion URL and the per-request `json_object` fallback. The 2 remaining
`reqwest` hits are `planner_client.rs:25,43`, which backs the **legacy** semantic pipeline
(`understanding/classifier/`, `assistant/llm/semantic/`) — the same modules Task 7.1 deletes
wholesale under `V-L9`. Migrating this file to Rig now would be rewriting code with a fixed
deletion date; `planner_client.rs` is deleted alongside its callers in Task 7.1, not migrated.

**Gate 1.2:** `V-L13` ∅, `V-L17` ∅, `V-TREE-RIG` non-empty, `cargo test -p chat llm::`, `V-BUILD`, `V-LINT`.

### Task 1.3: Understanding agent — the structured-agent boundary (spec §8.2)

Issue 012's Phase 1 is "the real Rig provider **and structured-agent** boundary". Task 1.2
is only the provider half. This task is the other half, and it lands here rather than in
Phase 3 because it has no dependency on the workflow contract.

**Files:** create `crates/chat/src/assistant/llm/agent/mod.rs`, `agent/understanding.rs`;
modify `assistant/understanding/gateway/`.

**Interfaces:** produces schema-validated intent / entities / request shape via a Rig
structured agent. Vocabulary restricted to catalog-visible terms. Output is **advisory**
until deterministically grounded — it never selects a capability by itself.

- [ ] **Step 1:** Failing test with a fake model: an extraction naming a term absent from the catalog is rejected at the schema boundary, not passed downstream.
- [ ] **Step 2:** Implement over `rig_core`'s structured agent. Reuse the existing gateway JSON schema (`understanding/gateway/schema.rs`) so the contract does not fork.
- [ ] **Step 3:** Test that `max_turns` is set and a model that loops terminates rather than running unbounded.

**Gate 1.3:** `cargo test -p chat llm::agent`; extraction contract unchanged for callers.

**Gate 1 (phase):** Gates 1.1 + 1.2 + 1.3. Request behaviour unchanged — `cargo test -p chat` selections for `runtime::` and `execution::` still green.

---

# Phase 2 — Resource and acquisition graph

Catalog-only. No runtime consumes the new metadata yet, so this phase cannot regress a request.

### Task 2.1: Extend `ParameterPolicy` (spec §6)

**Files:** `crates/chat/src/knowledge/catalog/parameter_policy.rs`, `catalog/validator.rs`, `catalog/loader.rs`.

**Interfaces:** three new fields — `user_required: bool` (default `false`),
`resolution: Vec<ResolutionStrategy>` (default `[]`), `probe: Option<ProbeRef>`.
Existing fields keep their names and meaning (spec §6 key-mapping table).

- [ ] **Step 1:** Write the four failing validator tests first: (a) `user_required: true` without a `ParameterInputKnowledge` entry is a load error; (b) `AuthorizedDataProbe` without `probe` is a load error; (c) `probe` naming a dataset/shape that does not exist, or whose output slot type mismatches, is a load error; (d) `office_ids` listing `VerifiedUserText` or `Clarify` is a load error.
- [ ] **Step 2:** Add a fifth: a parameter with `resolution: []`, no default, and `user_required: false` is a load error — it can never be filled.
- [ ] **Step 3:** Implement. Every existing capability YAML must still load (they omit all three new fields).

**Gate 2.1:** five rejection tests green; `cargo test -p chat knowledge::catalog`; all 41 capabilities load.

### Task 2.2: Capability kinds

- [ ] Add `kind: terminal | resolver | probe | composite` to `CapabilityKnowledge` (default `terminal`). `composite` marks a capability whose answer is assembled from more than one approved plan (the A4 comparison shape) — it is the catalog's permission for a multi-node workflow, not a second SQL surface.
- [ ] Validator: a `resolver` capability must reference a dataset shape with `role: resolver`; a `composite` capability must declare ≥2 member capability IDs, all `approved_mvp`.

### Task 2.3: Dataset resolver metadata (spec §7.1, §7.2)

**Files:** `crates/chat/src/knowledge/dataset/model.rs`, `validate.rs`, `compose.rs`.

- [ ] **Step 1:** Add `entity` block (`kind`, `id_field`, `label_fields`, `label_fallback`) and per-shape `role`, `expected_cardinality`, `row_cap`, `produces`.
- [ ] **Step 2:** Validator: `id_field` must be an output field of type bigint with sensitivity `public_business`; every `label_fields` entry must exist; a `resolver` shape must declare `produces`.
- [ ] **Step 3:** Add `FilterOperator::In`. Composer expands to `= ANY($n)` with a bound array — **write the failing test first** asserting the generated SQL contains no interpolated value. Validator caps array length at the shape's `row_cap`.

**Gate 2.3:** `cargo test -p chat knowledge::dataset`; existing four datasets unchanged and still compose byte-identically (`dataset_equivalence` test).

### Task 2.4: Author the seven datasets (spec §7.3)

One task per dataset, each with its own SQL source under `queries/datasets/`.

- [ ] `client.identity` + `identity_candidates`
- [ ] `organization.offices` + `office_candidates`
- [ ] `savings.accounts` extended with `accounts_by_client`
- [ ] `savings.transactions` + `activity_rows`
- [ ] `savings.products` + `products_by_client`
- [ ] `savings.charge_definitions` + `charge_type_candidates`
- [ ] `client.portfolio_counts` + `counts_by_client` (grouped — this is the anti-N+1 surface for A2)

Each must: keep source-level authorized office predicates; declare per-field sensitivity;
mark `core` fields; pass `validate_runtime` (prepare + output-column contract) via
`POST /catalog/validate`; **and run its own example end to end against the live Fineract
database before the task is checked off.**

**Do not add loan or audit datasets.** Those domains stay deferred
(`knowledge/data-scope/areas/deferred.yaml`).

**Gate 2 (phase):** all seven datasets pass `POST /catalog/validate`; each ran live;
`cargo test -p chat knowledge::`; `V-BUILD`, `V-LINT`.

---

# Phase 3 — Workflow compiler, verifier, graph

The core. Nothing routes here yet — this phase builds and tests the runtime in isolation.

### Task 3.1: Contract types (spec §4)

**Files:** create `crates/chat/src/assistant/workflow/{mod.rs,contract.rs}`.

- [ ] `ExecutionWorkflow`, `WorkflowNode`, `NodeKind` (6 variants), `WorkflowEdge`, `EdgeCondition`, `NodeInput`, `BindingSource` (8 variants), `NodeOutputSlot`, `WorkflowBudgets`, `FailPolicy`, `OutputContract`, `NodeId`.
- [ ] `#[serde(deny_unknown_fields)]` on every struct. Round-trip test: serialize → deserialize → equal; and a stale-field JSON fails to parse rather than defaulting.
- [ ] `NodeId` newtype validates `^[a-z][a-z0-9_]{0,47}$` at construction.

### Task 3.2: Graph wrapper (spec D3)

**Files:** create `workflow/graph.rs`.

- [ ] Build a `petgraph::DiGraph` from nodes+edges.
- [ ] Expose `is_cyclic()` (`petgraph::algo::is_cyclic_directed`), `topological_order()` (`toposort`), `runnable(completed: &HashSet<NodeId>)`, `reachable_from(entry)`.
- [ ] Test: a hand-built cyclic workflow returns `true`; the diamond workflow topo-sorts correctly; `runnable` returns both arms of a fan-out.

**This is the D3 gate.** If the compiler ends up not reading the graph, the petgraph
dependency is deleted in Phase 7 instead of kept as a marker.

### Task 3.3: Compiler (spec §4.2, §4.3, §6)

**Files:** create `workflow/compile.rs`.

- [ ] Resolve proposal IDs against the catalog; attach `query_id`, budgets, sensitivity.
- [ ] Apply the acquisition order (spec §6, nine steps) per unbound parameter, emitting `ResolveEntity` / `ClarificationInterrupt` nodes as needed. **Test one case per step**, in order, asserting the earlier step wins.
- [ ] Expand `iterate_over` into sibling nodes; reject when a `grouped_by` shape covers the same slot, with the grouped shape ID in the error.
- [ ] Choose the array-binding path over clarification whenever the downstream shape has an `In` filter on the ID (spec §9.4).

### Task 3.4: Verifier (spec §5)

**Files:** create `workflow/verify.rs`.

- [ ] `VerifyError` as a closed enum, one variant per V1–V12, each with a sanitized client message and a distinct audit reason code.
- [ ] **One rejecting test per rule.** Each test asserts *zero queries executed*, by counting repository calls — not by reading the error string.
- [ ] V4 in particular: a proposal whose filter value is a SQL identifier is rejected, and a proposal that binds the same text as a *parameter* is accepted.

### Task 3.5: Ambiguity gate (spec §6A)

**Files:** `crates/chat/src/assistant/retrieval/reranker.rs`, `workflow/compile.rs`.

- [ ] **Step 1:** Reranker returns a ranked scored candidate list. Delete the `MIN_SELECT_CONFIDENCE` coercion (`reranker.rs:208-215`) and the malformed-output clarify fallback (`:196-201`).
- [ ] **Step 2:** Malformed structured output retries per the provider policy, then terminates `FailedOperational`. Test: a fake model returning garbage produces a failed job, **not** a clarification listing capability IDs.
- [ ] **Step 3:** Compiler owns clarify/no-clarify via the three-part gate (§6A.3). Test each part: (a) exactly one compatible capability overrides low confidence and records the override; (b) alternatives that deterministic facts separate do not clarify; (c) alternatives a probe can separate produce a probe node, not a question.
- [ ] **Step 4:** Test that no policy decision is reachable from any agent tool — assert the tool registry contains no policy tool.

### Task 3.6: Metadata tools (spec §8.3)

**Files:** create `crates/chat/src/assistant/llm/tool/{mod.rs,metadata.rs}`.

Six tools, none of which touch data: `search_catalog`, `inspect_capability`,
`inspect_dataset`, `find_entity_resolver`, `find_compatible_next_steps`,
`propose_workflow`.

- [ ] **Step 1:** Every tool description is **generated from approved catalog metadata**, never hand-written prose. Test: a capability whose YAML display name changes produces a changed tool description with no Rust edit. This is the guard against the `reranker-judges-prose-not-sql` failure mode — hand-authored descriptions drift from the SQL and make working queries unreachable.
- [ ] **Step 2:** `find_compatible_next_steps` returns only typed `produces`→`NodeInput` matches (spec §7.1). Test: it never returns a pair whose types differ.
- [ ] **Step 3:** Assert the registry contains **no** raw-SQL tool and **no** policy tool (spec §6A.2 — policy is never asked, only enforced).

### Task 3.7: Planning agent and turn budget (spec §8.2, §8.4)

**Files:** create `assistant/llm/agent/planning.rs`.

- [ ] **Step 1:** Agent consumes the six metadata tools and returns a `WorkflowProposal` referencing catalog IDs only. Failing test first: a proposal containing a SQL string fails schema validation before it reaches the compiler.
- [ ] **Step 2:** `max_turns` enforced. Test with a fake model that never converges: the run terminates at the turn cap, and the job fails rather than executing a partial proposal.
- [ ] **Step 3:** Tool-loop contract test with a fake model — the fake returns tool calls and a final proposal. Per `tests-must-not-forge-inter-stage-values`, the fake substitutes the **model only**; the proposal still travels through the real compiler and verifier.
- [ ] **Step 4:** `dynamic_tools` / `dynamic_context` stay **off** (spec §8.3). Turning either on requires a measured `retrieval_eval` improvement recorded here first.

**Gate 3 (phase):** V1–V12 each have a rejecting test with a zero-query assertion;
`V-L21` ∅; petgraph read paths exercised; tool registry contains no SQL or policy tool;
adversarial proposals (A7 subset) execute zero queries;
`cargo test -p chat workflow:: llm::`; `V-BUILD`, `V-LINT`.

---

# Phase 4 — Durable sequential and conditional runner

### Task 4.1: Migration (spec §9.1)

**Files:** create `migrations/2026xxxx_workflow_runtime.sql`.

- [ ] `chat_jobs` gains `workflow_id`, `workflow_contract_version`, `workflow_revision`, `current_node_id`.
- [ ] Create `chat_workflow_node_runs` with the `UNIQUE (job_id, workflow_id, node_id, attempt)` constraint — this is what makes replay detectable rather than merely discouraged.
- [ ] Replace the `current_step` / `resume_from_step` CHECK constraints (19 fixed names, `20260617130000_create_chat_tables.sql:45-46`) with the workflow step set. **Backfill historical rows**; do not drop them.
- [ ] `chat_job_checkpoints.checkpoint_type` gains `node_started`, `node_completed`, `workflow_paused`, `workflow_resumed`.
- [ ] Migration test asserting a pre-migration job row still reads back.

### Task 4.2: Runner (spec §9)

**Files:** create `workflow/run.rs`, `workflow/state.rs`, `workflow/node/*.rs`.

- [ ] Execute runnable nodes in dependency order; persist a `chat_workflow_node_runs` row per attempt.
- [ ] **Per-node policy check inside `execute_node`** (SI-6). Test: a workflow whose second node is disallowed executes the first and blocks the second — a preflight-only implementation fails this test.
- [ ] `ExecuteQuery` calls the existing `chat::execution::repository` path unchanged.
- [ ] `ResolveEntity` binds probe output through typed slots; `ExactSensitiveInput` values are never written to `output_json` (SI-7 — assert by scanning the persisted JSON).
- [ ] `CardinalityBranch` selects the arm from actual row count.

### Task 4.3: Pause and resume (spec §9.3)

- [ ] `ClarificationInterrupt` persists the payload and terminates `WaitingForUserInput`.
- [ ] `POST /chat/jobs/{id}/responses` matches on **five** values: job, workflow, node, clarification ID, revision. One test per mismatch, each asserting rejection rather than reroute.
- [ ] Resume marks the interrupt node completed and continues at `resume`. Completed nodes are not re-run; an `ExecuteOnce` node reached twice is a hard error.
- [ ] **Restart test:** kill and restart the process between probe and resume; assert the probe row is still `completed` and no second probe row appears.

### Task 4.4: Clarification contract additive fields (spec §11, D4)

- [ ] Add `workflow_id`, `node_id`, `resume_node_id`, `entity_kind` as `Option` to `ClarificationPayload` / `ClarificationView`.
- [ ] `SelectEntity` options carry a stable ID; the runtime binds the selection as a **parameter value**. Delete `runtime/mod.rs:483` (`selected_capability = Some(option_id)`).
- [ ] **FE snapshot test**: serialize a clarification with all new fields `None` and assert the JSON is byte-identical to the pre-migration snapshot. This is the D4 guarantee.

### Task 4.5: Guarded data tools (spec §8.3, SI-4)

Lands here, not Phase 3, because the guard must validate **workflow-step membership** — it
needs the runner's node context to exist.

**Files:** create `assistant/llm/tool/data.rs`.

Two tools: `execute_approved_probe`, `execute_approved_capability`.

- [ ] **Step 1:** Implement the validation chain in this exact order, before the repository is called: workflow-step membership → capability → parameter provenance → policy → PII → office scope → timeout → row cap → query budget.
- [ ] **Step 2:** **One rejecting test per link, each asserting zero repository calls.** A test that only asserts the error message would pass against a guard that validates after executing.
- [ ] **Step 3:** Test that a tool call naming a node that is not runnable in the current workflow state is rejected — this is what stops a model from replaying a completed probe or reaching a node the graph never scheduled.
- [ ] **Step 4:** Tool output enters the model as untrusted content (SI-11). Test: a probe row containing prompt-injection text is passed through as a typed field and does not alter the next tool call.

### Task 4.6: Audit lineage and progress stages (SI-13)

Every execution and branch needs durable lineage, and the SSE stream needs to describe a
multi-node run. Neither is expressible with the current fixed stage list.

**Files:** `crates/chat/src/audit/`, `crates/chat/src/job/progress/stage.rs`, `job/service/events.rs`.

- [ ] **Step 1:** `Stage` is currently `Routing | Retrieval | Reranking | Policy | Execution | Formatting` (`progress/stage.rs:10-17`) — the legacy pipeline's shape. Add workflow stages `Understanding | Planning | Verifying | Node { id } | Composing`.

      **This is the one surface D4 does not protect.** D4 covers the clarification and
      structured-response contracts; SSE progress stages are a third surface, and
      *replacing* the six existing stage strings would break any frontend that keys on
      them. So: retain the existing six as emitted values wherever a workflow phase maps
      onto one (`Understanding` also emits `routing`, `Composing` also emits
      `formatting`), and add the node-level stages alongside. Confirm with the FE owner
      before removing any of the six — if none are consumed, drop them in Phase 7 and
      record that in the deletion diff. Do not assume.
- [ ] **Step 2:** Emit an audit event per node start, node completion, branch decision (with the cardinality that selected the arm), clarification pause, and resume. Test: a three-node workflow with one branch produces a lineage from which the executed path is reconstructable.
- [ ] **Step 3:** Assert no audit payload contains SQL, prompts, or sensitive selector values (SI-7, SI-12).

**Gate 4 (phase):** `V-L3` ∅; restart-resume test green against real Postgres;
FE snapshot byte-identical; per-link data-tool rejection tests green with zero repository
calls; node lineage reconstructable; `cargo test -p chat workflow:: job:: audit::`;
`V-BUILD`, `V-LINT`.

---

# Phase 5 — Composite and bounded iteration

- [ ] Bounded parallel fan-out honouring `max_parallel_queries`.
- [ ] `FailPolicy::FailFast` cancels in-flight siblings via a shared `CancellationToken`; no partial result is composed. Test: one node fails, siblings observe cancellation, response is a failure not a partial.
- [ ] `ContinueLabelled` legal only when `output_contract.allows_partial`; V7 rejects otherwise.
- [ ] Shared budget ledger on the job row: queries, rows, elapsed ms decremented per node; exceeding any terminates the workflow.
- [ ] N+1 rejection test: a proposal iterating a per-client count where `counts_by_client` exists fails compilation naming that shape.

**Gate 5:** cancellation test; N+1 rejection test; budget-exhaustion test; `V-BUILD`, `V-LINT`.

---

# Phase 6 — Composition and grounded response

- [x] `ComposeResult` modes `single` / `comparison` / `grouped`, deterministic, no LLM. Wired into `WorkflowRunner::resolve_execution` as an inline arm (matching `CardinalityBranch`/`Complete`), not the injected executor trait.
- [x] `comparison` compiler check: differing scope or temporal facts is a **compile error**, not a runtime warning. `CompileError::ComparisonFactsDiverge` in `compile.rs`, checked via `check_comparison_facts` on every compiled workflow.
- [x] Sensitivity enforcement at composition; every dropped field recorded in audit. `node::compose::compose` drops fields above the principal's visible `Sensitivity` and returns the names; `WorkflowStateRepository::record_sensitivity_drop` writes a `workflow_field_dropped` event with field names only, never values.
- [x] Optional Rig response agent consuming policy-filtered structured fields only. `llm::agent::response::ResponseAgent` mirrors `PlanningAgent`'s `llm::structured` pattern over `LlmPurpose::ResponseBuild`. Test: the agent's input contains no hidden identifier, no raw SQL, no unfiltered row.
- [x] Additive response fields `workflow.{id,node_id,steps_executed,partial}`. `AssistantResponse.workflow: Option<WorkflowResponseMeta>` with `skip_serializing_if`.
- [x] FE snapshot test for the structured response, same discipline as Task 4.4.

**Gate 6:** FE response snapshot unchanged with workflow fields absent; `V-BUILD`, `V-LINT`. All green.

---

# Phase 7 — Legacy deletion

**Separate commits. Reviewed as its own diff.** Nothing in this phase adds behaviour.

Order matters: delete callers before callees, so the compiler finds every orphan.

### Task 7.1: Delete the alternate runtime paths

- [ ] `AI_REPORT_GATEWAY_PIPELINE` branch, `run_via_gateway_pipeline`, `route_via_gateway_pipeline` (`runtime/mod.rs:154-319`, `:592-606`). → `V-L7`
- [ ] `CanonicalGatewayMode`, `CHAT_CANONICAL_GATEWAY_MODE`, `job/service/shadow.rs` (`core/src/config/mod.rs:136-151`, `:305`). → `V-L8`
- [ ] `deterministic_simple_response` (`runtime/mod.rs:572-587`). Greeting/help become workflow terminals. → `V-L6`
- [ ] `AssistantGraphRuntime::run` (`runtime/mod.rs:324-361`). → `V-L10`
- [ ] Classifier + semantic router (`understanding/classifier/`, `llm/router.rs`, `runtime/semantic.rs`). → `V-L9`

### Task 7.2: Delete the atomic planner and its clarification gate

- [ ] `assistant/execution/plan/` entirely. → `V-L1`
- [ ] `defaultless_missing_fields` and the pre-query gate (`runtime/execution.rs:41-76`). → `V-L2`
- [ ] `client_entity_options` and the two-capability match (`runtime/execution.rs:296-300`, `:399-431`). → `V-L4`
- [ ] All remaining `capability_id.as_str()` behaviour switches. → `V-L5`
- [ ] `knowledge/dataset/legacy.rs` and duplicate parameter planning. → `V-L11`

### Task 7.3: Delete superseded state and presentation code

- [ ] `AssistantGraphTopology` / `GraphState` / `GraphTransition` — **conditional on spec §20 open item 3.** Decide first whether `GraphState` survives as job-level lifecycle vocabulary; if it does, amend `V-L16` in this plan with the surviving symbols and the reason. Do not leave the decision implicit. → `V-L16`
- [ ] Formatter special cases in `assistant/presentation/`. → `V-L20`

### Task 7.4: Tests and docs

- [ ] Delete or rewrite tests asserting legacy paths, fixed catalog counts, or obsolete response strings (`runtime/tests.rs` 1742 lines, `execution/tool/tests.rs` 888, `plan/tests.rs`). A rewritten test asserts a behaviour or security contract, never an implementation shape.
- [ ] Update `AGENTS.md`, `CLAUDE.md`, `docs/architecture/ai-reporting-design/`, `docs/implementation-steps.md` to describe the implemented runtime. → `V-L19`

**Gate 7 — the deletion gate. All must hold:**

- [ ] Every `V-L*` command ∅ (or matches an amendment recorded in this plan with a reason).
- [ ] `V-FLAGS`: no production config key selects a runtime.
- [ ] `V-TREE-SWIFTIDE` ∅; `V-TREE-RIG` non-empty; if petgraph is unused, it is removed rather than retained.
- [ ] `git log --oneline` shows Phase 7 commits containing **no additions** to `crates/*/src` other than deletions and their mechanical fallout.
- [ ] Spec §13.1: exactly one router, planner, acquisition model, clarification path, runner, executor path, response authority. Assert by grep, not by reading.

---

# Phase 8 — Acceptance and rollout

### Task 8.1: Scenarios (spec §15)

One test per scenario, each asserting the **node trace**, not the final text.

- [ ] A1 savings activity — all five cardinality branches; no `latest_transaction_amount` required; no raw account number in any option.
- [ ] A2 office + portfolio — grouped counts with `query_cost == 1`; loan coverage returns honest unsupported and no savings substitution.
- [ ] A3 charge type — probe before clarification.
- [ ] A4 composite comparison — identical facts, both policy-passed pre-execution, no unlabelled partial.
- [ ] A5 sensitive account lookup — unauthorized and nonexistent produce indistinguishable responses.
- [ ] A6 recovery — resume after restart, probe not re-run.
- [ ] A7 adversarial planning — seven malformed proposals, each executing **zero** queries.

### Task 8.2: Security invariant sweep (spec §12)

One test per invariant. SI-1…SI-14 are the contract this migration must not have quietly
traded away for capability; a passing acceptance scenario does not prove any of them.

- [ ] SI-1 bearer + admin role on every chat route, including the new workflow endpoints.
- [ ] SI-2 office scope bound **inside** SQL — assert on the prepared statement, not the result rows.
- [ ] SI-3 a user filter can only narrow: a request naming an office outside scope returns that office's rows *intersected*, never widened.
- [ ] SI-4 no raw-SQL tool in the registry (also Task 3.6 Step 3).
- [ ] SI-5 an unapproved capability ID never executes (V2).
- [ ] SI-6 per-node policy — the second-node-blocked test from Task 4.2.
- [ ] SI-7 sensitive selectors absent from every persisted JSON column.
- [ ] SI-8 probe labels carry no unauthorized PII; `label_fallback` used when `can_view_pii` is false.
- [ ] SI-9 `reject_client_rows` still refuses Fineract row/export paths.
- [ ] SI-10 each of the six budgets terminates a run when exceeded.
- [ ] SI-11 injection text in tool output (Task 4.5 Step 4).
- [ ] SI-12 client errors contain no SQL, prompt, stack trace, credential or hidden ID — assert over every `VerifyError` variant, not a sample.
- [ ] SI-13 lineage reconstructable (Task 4.6 Step 2).
- [ ] SI-14 no unlabelled partial result (V7 + Phase 5 cancellation test).

### Task 8.3: Full sweep and metrics

- [ ] `cargo test --workspace` — the one full run this plan authorizes.
- [ ] Live scenario matrix: `RUN_LIVE_SCENARIO_MATRIX=1 cargo test -p chat --test scenario_matrix`.
- [ ] Re-measure every spec §17 metric against `012-legacy-baseline.md`.

**Gate 8 — resolution gate:**

- [ ] A1–A7 pass against a production-like database.
- [ ] All 14 security invariants have a passing test (Task 8.2).
- [ ] Gate 7 still holds (re-run every `V-L*`).
- [ ] Wrong-answer rate and unauthorized-data rate have **not increased**. This gate fails on any increase in either, regardless of how far the clarification rate fell.
- [ ] Spec §19 traceability table: all 13 done-conditions discharged.
- [ ] Issue 012 moved to `docs/issues/resolved/`.

---

## Issue coverage map

Issue 012's requirement sections, each traced to the task that discharges it. A section
with no task here is a planning gap — this table is how "is it covered?" gets answered
mechanically instead of from memory.

| Issue section | Tasks |
| --- | --- |
| Required execution model — atomic / sequential / conditional | 3.1, 3.3, 4.2 |
| — parallel / composite | 2.2 (`composite` kind), 5, 6 |
| — bounded iterative | 3.3 (compile-time expansion), 5 |
| Required workflow contracts | 3.1; verifier rejections 3.4 |
| Required parameter acquisition model | 2.1, 3.3 |
| Required dataset migration (9 items) | 2.3 (items 3–8), 2.4 (items 1–2), 2.1 + 2.4 (item 9) |
| Required Rig integration — understanding agent | 1.3 |
| — planning agent + 6 metadata tools | 3.6, 3.7 |
| — guarded data tools | 4.5 |
| — turn and tool budgets | 3.7 Step 2, 5 (budget ledger) |
| — response agent | 6 |
| Required Petgraph integration | 3.2; read paths asserted at Gate 3 |
| Durable workflow and clarification | 4.1, 4.2, 4.3, 4.4 |
| Ambiguity classes / data-informed clarification | 3.5 |
| Security invariants (14) | 8.2, plus per-phase tests 1.1, 4.2, 4.5, 4.6 |
| Mandatory legacy deletion (18 items) | 7.1–7.4, gated by `V-L1`…`V-L21` |
| Proving 100% removal (7 bullets) | Gate 7 |
| Required migration phases 0–8 | Phases 0–8 |
| Required acceptance scenarios (7) | 8.1 |
| Required tests and evidence (15 bullets) | 3.1–3.7 (pure), 4.2–4.6 (integration), 8.1 (live), Gate 7 (deletion), Gate 1 (dependency audit) |
| Operational success metrics (13) | 0.1 baseline, 8.3 re-measure |
| Definition of done (13) | spec §19 traceability table; checked at Gate 8 |

Deliberately **not** planned, with reason:

| Issue item | Why |
| --- | --- |
| Inventory item 16 — deprecated clarification API projections | Cannot accrue under D4 (additive-only). If one appears, D4 was violated. |
| Loan / audit datasets | Domains remain deferred; A2 returns an honest unsupported instead. |
| Migration window for FE contracts | D4 removes the need for one. |

## Risk register

| Risk | Mitigation |
| --- | --- |
| `rig_core` 0.40.0 cannot express the custom URL or the `json_object` fallback | Task 1.2 Step 2 spikes it *before* the rewrite; the adapter keeps those two cases with an amended `V-L12` and a stated reason |
| Phase 3–6 land while Phase 7 is deferred, leaving two runtimes in production | Phases 3–6 are not routed to production traffic. The router switch happens **in** Phase 7, in the same commit range as the deletions |
| `runtime/tests.rs` (1742 lines) makes Phase 7 look impossibly large | Task 7.4 is budgeted as its own multi-commit task; tests are rewritten to behaviour contracts as each source deletion lands, not all at the end |
| Petgraph turns out unused after Task 3.2 | D3 already commits to deleting it in that case. The dependency is not kept as a marker |
| SSE progress stages break the frontend | D4 protects the clarification and response contracts but **not** the stage strings. Task 4.6 Step 1 keeps the existing six emitted and adds node stages alongside; removal needs FE confirmation, not assumption |
| Guarded data tools validate after executing | Task 4.5 Step 2 asserts **zero repository calls** per rejected link. An error-message assertion would pass against a broken guard |
| Live database unavailable during Phase 2 | Dataset tasks cannot be checked off without a live example run. Blocked datasets stay unchecked rather than being marked done from a passing `PREPARE` |

## Out of scope

Loan / audit / tax / accounting datasets. Frontend changes (D4 makes them unnecessary).
New orchestration dependencies (D6). Splitting reviewed grouped SQL into multiple queries.
