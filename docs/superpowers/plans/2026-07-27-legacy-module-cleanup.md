# Legacy Module Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Delete the dead code and dissolve the misleading façades left by the 2026-07-18 module refactor, relocating every still-live piece to a correctly-owned home with zero behavior change.

**Architecture:** Mechanical `git mv` + import-path edits only. Each task is independently buildable and leaves a green `cargo check -p chat`. No logic is rewritten while ownership changes. Existing contract tests are the behavioral guard — no new tests are added (the changes are pure relocation/deletion).

**Tech Stack:** Rust edition 2024, Cargo workspace, axum, sqlx, PostgreSQL, Redis. Existing dependencies only.

**Authoritative spec:** `docs/superpowers/specs/2026-07-27-legacy-module-cleanup-design.md`.

## Global Constraints

- Keep exactly `crates/app`, `crates/core`, `crates/chat`. No new crate.
- No new dependencies, migrations, `knowledge/**/*.yaml`, or `queries/**/*.sql`.
- Preserve every public signature, HTTP route, JSON envelope, SSE event, and user-visible response.
- Preserve bearer-session JWT chat auth, `role == "admin"`, `project_admin_principal`; `X-API-Key` stays an optional office opt-down.
- Preserve office-scope enforcement inside approved SQL via bound `office_ids`; never post-filter in Rust; never move `sqlx` into handlers, services, or `assistant/**`.
- Preserve PII policy and sanitized errors; PostgreSQL durable truth; Redis live-only; same-job clarification via `POST /chat/jobs/{job_id}/responses`.
- Move code mechanically; do not rewrite logic. Before deleting any symbol, re-confirm zero live callers by search; if a live path appears, relocate instead of delete.
- `assistant/temporal/` is **out of scope** — do not touch it.
- **Do not include commit steps.** (Per spec.) A task is done when its listed checks exit `0` and `git diff --check` is clean.

## Deviation from spec wording

The spec's Section B names the moved model file `intent.rs`. This plan keeps the original filename **`model.rs`** at the new location to keep the move purely mechanical (avoids editing every internal `model::` reference and avoids confusion with the unrelated `assistant/understanding/intent.rs`). Behavior and public types are identical either way.

---

## Task 1: Record a green baseline

**Files:**
- Read only. Create/modify none.

- [ ] **Step 1: Verify formatting and full-workspace compilation**

Run:
```bash
cargo fmt --check
cargo check
```
Expected: both exit `0`.

- [ ] **Step 2: Run the contract tests that must stay green through this plan**

Run:
```bash
cargo test -p chat --test classification_semantic
cargo test -p chat --test lqr_layered_retrieval
cargo test -p chat --test chat_full_flow
cargo test -p chat --test savings_answer_quality
cargo test -p chat missing_date_range_triggers_clarification_and_continues_same_job
```
Expected: each passes. If any is red or blocked (e.g. no database), record the exact command and error before proceeding — do not start moving code on an unexplained red baseline.

---

## Task 2: Delete dead `chat/formatter` (`ResponseText`)

`crates/chat/src/chat/formatter/labels.rs` (`ResponseText` and its methods) is an **orphan file**: no `mod formatter;` declaration exists anywhere, so it is not compiled into the crate and has zero callers. Pure deletion — no `chat/mod.rs` edit.

**Files:**
- Delete: `crates/chat/src/chat/formatter/labels.rs` (and the `crates/chat/src/chat/formatter/` directory)

- [ ] **Step 1: Re-confirm it is an orphan with zero callers**

Run:
```bash
git grep -nE 'mod formatter|mod labels' -- 'crates'
git grep -nE 'ResponseText|formatter::labels|chat::formatter' -- 'crates'
```
Expected: the first prints nothing (no module declaration); the second prints only hits inside `crates/chat/src/chat/formatter/labels.rs`. If a real caller or a `mod formatter;` declaration appears, stop — do not delete.

- [ ] **Step 2: Delete the file and its now-empty directory**

