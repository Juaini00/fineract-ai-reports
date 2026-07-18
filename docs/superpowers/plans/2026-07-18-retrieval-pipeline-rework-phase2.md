# Retrieval Pipeline Rework — Phase 2 (consolidated) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close every remaining sub-issue of the retrieval pipeline rework epic (issues 02, 03, 05, 07, 08) on a single feature branch so the full stack — LLM reranker, browse primitives, schemars contracts, eval harness, and the Turn-2 clarification fix — can be validated end-to-end before merging to `master`.

**Architecture:** Phase 1 shipped semantic-first retrieval + trace observability + the domain-filter fix. Phase 2 layers the LLM reranker on top of the top-K candidates (replacing `EvidenceEvaluator`), adds browse capabilities so "give me N X" queries actually have targets to route to, fixes the clarification-reply routing bug that Phase 1 surfaced, tightens LLM I/O with `schemars`-derived schemas, and locks in the accuracy floor with a fixture eval harness. All five issues share the same code surface (`crates/chat/src/assistant/`) — bundling them lets us validate the emergent behavior once, not five times.

**Tech Stack:** Rust edition 2024, axum 0.8, sqlx 0.9 (postgres+pgvector), rig-core 0.40, swiftide 0.32, schemars 1.2, serde_json, tracing, cargo test. All deps already declared in workspace `Cargo.toml` (no new deps).

## Global Constraints

- Layer order stays `route → service → repository → database` (`CLAUDE.md`).
- No `sqlx` in handlers/services — repository layer only.
- HTTP envelope `{ success, data, error }`; errors via `ApiError`.
- Schema changes only via `migrations/*.sql`. Phase 2 introduces NO migration.
- Do not widen `allowed_capabilities` — office scope and capability scope stay enforced.
- Ponytail Mode (AGENTS.md): smallest correct change; no preemptive refactors.
- Rust identifiers in English; prose comments in English.
- Commits must NOT `--no-verify` or skip hooks.
- Existing `#[ignore]`d tests (`savings_answer_respects_narrow_office_scope`, `savings_clarification_keeps_selected_capability_for_parameter_only_reply`) MUST be un-ignored and green by the end of this phase — they are exactly what issue 02 targets.
- Base branch: `feat/retrieval-phase-2` off `master` at `46725a3` (Phase 1 merged).

## Phase scope

This plan implements all remaining sub-issues in `docs/issues/retrieval-pipeline-rework/`. Each task references its issue file for the full problem context — read the issue before starting the task.

## Task sequence and dependency

```
Task 1 (issue 08 Bug A)  → clarification display fallback + YAML audit
Task 2 (issue 08 Bug B)  → clarification-reply routing fix
Task 3 (issue 03)        → 3 browse capabilities
Task 4 (issue 05)        → schemars-derived LLM I/O
Task 5 (issue 02)        → LLM reranker replaces EvidenceEvaluator
Task 6 (issue 07)        → fixture eval harness (locks accuracy floor)
```

Order rationale:
- 08 first — it's user-visible broken NOW and doesn't depend on the others.
- 03 before 02 — reranker needs "browse" targets to route to, otherwise "give me N X" still dead-ends.
- 05 before 02 — reranker declares its output type via `#[derive(JsonSchema)]`; the plumbing must exist first.
- 07 last — eval harness measures everything above.

## File map (union across tasks)

| Path | Tasks that touch it |
|---|---|
| `crates/chat/src/assistant/runtime/mod.rs` | 2, 5 |
| `crates/chat/src/assistant/router.rs` | 4 |
| `crates/chat/src/assistant/llm.rs` | 4, 5 |
| `crates/chat/src/assistant/reranker.rs` (new) | 5 |
| `crates/chat/src/assistant/evidence.rs` | 5 (delete `EvidenceEvaluator`) |
| `crates/chat/src/assistant/clarification.rs` | 1, 2 |
| `crates/chat/src/assistant/tool.rs` | 2 (fix `verify_capability_metric`) |
| `crates/chat/src/assistant/mod.rs` | 4, 5 (re-exports) |
| `knowledge/capabilities/organization/office_summary.yaml` | 1 (add display_name/description) |
| `knowledge/capabilities/organization/office_list_basic.yaml` | 3 (new) |
| `knowledge/capabilities/client/client_list_recent.yaml` | 3 (new) |
| `knowledge/capabilities/client/client_random_sample.yaml` | 3 (new) |
| `knowledge/queries/**/*.yaml` | 3 (3 new) |
| `queries/**/*.sql` | 3 (3 new) |
| `crates/chat/tests/retrieval_scoring.rs` | 1, 2, 3, 5 |
| `crates/chat/tests/chat_full_flow.rs` | 2, 3 |
| `crates/chat/tests/savings_answer_quality.rs` | 5 (un-ignore both) |
| `crates/chat/tests/organization_answer_quality.rs` | 5 (tighten assertions) |
| `crates/chat/tests/retrieval_eval.rs` (new) | 6 |
| `crates/chat/tests/fixtures/retrieval_eval/*.yaml` | 6 (new) |

