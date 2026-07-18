# Rust Module Structure Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Track every step with the checkbox (`- [ ]`) syntax below.

**Goal:** Reorganize the existing Rust code into the approved hybrid feature-first, layer-second module structure without changing behavior, public signatures, or external contracts.

**Architecture:** Keep exactly the `app`, `core`, and `chat` crates. Perform mechanical, independently buildable moves behind temporary compatibility re-exports, then decompose only the identified mixed-responsibility files. Preserve `route → service → repository → database`; repositories remain the only SQL boundary.

**Tech Stack:** Rust edition 2024, Cargo workspace, axum, sqlx, PostgreSQL, Redis, existing workspace dependencies only.

**Authoritative design:** `docs/superpowers/specs/2026-07-18-rust-project-structure-design.md`.

## Non-negotiable constraints

- Do not add, rename, or merge crates; keep exactly `crates/app`, `crates/core`, and `crates/chat`.
- Do not add dependencies, migrations, knowledge YAML, approved SQL, or empty module scaffolding.
- Preserve every public function, method, trait, DTO, serialized field, HTTP route, JSON envelope, SSE event, and user-visible response.
- Preserve bearer-session JWT chat authentication, `role == "admin"`, and the `project_admin_principal` behavior. `X-API-Key` remains an optional voluntary office opt-down and never becomes chat authentication.
- Preserve approved-catalog SQL execution and bind authorized `office_ids` inside SQL. Never post-filter office scope in Rust and never move `sqlx` into handlers or services.
- Preserve PII policy and sanitized errors; do not expose prompts, SQL errors, stack details, or internal traces.
- Preserve PostgreSQL as durable truth for sessions, messages, jobs, checkpoints, and events. Redis remains live SSE coordination only.
- Preserve clarification on the same job through `POST /chat/jobs/{job_id}/responses`.
- Preserve both distinct `RetrievalPlan` types: legacy planning at `chat::planner::RetrievalPlan` and assistant evidence planning at `assistant::evidence::RetrievalPlan`. Do not merge or substitute them because their names match.
- Preserve English-only product behavior.
- Treat approximately 400 production LOC as a mandatory decomposition-review trigger, not an automatic split threshold. Cohesion decides the resulting boundaries at every size.
- Use `pub(super)` or `pub(crate)` only for demonstrated callers. Unrestricted `pub` remains limited to intentional crate APIs.
- Move code and tests mechanically; do not rewrite logic while changing ownership.
- Do not include or perform commit steps.

## Validation convention

After every task, run the listed focused checks from the repository root. A task is complete only when the expected result is observed and `git diff --check` reports no whitespace errors. If a move exposes a pre-existing failing database-backed test, record the exact environment blocker; do not weaken or ignore the test.

---

## Task 1 — Record a green behavioral baseline

**Files:**
- Read: `Cargo.toml`
- Read: `crates/chat/tests/chat_jobs.rs`
- Read: `crates/chat/tests/auth_api_keys.rs`
- Read: `crates/chat/tests/savings_answer_quality.rs`
- Read: `crates/chat/tests/chat_full_flow.rs`
- Read: `crates/chat/src/assistant/{runtime/mod.rs,extraction.rs,tool.rs,canonical_state.rs}`
- Create no files.

**Steps:**

- [x] **1.1 Verify formatting and compilation before moving anything.**
  ```bash
  cargo fmt --check
  cargo check
  ```
  Expected: both commands exit `0`; the workspace contains only `app`, `core`, and `chat` crates.

- [x] **1.2 Run the high-value contract tests.**
  ```bash
  cargo test -p chat missing_date_range_triggers_clarification_and_continues_same_job
  cargo test -p chat x_api_key_variants_do_not_change_bearer_chat_authorization
  cargo test -p chat wildcard_key_option_id_response_executes_same_job
  cargo test -p chat savings_answer_respects_narrow_office_scope
  cargo test -p chat savings_clarification_keeps_selected_capability_for_parameter_only_reply
  ```
  Expected: each named test passes, proving same-job clarification, bearer-admin authority, office scope, and answer behavior before the refactor.

- [x] **1.3 Run focused unit tests for the priority files.**
  ```bash
  cargo test -p chat temporal_reuses_the_same_job_reference_after_clarification
  cargo test -p chat canonical_snapshot_rejects_malformed_parameters
  cargo test -p chat precedence_clear_and_list_algebra
  cargo test -p chat semantic_router_unavailable_fails_closed
  ```
  Expected: each named test passes without source changes.

