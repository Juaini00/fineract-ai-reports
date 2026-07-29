# Legacy Module Cleanup Design

**Goal:** Dissolve the three leftover legacy artifacts left behind by the
2026-07-18 module-structure refactor, deleting provably-dead code and giving
every still-live piece a correctly-owned home — with **zero behavior change**.

**Authoritative predecessor:** `docs/superpowers/specs/2026-07-18-rust-project-structure-design.md`
and its plan `docs/superpowers/plans/2026-07-18-rust-module-structure-refactor.md`
(fully implemented; Task 7.3 and Task 9.2 deliberately deferred the items this
spec now finishes).

## Background — why these leftovers exist

The 2026-07-18 refactor moved code into the hybrid feature-first tree behind
temporary compatibility shims. Task 9 was supposed to remove the shims, but
three items survived because they were still referenced or held a layering
concern the refactor chose not to touch:

1. `crates/chat/src/chat/` — a compatibility façade module.
2. `crates/chat/src/assistant/legacy_pipeline/` — a module whose name says
   "legacy" but whose submodules are still imported by live code.
3. `crates/chat/src/chat/executor.rs` — live approved-SQL execution left
   outside a repository (Task 7.3's documented exception).

`crates/chat/src/assistant/temporal/` is **not** legacy — it is live
infrastructure (`BusinessDateSource` / `BusinessDateProvider`) used by `api/`,
`assistant/execution/runtime/`, and `job/service/`. It is explicitly **out of
scope**.

## Reference forensics (verified across `crates/**`, src + tests)

Dead (no production caller):
- `chat/formatter/labels.rs` (`ResponseText` and helpers) — 0 callers anywhere.
- `chat/mod.rs` re-exports `model / repository / service / planner / classifier / llm` — 0 production callers.
- `legacy_pipeline` top-level `run_strict_pipeline` + `StrictPipelineInput/Output`, and submodules `resolver`, `router`, `evidence` — no production caller. The strict-only types in `model.rs` (`StrictPipelineState`, `RouteDecision`, `StrictPipelineError`, and `ResolvedConstraints` if only `resolver` consumes it) die with them.

Live (must be preserved and relocated, not deleted):
- `chat/executor.rs::execute_plan` — called by `assistant/execution/runtime/mod.rs`; contains raw `sqlx` approved-SQL execution.
- `legacy_pipeline::{answer, parser, retrieval}` and the live half of `legacy_pipeline::model` (`ParsedIntent`, `ParsedIntentKind`, `ParsedEntity`, `ParsedConstraints`, `QuantityConstraint`, `RetrievalEvidence`) — imported by `assistant/llm/planner_client.rs`. `parser` and `retrieval` both depend on `model`, so these move as one coupled cluster.
- `legacy_pipeline::lqr` (`LayerTrace`, `decide_domain_layer`, `DomainDecision`, etc.) — standalone (no `model` dependency), imported by `assistant/understanding/classifier` and by the test `tests/lqr_layered_retrieval.rs`.

## Non-negotiable constraints

- Keep exactly `crates/app`, `crates/core`, `crates/chat`. No new crate.
- No new dependencies, migrations, `knowledge/**/*.yaml`, or `queries/**/*.sql`.
- Preserve every public signature, HTTP route, JSON envelope, SSE event, and user-visible response.
- Preserve bearer-session JWT chat auth, `role == "admin"`, `project_admin_principal`; `X-API-Key` stays an optional office opt-down.
- Preserve office-scope enforcement **inside** approved SQL via bound `office_ids`; never post-filter in Rust.
- Preserve PII policy and sanitized errors.
- Preserve PostgreSQL as durable truth; Redis as live SSE coordination only.
- Preserve same-job clarification via `POST /chat/jobs/{job_id}/responses`.
- Move code mechanically; do not rewrite logic while changing ownership.
- Before deleting any "dead" symbol, re-confirm zero live callers by search; if a live path appears, relocate instead of delete.
- Do not include commit steps in the resulting plan tasks.

## Design

### A. Delete provably-dead code
Remove `chat/formatter/` (`labels.rs` + `formatter` mod), the dead
`legacy_pipeline` top-level pipeline function and its `resolver`/`router`/
`evidence` submodules plus the strict-only `model` types, and the dead
`chat/mod.rs` re-exports. Each deletion is gated on a fresh zero-caller search.

### B. Relocate the live `legacy_pipeline` cluster, then delete the directory
The coupled cluster (`answer`, `parser`, `retrieval`, live `model` types) is
consumed by `assistant/llm/planner_client.rs` → move it **together** under
`assistant/llm/` (e.g. `assistant/llm/semantic/{answer,parser,retrieval,intent}.rs`,
where `intent` holds the surviving `model` types). `lqr` is standalone and
consumed by `understanding/classifier` → move to `assistant/understanding/lqr.rs`.
Update the two importers (`planner_client.rs`, `classifier/mod.rs`) to the new
paths. Then delete `assistant/legacy_pipeline/` entirely and drop its `mod`
declaration from `assistant/mod.rs`. No "legacy" name remains on live code.

### C. Relocate `executor.rs` into a repository (fix the layering wart)
Move `chat/executor.rs` → **`crates/chat/src/execution/repository.rs`** (new
feature module `execution` inside the existing `chat` crate — not a new crate).
This returns approved-SQL execution to the repository layer and out of
`assistant/**`, satisfying the Task 9.4 invariant (no SQL under
`api/**`, `assistant/**`, or any `service/**`). Update `assistant/execution/
runtime/mod.rs` import from `crate::chat::executor::execute_plan` →
`crate::execution::repository::execute_plan`. Register `pub mod execution;` in
`lib.rs`. SQL, office binding, and PII behavior are byte-for-byte unchanged.

### D. Remove the `chat` module and fix the one test
After A–C, `chat/mod.rs` and `chat/` hold nothing live → delete the directory
and remove `pub mod chat;` from `lib.rs`. Migrate
`tests/lqr_layered_retrieval.rs` from `chat::chat::pipeline::lqr::…` to
`chat::assistant::understanding::lqr::…`. No other test references the old
paths.

### E. Visibility
Set moved symbols to the narrowest visibility their real callers require
(`pub(crate)` / `pub(super)`); reserve unrestricted `pub` for intentional
crate APIs. Do not widen visibility for convenience.

## Testing / validation strategy
Run from the repo root after each move (independently buildable steps):
```
cargo fmt --check
cargo check -p chat
git diff --check
```
Contract tests that must stay green across the change:
```
cargo test -p chat --test classification_semantic
cargo test -p chat --test lqr_layered_retrieval
cargo test -p chat --test chat_full_flow
cargo test -p chat --test savings_answer_quality
cargo test -p chat missing_date_range_triggers_clarification_and_continues_same_job
```
A move is complete only when its checks exit `0` and `git diff --check` reports
no whitespace errors. If a "dead" deletion turns up a live caller mid-move,
stop and relocate the symbol instead of deleting it.

## Out of scope
- `assistant/temporal/` (live infrastructure, not legacy).
- Any behavior, contract, dependency, migration, YAML, or approved-SQL change.
- Renaming or splitting modules that the 2026-07-18 refactor already placed correctly.