Run:
```bash
git rm crates/chat/src/chat/formatter/labels.rs
rmdir crates/chat/src/chat/formatter 2>/dev/null || true
```

- [ ] **Step 4: Validate**

Run:
```bash
cargo fmt --check
cargo check -p chat
git diff --check
```
Expected: all exit `0`.

---

## Task 3: Move `lqr` into `understanding`

`legacy_pipeline::lqr` is standalone from the pipeline's dead code and is mutually coupled with `understanding::classifier` (classifier holds `Vec<lqr::LayerTrace>`; lqr imports `classifier::{ClarificationOption, ClassificationCandidate}`). Co-locate it under `understanding`.

**Files:**
- Move: `crates/chat/src/assistant/legacy_pipeline/lqr.rs` → `crates/chat/src/assistant/understanding/lqr.rs`
- Modify: `crates/chat/src/assistant/understanding/mod.rs`
- Modify: `crates/chat/src/assistant/legacy_pipeline/mod.rs`
- Modify: `crates/chat/src/assistant/understanding/classifier/mod.rs:78`
- Modify: `crates/chat/tests/lqr_layered_retrieval.rs`

**Interfaces:**
- Produces: `crate::assistant::understanding::lqr::{LayerTrace, LqrInputs, LqrOutcome, LqrResult, DomainDecision, CapabilityDecision, decide_domain_layer, aggregate_confidence, decide_capability_layer}` (all names unchanged).

- [ ] **Step 1: Move the file**

Run:
```bash
git mv crates/chat/src/assistant/legacy_pipeline/lqr.rs crates/chat/src/assistant/understanding/lqr.rs
```

- [ ] **Step 2: Register the module under `understanding`**

In `crates/chat/src/assistant/understanding/mod.rs`, add `pub mod lqr;` in alphabetical position. Result:
```rust
pub mod clarification_resolver;
pub mod classifier;
pub mod extraction;
pub mod intent;
pub mod lqr;
```

- [ ] **Step 3: Deregister `lqr` from `legacy_pipeline`**

In `crates/chat/src/assistant/legacy_pipeline/mod.rs`, delete the line `pub mod lqr;`.

- [ ] **Step 4: Repoint the classifier reference**

In `crates/chat/src/assistant/understanding/classifier/mod.rs` line 78, change:
```rust
    pub layers: Vec<crate::assistant::legacy_pipeline::lqr::LayerTrace>,
```
to:
```rust
    pub layers: Vec<super::lqr::LayerTrace>,
```

- [ ] **Step 5: Repoint the integration test**

In `crates/chat/tests/lqr_layered_retrieval.rs`, replace both occurrences of `chat::chat::pipeline::lqr` with `chat::assistant::understanding::lqr`. Result (lines 9 and 12):
```rust
    let decision = chat::assistant::understanding::lqr::decide_domain_layer(&policy, &ranked);
```
```rust
        chat::assistant::understanding::lqr::DomainDecision::Reject { reason } => {
```

- [ ] **Step 6: Validate**

Run:
```bash
cargo fmt --check
cargo check -p chat
cargo test -p chat --test lqr_layered_retrieval
cargo test -p chat --test classification_semantic
git diff --check
```
Expected: all exit `0`; both tests pass.

---

## Task 4: Move the live `legacy_pipeline` cluster to `assistant/llm/semantic`, delete the dead pipeline, remove the module

The survivors `answer`, `parser`, `retrieval`, `model` are consumed only by `assistant/llm/planner_client.rs` and are coupled (`parser` and `retrieval` import `model`). Move them together. Delete the dead top-level pipeline and its `resolver`/`router`/`evidence` submodules, then delete the directory.