- [x] **1.4 Save the exact command output or blocker in the implementation notes.** Do not proceed with an unexplained red baseline.

---

## Task 2 — Group `core` modules with no behavior change

**Files:**
- Move: `crates/core/src/config.rs` → `crates/core/src/config/mod.rs`
- Move: `crates/core/src/db.rs` → `crates/core/src/database/mod.rs`
- Move: `crates/core/src/telemetry.rs` → `crates/core/src/telemetry/mod.rs`
- Modify: `crates/core/src/lib.rs`
- Modify callers returned by the searches below only when required.

**Steps:**

- [x] **2.1 Search exact callers before moving files.**
  ```bash
  git grep -n -E 'core::(config|db|telemetry)|crate::(config|db|telemetry)|\bdb::' -- ':(glob)crates/**/*.rs'
  ```
  Expected: a finite caller inventory; no generated or non-Rust files need edits.

- [x] **2.2 Create only the three destination directories and mechanically move file contents.** Do not split types or change signatures.

- [x] **2.3 Declare `pub mod database;` in `crates/core/src/lib.rs` and add the temporary compatibility shim `pub use database as db;`.** Keep existing `config` and `telemetry` public paths unchanged through their new `mod.rs` files.

- [x] **2.4 Update internal imports to prefer `core::database`; leave external/current callers compiling through `core::db` until Task 9.**

- [x] **2.5 Validate the no-op grouping.**
  ```bash
  cargo fmt --check
  cargo check -p core
  cargo check
  git diff --check
  ```
  Expected: all commands exit `0`; the only semantic surface addition is the temporary `database` name and the old `db` path still compiles.

---

## Task 3 — Extract conversation ownership

**Files:**
- Move: `crates/chat/src/chat/model/session.rs` → `crates/chat/src/conversation/model/session.rs`
- Move: `crates/chat/src/chat/model/message.rs` → `crates/chat/src/conversation/model/message.rs`
- Move: `crates/chat/src/chat/repository/session.rs` → `crates/chat/src/conversation/repository/session.rs`
- Move: `crates/chat/src/chat/repository/message.rs` → `crates/chat/src/conversation/repository/message.rs`
- Move: `crates/chat/src/chat/service/session.rs` → `crates/chat/src/conversation/service/session.rs`
- Move: `crates/chat/src/chat/service/message.rs` → `crates/chat/src/conversation/service/message.rs`
- Create: `crates/chat/src/conversation/{mod.rs,model/mod.rs,repository/mod.rs,service/mod.rs}`
- Modify: `crates/chat/src/{lib.rs,chat/model/mod.rs,chat/repository/mod.rs,chat/service/mod.rs}`
- Modify direct callers in `crates/chat/src/api/` and `crates/chat/src/chat/` only as required.

**Steps:**

- [x] **3.1 Inventory callers and public names.**
  ```bash
  git grep -n -E 'chat::(model|repository|service)::(session|message)|Chat(Session|Message)(Repository|Service)?' -- ':(glob)crates/chat/**/*.rs'
  ```
  Expected: all current session/message callers are identified before imports move.

- [x] **3.2 Add the thin `conversation` façades and move session/message files without editing their implementations.** Do not move job types in this task.

- [x] **3.3 Re-export the existing public names from the old `chat::{model,repository,service}` paths.** The old-path shims must point to the new definitions rather than duplicate types.

- [x] **3.4 Update API and internal callers to canonical `crate::conversation` paths where doing so is mechanical.** Preserve repository-only SQL and all method signatures.

- [x] **3.5 Validate conversation behavior.**
  ```bash
  cargo fmt --check
  cargo check -p chat
  cargo test -p chat --test chat_sessions
  cargo test -p chat --test assistant_context_window
  git diff --check
  ```
  Expected: all commands exit `0`; session/message HTTP and persistence contracts are unchanged, and old imports remain available.

---

## Task 4 — Extract durable job ownership