---

## Task 1 — Fix clarification display metadata (issue 08 Bug A)

**Issue:** [docs/issues/retrieval-pipeline-rework/08-clarification-reply-routing-failed.md](../../issues/retrieval-pipeline-rework/08-clarification-reply-routing-failed.md) (Part 1).

**Files:**
- Modify: `crates/chat/src/assistant/clarification.rs` (find the `ClarificationOption` builder).
- Modify: `knowledge/capabilities/organization/office_summary.yaml` — add `display_name` and `description`.
- Audit: `knowledge/capabilities/**/*.yaml` — flag any others missing `display_name`.
- Test: append to `crates/chat/tests/retrieval_scoring.rs`.

**Steps:**

- [ ] **1.1 Grep for missing display_name in catalog:**
  ```bash
  for f in knowledge/capabilities/**/*.yaml; do
    grep -L "display_name:" "$f" && echo "  ↑ missing display_name"
  done
  ```
  If any found beyond `office_summary.yaml`, file a follow-up ticket (do not fix in this task).

- [ ] **1.2 Write failing test:** open `crates/chat/tests/retrieval_scoring.rs`, append a test that constructs a `ClarificationOption` from a capability without `display_name` and asserts the label is a humanized id (e.g., `"Organization Office Summary"`), not the raw id.

- [ ] **1.3 Add humanize_id helper** in `clarification.rs`:
  ```rust
  fn humanize_id(id: &str) -> String {
      id.split('_')
          .map(|part| {
              let mut chars = part.chars();
              match chars.next() {
                  Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                  None => String::new(),
              }
          })
          .collect::<Vec<_>>()
          .join(" ")
  }
  ```
  Use in the option builder:
  ```rust
  label: cap.display_name.clone().unwrap_or_else(|| humanize_id(&cap.id)),
  description: cap.description.clone(),  // null is fine
  ```

- [ ] **1.4 Update the YAML:** add `display_name: Organization Office Summary` and a one-sentence `description` to `knowledge/capabilities/organization/office_summary.yaml`.

- [ ] **1.5 Rebuild vector index (dev):** `curl -X POST http://127.0.0.1:3007/vector-index/rebuild`.

- [ ] **1.6 Run tests:** `cargo test -p chat --lib && cargo test -p chat --test retrieval_scoring`.

- [ ] **1.7 Commit:** `fix(assistant): humanize clarification label + backfill office_summary metadata (issue 08 Bug A)`.

---

## Task 2 — Fix clarification-reply routing failure (issue 08 Bug B)

**Issue:** [docs/issues/retrieval-pipeline-rework/08-clarification-reply-routing-failed.md](../../issues/retrieval-pipeline-rework/08-clarification-reply-routing-failed.md) (Part 2).

**Files:**
- Modify: `crates/chat/src/assistant/runtime/mod.rs` — instrument the `ClarificationReply` block with `tracing::warn!` at every `Err` branch inside `execute_selected_capability`.
- Modify: `crates/chat/src/assistant/tool.rs::verify_capability_metric` — refresh metadata from the current turn's message, not the memory snapshot.
- Test: append to `crates/chat/tests/chat_full_flow.rs`.

**Steps:**

- [ ] **2.1 Reproduce and read the trace:** with the dev server running, replay the Turn 1/Turn 2 sequence from issue 08's Reproduction section. Then `GET /chat/jobs/{id}` and inspect `state_json.retrieval_trace` (added by phase 1 issue 06) to identify which step returned `Err`.