**Files:**
- Create dir: `crates/chat/src/assistant/llm/semantic/`
- Move: `.../legacy_pipeline/answer.rs` → `.../llm/semantic/answer.rs`
- Move: `.../legacy_pipeline/parser.rs` → `.../llm/semantic/parser.rs`
- Move: `.../legacy_pipeline/retrieval.rs` → `.../llm/semantic/retrieval.rs`
- Move: `.../legacy_pipeline/model.rs` → `.../llm/semantic/model.rs`
- Create: `crates/chat/src/assistant/llm/semantic/mod.rs`
- Delete: `.../legacy_pipeline/resolver.rs`, `.../legacy_pipeline/router.rs`, `.../legacy_pipeline/evidence.rs`, `.../legacy_pipeline/mod.rs` (whole dir)
- Modify: `crates/chat/src/assistant/mod.rs` (remove `pub mod legacy_pipeline;`)
- Modify: `crates/chat/src/assistant/llm/mod.rs` (add `pub mod semantic;`)
- Modify: `crates/chat/src/assistant/llm/planner_client.rs:8-12`
- Modify: `crates/chat/src/chat/mod.rs` (remove the dead `pipeline` re-export)

**Interfaces:**
- Produces: `crate::assistant::llm::semantic::answer::{GeneratedAnswer, parse_generated_answer}`, `crate::assistant::llm::semantic::model::ParsedIntent`, `crate::assistant::llm::semantic::parser::parse_semantic_response`, `crate::assistant::llm::semantic::retrieval::{LayeredRetrievalPlan, parse_layered_retrieval_response}` (names unchanged).

- [ ] **Step 1: Re-confirm the dead set has no live caller**

Run:
```bash
git grep -nE 'run_strict_pipeline|StrictPipeline|legacy_pipeline::(resolver|router|evidence)' -- 'crates' | grep -v 'legacy_pipeline/'
```
Expected: no hits. If any appear, stop and relocate that symbol instead of deleting.

- [ ] **Step 2: Move the four live submodules**

Run:
```bash
mkdir -p crates/chat/src/assistant/llm/semantic
git mv crates/chat/src/assistant/legacy_pipeline/answer.rs    crates/chat/src/assistant/llm/semantic/answer.rs
git mv crates/chat/src/assistant/legacy_pipeline/parser.rs    crates/chat/src/assistant/llm/semantic/parser.rs
git mv crates/chat/src/assistant/legacy_pipeline/retrieval.rs crates/chat/src/assistant/llm/semantic/retrieval.rs
git mv crates/chat/src/assistant/legacy_pipeline/model.rs     crates/chat/src/assistant/llm/semantic/model.rs
```

- [ ] **Step 3: Create the `semantic` module file**

Create `crates/chat/src/assistant/llm/semantic/mod.rs`:
```rust
pub mod answer;
pub mod model;
pub mod parser;
pub mod retrieval;
```

- [ ] **Step 4: Register `semantic` under `llm`**

In `crates/chat/src/assistant/llm/mod.rs`, add `pub mod semantic;` alongside the existing module declarations (after `pub mod router;`):
```rust
pub mod planner_client;
pub mod rig_client;
pub mod router;
pub mod semantic;
pub mod traced_client;
```

- [ ] **Step 5: Fix internal cross-references in the moved files**

In `crates/chat/src/assistant/llm/semantic/parser.rs` and `.../retrieval.rs`, replace every `crate::assistant::legacy_pipeline::model` with `super::model`. Run to find them:
```bash
git grep -nE 'legacy_pipeline::model' -- 'crates/chat/src/assistant/llm/semantic'
```
Edit each hit: `crate::assistant::legacy_pipeline::model::X` → `super::model::X`. (`answer.rs` and `model.rs` have no such reference.)

- [ ] **Step 6: Delete the dead pipeline submodules and directory**

Delete the dead top-level pipeline body and strict-only types, then remove the directory:
```bash
git rm crates/chat/src/assistant/legacy_pipeline/resolver.rs
git rm crates/chat/src/assistant/legacy_pipeline/router.rs
git rm crates/chat/src/assistant/legacy_pipeline/evidence.rs
git rm crates/chat/src/assistant/legacy_pipeline/mod.rs
rmdir crates/chat/src/assistant/legacy_pipeline 2>/dev/null || true
```
The strict-only types (`StrictPipelineState`, `RouteDecision`, `StrictPipelineError`) lived in `mod.rs`/`model.rs` and were only consumed by the deleted `run_strict_pipeline`/`router`/`resolver`. If `cargo check` in Step 9 reports any of them still referenced, restore that specific type into `semantic/model.rs`; otherwise leave them gone.