**Files:**
- Move: `crates/chat/src/chat/model/job.rs` → `crates/chat/src/job/model.rs`
- Move: `crates/chat/src/chat/repository/job.rs` → `crates/chat/src/job/repository/mod.rs`
- Move intact first: `crates/chat/src/chat/service/job.rs` → `crates/chat/src/job/service/mod.rs`
- Create: `crates/chat/src/job/mod.rs`
- Modify: `crates/chat/src/{lib.rs,chat/model/mod.rs,chat/repository/mod.rs,chat/service/mod.rs}`
- Modify direct callers in `crates/chat/src/api/` and `crates/app/src/` only as required.

**Steps:**

- [x] **4.1 Inventory job callers and construction sites.**
  ```bash
  git grep -n -E 'chat::(model|repository|service)::job|ChatJob|JobService|JobRepository' -- ':(glob)crates/**/*.rs'
  ```
  Expected: API handlers, worker/bootstrap wiring, tests, and intra-chat callers are listed.

- [x] **4.2 Move `JobService` intact before any decomposition.** Move its model and repository mechanically, then add thin `job` façades.

- [x] **4.3 Keep temporary old-path re-exports in `chat::{model,repository,service}`.** Do not create duplicate job types and do not change checkpoints, events, retries, or transitions.

- [x] **4.4 Update canonical internal imports to `crate::job`; preserve every constructor and method signature.**

- [x] **4.5 Validate the durable lifecycle.**
  ```bash
  cargo fmt --check
  cargo check -p chat
  cargo test -p chat --test chat_jobs
  cargo test -p chat --test assistant_repositories
  cargo test -p chat missing_date_range_triggers_clarification_and_continues_same_job
  git diff --check
  ```
  Expected: all commands exit `0`; job IDs, revisions, checkpoints, events, same-job responses, and PostgreSQL/Redis roles are unchanged.

---

## Task 5 — Group assistant leaf modules

**Files:**
- Move: `crates/chat/src/assistant/intent.rs` → `crates/chat/src/assistant/understanding/intent.rs`
- Move: `crates/chat/src/assistant/extraction.rs` → `crates/chat/src/assistant/understanding/extraction.rs`
- Move: `crates/chat/src/assistant/clarification_resolver.rs` → `crates/chat/src/assistant/understanding/clarification_resolver.rs`
- Move: `crates/chat/src/assistant/context.rs` → `crates/chat/src/assistant/context/window.rs`
- Move: `crates/chat/src/assistant/context_builder.rs` → `crates/chat/src/assistant/context/builder.rs`
- Move: `crates/chat/src/assistant/clarification.rs` → `crates/chat/src/assistant/context/clarification.rs`
- Move: `crates/chat/src/assistant/canonical_state.rs` → `crates/chat/src/assistant/context/canonical_state.rs`
- Move: `crates/chat/src/assistant/canonical_state_repo.rs` → `crates/chat/src/assistant/context/canonical_state_repository.rs`
- Move: `crates/chat/src/assistant/evidence.rs` → `crates/chat/src/assistant/retrieval/evidence.rs`
- Move: `crates/chat/src/assistant/retrieval.rs` → `crates/chat/src/assistant/retrieval/engine.rs`
- Move: `crates/chat/src/assistant/reranker.rs` → `crates/chat/src/assistant/retrieval/reranker.rs`
- Move: `crates/chat/src/assistant/graph.rs` → `crates/chat/src/assistant/state/graph.rs`
- Move: `crates/chat/src/assistant/memory.rs` → `crates/chat/src/assistant/state/memory.rs`
- Move: `crates/chat/src/assistant/response.rs` → `crates/chat/src/assistant/presentation/response.rs`
- Move: `crates/chat/src/assistant/response_builder.rs` → `crates/chat/src/assistant/presentation/builder.rs`
- Move: `crates/chat/src/assistant/renderer.rs` → `crates/chat/src/assistant/presentation/renderer.rs`
- Move: `crates/chat/src/assistant/contracts.rs` → `crates/chat/src/assistant/presentation/contracts.rs`
- Move: `crates/chat/src/assistant/llm.rs` → `crates/chat/src/assistant/llm/mod.rs`
- Move: `crates/chat/src/assistant/router.rs` → `crates/chat/src/assistant/llm/router.rs`
- Keep: `crates/chat/src/assistant/llm/{rig_client.rs,traced_client.rs}`
- Create only populated façades: `assistant/{understanding,context,retrieval,state,presentation}/mod.rs`
- Modify: `crates/chat/src/assistant/mod.rs` and demonstrated callers.

**Steps:**