- [ ] **2.2 Write failing test** in `chat_full_flow.rs`:
  ```rust
  #[test]
  fn clarification_selecting_hierarchy_tree_from_office_query_routes_successfully() {
      // Turn 1: "berikan 3 office yg ada pada system saat ini" → clarify
      // Turn 2: select organization_office_hierarchy_tree
      // Expect: job reaches completed with populated table, not error.
  }
  ```

- [ ] **2.3 Fix the root cause identified in step 2.1.** Most likely:
  - In `tool.rs::verify_capability_metric`: replace `memory.deterministic_extraction` reads with the turn's current `source_message` extraction result.
  - OR: in `runtime/mod.rs` clarification branch, allow the target capability's shape to override the original intent's shape (that's the point of clarification).

- [ ] **2.4 Run tests:** `cargo test -p chat` — the new test passes, no existing regressions.

- [ ] **2.5 Commit:** `fix(assistant): route clarification-reply to selected capability without shape mismatch (issue 08 Bug B)`.

---

## Task 3 — Add browse/list capability primitives (issue 03)

**Issue:** [docs/issues/retrieval-pipeline-rework/03-catalog-browse-primitives.md](../../issues/retrieval-pipeline-rework/03-catalog-browse-primitives.md).

**Files (all new):**
- `knowledge/capabilities/organization/office_list_basic.yaml`
- `knowledge/queries/organization/office_list_basic.yaml`
- `queries/organization/office_list_basic.sql`
- `knowledge/capabilities/client/client_list_recent.yaml`
- `knowledge/queries/client/client_list_recent.yaml`
- `queries/client/client_list_recent.sql`
- `knowledge/capabilities/client/client_random_sample.yaml`
- `knowledge/queries/client/client_random_sample.yaml`
- `queries/client/client_random_sample.sql`
- Test: append to `crates/chat/tests/retrieval_scoring.rs` — one fixture per new capability asserting top-1 selection for its target query.

**Steps:**

- [ ] **3.1 Study an existing pair** (e.g., `organization_office_summary.yaml` + its query) to match field layout.

- [ ] **3.2 Write failing tests first** (3 fixtures):
  - `office_list_basic_selected_for_berikan_office_query` → asserts top-1 is `office_list_basic`.
  - `client_list_recent_selected_for_new_clients_query` → asserts top-1 is `client_list_recent`.
  - `client_random_sample_selected_for_sembarang_query` → asserts top-1 is `client_random_sample`.

- [ ] **3.3 Author the 3 capability YAMLs.** Reference issue 03's table for exact shapes.
  - `office_list_basic`: `op=list, subj=office, output=list, pii=none`, `limit` param (default 50, max 200).
  - `client_list_recent`: `op=list, subj=client, grouping=none, output=list, pii=client_identity`, `limit` param.
  - `client_random_sample`: `op=random_sample, subj=client, output=list, pii=client_identity`, `limit` param (max 50).

- [ ] **3.4 Author the 3 query YAMLs** (parameter/output declarations) + 3 SQL files.
  - Every query MUST bind `office_ids` and enforce `require_office_filter=true`.
  - `client_random_sample`: prefer `TABLESAMPLE SYSTEM_ROWS(n)` if pgext available, else `ORDER BY random() LIMIT n`.

- [ ] **3.5 Validate catalog:** `curl -X POST http://127.0.0.1:3007/catalog/validate`.

- [ ] **3.6 Rebuild vector index:** `curl -X POST http://127.0.0.1:3007/vector-index/rebuild`.

- [ ] **3.7 Run tests:** `cargo test -p chat --test retrieval_scoring`.

- [ ] **3.8 Commit:** `feat(knowledge): add office_list_basic, client_list_recent, client_random_sample capabilities (issue 03)`.

---

## Task 4 — Structured LLM contracts via schemars (issue 05)

**Issue:** [docs/issues/retrieval-pipeline-rework/05-schemars-contracts.md](../../issues/retrieval-pipeline-rework/05-schemars-contracts.md).

**Files:**
- Modify: `crates/chat/src/assistant/llm.rs` — extend the `structured` helper to accept an optional `schemars::Schema` and pass it to the provider.
- Modify: `crates/chat/src/assistant/router.rs` — trim the rules array, pass `schema_for!(AssistantIntent)`.
- Modify: `crates/chat/src/assistant/mod.rs` — confirm `#[derive(JsonSchema)]` on `AssistantIntent`, `RequestShape`, related enums (likely already present).

**Steps:**

- [ ] **4.1 Write failing test in `router.rs::tests`:** LLM returns an intent with an invalid enum value for `request_shape.operation` → assert schema-level rejection error mentions the field, not `serde_json` decode error.

- [ ] **4.2 Extend `llm::structured` signature** to accept `schema: Option<schemars::Schema>` and pass through to provider structured-output mode.

- [ ] **4.3 Update `SemanticRouter::route`** to use `schemars::schema_for!(AssistantIntent)`, trim the redundant "rules" array to just semantic guidance (schema now encodes valid enums).

- [ ] **4.4 Verify router still works** for known-good inputs: `cargo test -p chat --test retrieval_scoring` + the existing `chat_full_flow` tests.

- [ ] **4.5 Commit:** `refactor(assistant): derive LLM schemas from types via schemars (issue 05)`.

---

## Task 5 — LLM reranker replaces EvidenceEvaluator (issue 02)

**Issue:** [docs/issues/retrieval-pipeline-rework/02-llm-reranker.md](../../issues/retrieval-pipeline-rework/02-llm-reranker.md).

**Files:**
- Create: `crates/chat/src/assistant/reranker.rs` — `LlmReranker` with `RerankerDecision` struct + `rerank(query, candidates)`.
- Modify: `crates/chat/src/assistant/runtime/mod.rs` — replace `EvidenceEvaluator.evaluate(&plan, &evidence)` with `LlmReranker.rerank(&plan.query_text, &evidence)`.
- Modify: `crates/chat/src/assistant/evidence.rs` — delete `EvidenceEvaluator` and `EvidenceDecision` (move needed variants into reranker).
- Modify: `crates/chat/src/assistant/mod.rs` — re-export `LlmReranker`, `RerankerDecision`.
- Test port: `crates/chat/tests/assistant_retrieval_evidence.rs` — port existing scenarios to the new reranker via stub `LlmClient`.
- Un-ignore: `crates/chat/tests/savings_answer_quality.rs` — remove `#[ignore]` on both tests, verify they pass.

**Steps:**

- [ ] **5.1 Define types in `reranker.rs`:**
  ```rust
  #[derive(Debug, Serialize, Deserialize, JsonSchema)]
  pub struct RerankerDecision {
      pub decision: RerankerVerdict,
      pub capability_id: Option<String>,
      pub confidence: f32,
      pub alternatives: Vec<String>,
      pub reason: String,
  }

  #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
  #[serde(rename_all = "snake_case")]
  pub enum RerankerVerdict { Select, Clarify, Unsupported }
  ```

- [ ] **5.2 Write failing test:** a fake `LlmClient` returning `RerankerDecision::Select { capability_id: Some("client_top_n_by_savings_account_count"), confidence: 0.85, ... }` → runtime routes to that capability. Also test `Clarify` (alternatives non-empty → clarification payload) and `Unsupported` (no capability → structured unsupported response).

- [ ] **5.3 Implement `LlmReranker::rerank(&self, query: &str, candidates: &[Evidence]) -> Result<RerankerDecision>`:**
  - Build a compact input JSON: `{ query, candidates: candidates.take(8).map(|e| { id, title, description, examples, request_shape }) }`.
  - Call `llm::structured::<RerankerDecision>` with schemars-derived schema (from Task 4).
  - On confidence < 0.6 → coerce to `Clarify` with `alternatives` = top-4 candidate ids.
  - Retry once on schema-invalid response; then fall back to `Clarify` with alternatives.

- [ ] **5.4 Swap call site in `runtime/mod.rs`:** delete the `EvidenceEvaluator.evaluate(...)` line, insert `LlmReranker::new(llm).rerank(&plan.query_text, &evidence).await`. Map `RerankerDecision::Select` → `execute_selected_capability`; `Clarify` → `ClarificationPayload::from_alternatives(alts)`; `Unsupported` → existing unsupported response.

- [ ] **5.5 Delete `EvidenceEvaluator` and `EvidenceDecision`** from `evidence.rs`. Delete their tests. Update `retrieval_trace` builder in `runtime/mod.rs` to serialize `RerankerDecision` instead of `EvidenceDecision`.

- [ ] **5.6 Port `assistant_retrieval_evidence.rs`** to the new reranker API. Delete tests that only asserted `MIN_SELECT_SCORE` threshold arithmetic (obsolete).

- [ ] **5.7 Un-ignore savings tests** in `savings_answer_quality.rs`. Run them:
  ```bash
  cargo test -p chat --test savings_answer_quality
  ```
  Expected: both pass. If not, diagnose via retrieval_trace + reranker output — the LLM should pick correctly given the descriptions. If the reranker is picking wrong, tune the input format (add more per-candidate context, e.g. `metrics` or `output_mode`).

- [ ] **5.8 Tighten integration test assertions** that were loosened in phase 1 Task 2 (`chat_full_flow.rs`, `organization_answer_quality.rs`) back to exact-match where the reranker now provides deterministic top-1 selection.

- [ ] **5.9 Run full workspace:** `cargo test --workspace --no-fail-fast`. Expected: all previously-ignored phase-1 tests are now un-ignored and green; no new failures.

- [ ] **5.10 Commit:** `feat(assistant): replace EvidenceEvaluator with LLM reranker via rig-core + schemars (issue 02)`.

---

## Task 6 — Fixture eval harness (issue 07)

**Issue:** [docs/issues/retrieval-pipeline-rework/07-swiftide-eval-harness.md](../../issues/retrieval-pipeline-rework/07-swiftide-eval-harness.md).

**Files (all new):**
- `crates/chat/tests/fixtures/retrieval_eval/*.yaml` — 20 fixtures, ID + EN queries, per acceptance criteria in issue 07.
- `crates/chat/tests/retrieval_eval.rs` — runner.

**Steps:**

- [ ] **6.1 Author 20 fixtures.** Cover: 3 domains (client, organization, savings) × ~7 queries each; balance select/clarify/unsupported; balance ID/EN.

- [ ] **6.2 Write the runner:** loads fixtures, invokes the assistant graph with a stub or real `LlmClient` (env `EVAL_USE_REAL_LLM=1` switches), computes top-1 accuracy per bucket, fails if below floor.

- [ ] **6.3 Set accuracy floors:** 90% overall, 85% per language, 85% per decision-type. If real numbers land lower, do NOT lower the floor — fix the reranker prompt or add fixtures where a gap exists, then re-measure.

- [ ] **6.4 Run:** `cargo test -p chat --test retrieval_eval`. Establish baseline.

- [ ] **6.5 Commit:** `test(assistant): add 20-fixture bilingual retrieval eval harness (issue 07)`.

---

## Whole-branch verification (before opening PR)

- [ ] **V.1 fmt + clippy:** `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **V.2 Full test:** `cargo test --workspace --no-fail-fast`. Expected: 0 fail, 0 ignored (both phase-1-ignored tests now un-ignored by Task 5).
- [ ] **V.3 Live smoke — all 3 original failing queries:**
  ```bash
  # Query A
  curl -X POST http://127.0.0.1:3007/chat/jobs -H "Content-Type: application/json" -H "X-API-Key: $KEY" -d '{"message":"coba berikan saya 5 client sembarang pada tahun ini"}'
  # Query B
  curl -X POST ... -d '{"message":"give me 3 clients where have the most savings account for this year"}'
  # Query C (+ Turn 2)
  curl -X POST ... -d '{"message":"berikan 3 office yg ada pada system saat ini"}'
  # ... then POST /responses with selected_option_id.
  ```
  Expected: A → executes `client_random_sample`; B → executes `client_top_n_by_savings_account_count`; C Turn 1 → clarifies with populated labels; C Turn 2 → executes selected capability without "Routing failed".
- [ ] **V.4 Retrieval trace visible:** `GET /chat/jobs/{id}` shows `state_json.retrieval_trace` with reranker decision instead of `EvidenceDecision`.
- [ ] **V.5 Git log check:** `git log master..HEAD --oneline` shows one commit per task (6 commits), each linked to its issue. No fixup commits.

## Rollout

- After V.1–V.5 all green, open PR.
- CI runs the full workspace suite + eval harness.
- Merge as a merge-commit (not squash) so per-issue commits stay visible in `master` history.
- After merge, update the epic README to mark all sub-issues closed.

## Rollback

If issue 02 (reranker) causes production regressions after merge:
- Revert only the reranker commit — the branch is ordered so Task 5 is the highest-blast-radius change. Previous commits (browse primitives, schemars plumbing, issue 08 fixes) are safe to keep.
- `EvidenceEvaluator` and `EvidenceDecision` are deleted in Task 5 — revert brings them back.