- [ ] **Step 7: Deregister `legacy_pipeline` from `assistant`**

In `crates/chat/src/assistant/mod.rs`, delete the line `pub mod legacy_pipeline;` (line 3).

- [ ] **Step 8: Repoint the one production importer**

In `crates/chat/src/assistant/llm/planner_client.rs`, replace lines 8–12:
```rust
use crate::assistant::legacy_pipeline::answer::{GeneratedAnswer, parse_generated_answer};
use crate::assistant::legacy_pipeline::model::ParsedIntent;
use crate::assistant::legacy_pipeline::parser::parse_semantic_response;
use crate::assistant::legacy_pipeline::retrieval::{
    LayeredRetrievalPlan, parse_layered_retrieval_response,
};
```
with:
```rust
use crate::assistant::llm::semantic::answer::{GeneratedAnswer, parse_generated_answer};
use crate::assistant::llm::semantic::model::ParsedIntent;
use crate::assistant::llm::semantic::parser::parse_semantic_response;
use crate::assistant::llm::semantic::retrieval::{
    LayeredRetrievalPlan, parse_layered_retrieval_response,
};
```

- [ ] **Step 9: Remove the now-broken dead `pipeline` re-export from `chat/mod.rs`**

In `crates/chat/src/chat/mod.rs`, delete the block (it re-exported the deleted `legacy_pipeline` and has no remaining caller — the test was repointed in Task 3):
```rust
pub mod pipeline {
    pub use crate::assistant::legacy_pipeline::*;
}
```

- [ ] **Step 10: Validate**

Run:
```bash
cargo fmt --check
cargo check -p chat
cargo test -p chat --test classification_semantic
cargo test -p chat --test lqr_layered_retrieval
cargo test -p chat --test savings_answer_quality
git diff --check
```
Expected: all exit `0`; tests pass. `git grep -nE 'legacy_pipeline' -- crates` returns nothing.

---

## Task 5: Move `executor.rs` into a repository (`crate::execution::repository`)

`chat/executor.rs::execute_plan` is live (called by `assistant/execution/runtime/mod.rs`) and holds raw approved-SQL `sqlx`. Move it into a real repository module so SQL leaves `assistant/**` and returns to the repository layer.

**Files:**
- Create: `crates/chat/src/execution/mod.rs`
- Move: `crates/chat/src/chat/executor.rs` → `crates/chat/src/execution/repository.rs`
- Modify: `crates/chat/src/lib.rs` (add `pub mod execution;`)
- Modify: `crates/chat/src/chat/mod.rs` (remove `pub mod executor;`)
- Modify: `crates/chat/src/assistant/execution/runtime/mod.rs:49`

**Interfaces:**
- Consumes: `crate::assistant::execution::plan::{ExecutionPlan, PolicyDecision, PolicyDecisionStatus}`, `crate::knowledge::model::{KnowledgeCatalog, QueryParameter}` (unchanged imports inside the moved file).
- Produces: `crate::execution::repository::execute_plan(...)` (same signature as before).

- [ ] **Step 1: Move the file**

Run:
```bash
mkdir -p crates/chat/src/execution
git mv crates/chat/src/chat/executor.rs crates/chat/src/execution/repository.rs
```
The moved file's internal `use crate::assistant::execution::plan::...` and `use crate::knowledge::model::...` lines stay valid (absolute paths) — do not edit them.

- [ ] **Step 2: Create the `execution` module file**

Create `crates/chat/src/execution/mod.rs`:
```rust
pub mod repository;
```

- [ ] **Step 3: Register `execution` in the crate root**