- [x] **5.1 Search callers for every moved leaf and record intentional public exports.**
  ```bash
  git grep -n -E 'assistant::(intent|extraction|clarification_resolver|context_builder|context|clarification|canonical_state|canonical_state_repo|evidence|retrieval|reranker|graph|memory|response_builder|response|renderer|contracts|router|llm)' -- ':(glob)crates/**/*.rs'
  ```
  Expected: a caller list that distinguishes crate API exports from intra-crate imports.

- [x] **5.2 Move one cohesive group at a time in this order:** understanding, context, retrieval, state, presentation, then LLM. After each group run `cargo check -p chat` before proceeding.

- [x] **5.3 Keep `assistant/mod.rs` as a thin compatibility façade.** Re-export only existing names needed by current callers; do not create empty destinations or broad wildcard exports.

- [x] **5.4 Preserve the two `RetrievalPlan` identities while updating imports.** `chat::planner::RetrievalPlan` remains legacy plan data; the moved assistant type becomes `assistant::retrieval::evidence::RetrievalPlan` behind any necessary old-path re-export.

- [x] **5.5 Validate focused assistant contracts.**
  ```bash
  cargo fmt --check
  cargo check -p chat
  cargo test -p chat --test assistant_contracts
  cargo test -p chat --test assistant_context_window
  cargo test -p chat --test assistant_retrieval_evidence
  cargo test -p chat --test assistant_response
  cargo test -p chat temporal_reuses_the_same_job_reference_after_clarification
  git diff --check
  ```
  Expected: all commands exit `0`; type identities, JSON contracts, clarification resolution, retrieval, and rendering are behaviorally unchanged.

---

## Task 6 — Put execution and supporting repositories under their owners

**Files:**
- Move: `crates/chat/src/assistant/tool.rs` → `crates/chat/src/assistant/execution/tool/mod.rs`
- Move: `crates/chat/src/assistant/runtime/mod.rs` → `crates/chat/src/assistant/execution/runtime/mod.rs`
- Move: `crates/chat/src/assistant/job_memory_repo.rs` → `crates/chat/src/job/repository/assistant_memory.rs`
- Move: `crates/chat/src/assistant/session_memory_repo.rs` → `crates/chat/src/conversation/repository/assistant_memory.rs`
- Move: `crates/chat/src/assistant/llm_trace_repo.rs` → `crates/chat/src/audit/llm_trace_repository.rs`
- Move: `crates/chat/src/audit.rs` → `crates/chat/src/audit/mod.rs`
- Move: `crates/chat/src/assistant/swiftide_index.rs` → `crates/chat/src/knowledge/index/swiftide.rs`
- Create only populated façades: `assistant/execution/mod.rs` and `assistant/execution/tool/mod.rs`.
- Modify owner façades and demonstrated callers.

**Steps:**

- [x] **6.1 Inventory execution and repository callers.**
  ```bash
  git grep -n -E 'assistant::(tool|runtime|job_memory_repo|session_memory_repo|llm_trace_repo|swiftide_index)|crate::audit' -- ':(glob)crates/**/*.rs'
  ```
  Expected: all imports and constructor sites are known before the ownership move.

- [x] **6.2 Move `tool` and `runtime` unchanged under `assistant/execution`; keep narrow old-path re-exports in `assistant/mod.rs`.**

- [x] **6.3 Move each repository to its owning durable/support feature.** Keep SQL calls in these repository files; do not pull persistence into job, conversation, execution, or audit services.

- [x] **6.4 Move the Swiftide adapter under `knowledge/index` and re-export only its demonstrated integration surface.** Do not change index persistence, embedding, or retrieval behavior.

- [x] **6.5 Validate execution and ownership.**
  ```bash
  cargo fmt --check
  cargo check -p chat
  cargo test -p chat --test assistant_graph_runtime
  cargo test -p chat --test assistant_repositories
  cargo test -p chat --test assistant_swiftide_index
  cargo test -p chat canonical_snapshot_rejects_malformed_parameters
  git diff --check
  ```
  Expected: all commands exit `0`; execution, persistence, trace audit, and indexing behavior are unchanged.

---

## Task 7 — Move legacy computational modules behind assistant ownership

**Files:**
- Move: `crates/chat/src/chat/classifier.rs` and `chat/classifier/tests.rs` → `crates/chat/src/assistant/understanding/classifier/{mod.rs,tests.rs}`
- Move: `crates/chat/src/chat/planner.rs` and `chat/planner/tests.rs` → `crates/chat/src/assistant/execution/plan/{mod.rs,tests.rs}`
- Move: `crates/chat/src/chat/llm.rs` → `crates/chat/src/assistant/llm/planner_client.rs`
- Move: `crates/chat/src/chat/pipeline/` → `crates/chat/src/assistant/legacy_pipeline/`
- Keep temporarily when required: `crates/chat/src/chat/executor.rs`
- Modify: `crates/chat/src/chat/mod.rs`, assistant façades, and demonstrated callers.

**Steps:**

- [x] **7.1 Inventory legacy callers and all SQL use before moving.**
  ```bash
  git grep -n -E 'chat::(classifier|planner|llm|pipeline|executor)|crate::chat::(classifier|planner|llm|pipeline|executor)|sqlx::|query(_as)?!' -- ':(glob)crates/chat/src/**/*.rs'
  ```
  Expected: caller and SQL inventories show whether `chat/executor.rs` can move without violating repository-only SQL.

- [x] **7.2 Move classifier to understanding, planner to `assistant/execution/plan`, legacy LLM planner client to `assistant/llm/planner_client.rs`, and the pipeline directory to `assistant/legacy_pipeline`.** Preserve tests and all public signatures.

- [x] **7.3 Leave `crates/chat/src/chat/executor.rs` in place if moving it would put direct SQL execution outside a repository.** Record that concrete reason; do not disguise a layer violation as structural progress.

- [x] **7.4 Keep narrow old `chat::*` compatibility re-exports.** Preserve `chat::planner::RetrievalPlan` as distinct from assistant evidence `RetrievalPlan`.

- [x] **7.5 Validate legacy behavior.**
  ```bash
  cargo fmt --check
  cargo check -p chat
  cargo test -p chat --test classification_semantic
  cargo test -p chat --test lqr_layered_retrieval
  cargo test -p chat --test chat_no_loop
  cargo test -p chat --test chat_full_flow
  git diff --check
  ```
  Expected: all commands exit `0`; classifier, planning, LLM, strict pipeline, and executor behavior are unchanged.

---

## Task 8 — Decompose the five mixed-responsibility files

**Files:**
- Split: `crates/chat/src/assistant/execution/runtime/mod.rs` into `runtime/{mod.rs,clarification.rs,extraction.rs,planning.rs,execution.rs,transition.rs,tests.rs}`
- Split: `crates/chat/src/job/service/mod.rs` into `service/{mod.rs,run.rs,events.rs,shadow.rs}`
- Split: `crates/chat/src/assistant/understanding/extraction.rs` into `understanding/extraction/{mod.rs,temporal.rs,token.rs,domain.rs,quantity.rs,tests.rs}`
- Split: `crates/chat/src/assistant/execution/tool/mod.rs` into `tool/{mod.rs,parameters.rs,planning.rs,guard.rs,tests.rs}`
- Split: `crates/chat/src/assistant/context/canonical_state.rs` into `context/canonical_state/{mod.rs,contracts.rs,facts.rs,merge.rs,tests.rs}`
- Modify only direct callers needed to preserve signatures.

**Steps:**

- [x] **8.1 Before each split, write a responsibility map using existing symbols.** Runtime owns clarification, extraction coordination, planning, execution, and transitions; `JobService` owns run flow, events, and shadow writes; extraction owns temporal/token/domain/quantity parsing; tool owns parameters/planning/guarding; canonical state owns contracts/facts/merge. If a listed leaf has no real behavior, keep that responsibility in the cohesive parent rather than creating an empty file.

- [x] **8.2 Extract tests first when they obscure production flow.** Move existing tests unchanged to the specified `tests.rs`; do not add broad new scaffolding.

- [x] **8.3 Split runtime mechanically and leave `runtime/mod.rs` as the thin façade/coordinator.** Preserve the exact transition order, terminal states, clarification payload, source-intent recovery, retrieval trace, approved plan, and execution behavior.

- [x] **8.4 Split `JobService` mechanically into `run`, `events`, and `shadow`.** Preserve the `JobService` type, constructor, public methods, event ordering, checkpoint boundaries, revisions, hashes, and Redis live-state semantics.