In `crates/chat/src/lib.rs`, add `pub mod execution;` in alphabetical position:
```rust
pub mod api;
pub mod assistant;
pub mod audit;
pub mod chat;
pub mod conversation;
pub mod execution;
pub mod job;
pub mod knowledge;
pub mod management;
pub mod policy;
```

- [ ] **Step 4: Remove `executor` from `chat/mod.rs`**

In `crates/chat/src/chat/mod.rs`, delete the line `pub mod executor;`.

- [ ] **Step 5: Repoint the runtime importer**

In `crates/chat/src/assistant/execution/runtime/mod.rs` line 49, change:
```rust
use crate::chat::executor::execute_plan;
```
to:
```rust
use crate::execution::repository::execute_plan;
```

- [ ] **Step 6: Validate (including that no SQL remains under assistant)**

Run:
```bash
cargo fmt --check
cargo check -p chat
git grep -nE 'sqlx::|query(_as)?!' -- 'crates/chat/src/assistant/**/*.rs' 'crates/chat/src/api/**/*.rs' || echo "clean: no SQL in assistant/api"
cargo test -p chat --test chat_full_flow
git diff --check
```
Expected: `fmt`/`check` exit `0`; the SQL grep prints `clean: no SQL in assistant/api` (no matches); `chat_full_flow` passes.

---

## Task 6: Delete the empty `chat` module

After Tasks 2–5, `chat/mod.rs` holds only dead re-exports (`model`, `repository`, `service`, `classifier`, `llm`, `planner`) with zero callers. Remove the module entirely.

**Files:**
- Delete: `crates/chat/src/chat/mod.rs` (and the `crates/chat/src/chat/` directory)
- Modify: `crates/chat/src/lib.rs` (remove `pub mod chat;`)

- [ ] **Step 1: Re-confirm zero callers of every remaining `chat::` façade path**

Run:
```bash
git grep -nE '\bchat::(model|repository|service|classifier|planner|llm)\b' -- 'crates' | grep -v 'crates/chat/src/chat/mod.rs'
```
Expected: no hits. If any real caller remains, migrate it to the canonical path (`crate::conversation::…`, `crate::job::…`, `crate::assistant::…`) before deleting.

- [ ] **Step 2: Delete the module**

Run:
```bash
git rm crates/chat/src/chat/mod.rs
rmdir crates/chat/src/chat 2>/dev/null || true
```

- [ ] **Step 3: Remove the declaration from the crate root**

In `crates/chat/src/lib.rs`, delete the line `pub mod chat;`. Result:
```rust
pub mod api;
pub mod assistant;
pub mod audit;
pub mod conversation;
pub mod execution;
pub mod job;
pub mod knowledge;
pub mod management;
pub mod policy;
```

- [ ] **Step 4: Final phase-gate validation**

Run:
```bash
cargo fmt --check
cargo check
cargo test -p chat --lib
cargo test -p chat --test classification_semantic
cargo test -p chat --test lqr_layered_retrieval
cargo test -p chat --test chat_full_flow
cargo test -p chat --test savings_answer_quality
cargo test -p chat missing_date_range_triggers_clarification_and_continues_same_job
git grep -nE 'legacy_pipeline|chat::(model|repository|service|classifier|planner|llm|pipeline|executor|formatter)' -- 'crates'
git diff --check
```
Expected: `fmt`/`check`/tests exit `0`; the final `git grep` returns nothing; `git diff --check` is clean. `crates/chat/src/chat/` no longer exists.

---

## Deliberately deferred

Spec Section E (tightening moved symbols from `pub` to `pub(crate)`/`pub(super)`) is **not** a task here. Mechanical moves preserve each symbol's existing working visibility; retightening is optional polish that adds edit churn and compile risk with no behavioral gain. Do it as a separate follow-up only if a concrete need arises.

## Completion gate

The cleanup is complete only when: all six tasks are checked; each task was independently buildable; the final `git grep` for legacy paths returns nothing; `assistant/temporal/` is untouched; no dependency, migration, YAML, or approved-SQL changed; no commit step was performed; and all listed checks exit `0`.