- [x] **8.5 Split extraction, tool, and canonical state mechanically.** Preserve all public signatures, deterministic provenance, Jakarta/reference-time handling, parameter validation, policy guards, stable UUID behavior, fact precedence, clear/list algebra, and replay behavior.

- [x] **8.6 Review every resulting production file for cohesion.** Approximately 400 LOC mandates this review, but does not mandate another split. Record a concrete cohesion justification for any file retained near/above that size.

- [x] **8.7 Run focused checks after each split, then the phase gate.**
  ```bash
  cargo test -p chat semantic_router_unavailable_fails_closed
  cargo test -p chat exact_pending_option_id_resolves_before_router
  cargo test -p chat temporal_uses_jakarta_date_and_exact_period_boundaries
  cargo test -p chat temporal_reuses_the_same_job_reference_after_clarification
  cargo test -p chat canonical_snapshot_rejects_malformed_parameters
  cargo test -p chat precedence_clear_and_list_algebra
  cargo test -p chat metric_mismatch_rejected
  cargo test -p chat metric_match_accepted
  cargo fmt --check
  cargo check
  cargo test -p chat --lib
  git diff --check
  ```
  Expected: every command exits `0`; no signature, state transition, authorization, persistence, SQL, or response behavior changes.

---

## Task 9 — Remove proven-internal shims, tighten visibility, and synchronize docs

**Files:**
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/chat/src/{lib.rs,chat/mod.rs,assistant/mod.rs}`
- Modify: thin `mod.rs` façades created in Tasks 2–8.
- Modify: `docs/architecture/overview.md`
- Modify: `docs/architecture/chat-data-model.md`
- Modify: `docs/current/status.md`
- Modify: `docs/current/active-context.md` only if an active rule/path became stale.
- Do not modify product behavior, migrations, `knowledge/**/*.yaml`, or `queries/**/*.sql`.

**Steps:**

- [x] **9.1 Search every compatibility path before deleting it.**
  ```bash
  git grep -n -E 'core::db|crate::chat::(model|repository|service|classifier|planner|llm|pipeline)|assistant::(tool|runtime|job_memory_repo|session_memory_repo|llm_trace_repo|swiftide_index)' -- ':(glob)crates/**/*.rs'
  ```
  Expected: only shim declarations remain before removal. If a real caller remains, migrate it first or retain the shim with a documented caller.

- [x] **9.2 Remove only internal compatibility shims proven unused by the search.** Keep intentional external crate APIs and any demonstrated integration path. Remove temporary `pub use database as db` only after no caller uses `core::db`.

- [x] **9.3 Tighten visibility from `pub` to private, `pub(super)`, or `pub(crate)` based on actual callers.** Do not widen visibility to make tests or moves convenient.

- [x] **9.4 Verify layer ownership by search.**
  ```bash
  git grep -n -E 'sqlx::|query(_as)?!' -- ':(glob)crates/chat/src/api/**/*.rs' ':(glob)crates/chat/src/assistant/**/*.rs' ':(glob)crates/chat/src/job/service/**/*.rs' ':(glob)crates/chat/src/conversation/service/**/*.rs' || true
  ```
  Expected: no SQL execution in handlers, services, or assistant orchestration. Any intentional executor exception must remain at its pre-refactor location and be documented, not newly introduced.

- [x] **9.5 Update architecture and current-state docs to match the implemented tree.** State that exactly three crates remain; document conversation, job, assistant subfeatures, knowledge, policy, and audit ownership; preserve same-job clarification, bearer-admin auth, office-bound SQL, PII, PostgreSQL/Redis, and HTTP/JSON invariants.

  Expected: format, checks, and tests exit `0`; diff checks are clean; status/diff show only the planned module moves, import/visibility adjustments, and documentation updates. No dependency, migration, YAML, SQL, external-contract, or commit changes appear.

- [x] **9.7 Self-review against the authoritative spec and this plan.** Confirm every planned path exists and owns real behavior; no placeholders or empty scaffolding exist; no contradictory old/new module declarations remain; moved and re-exported types have one identity; both `RetrievalPlan` types remain distinct; all public signatures and behavior are preserved; the final diff stays within the structural/documentation scope.

## Completion gate

- [x] The refactor is complete only when all nine tasks are checked, every task was independently buildable, all final commands have the expected result, the documentation describes the actual tree, and self-review finds no missing spec requirement, placeholder, contradiction, path mismatch, type inconsistency, scope expansion, or commit step.
