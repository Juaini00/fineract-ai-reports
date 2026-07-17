# Retrieval Pipeline Rework — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unblock the three failing report queries observed in production logs (2026-07-17) by removing the redundant domain filter, inverting the retrieval pipeline to semantic-first (embedding always runs, shape becomes a score), and persisting the full retrieval trace to `chat_jobs.state_json` for zero-code debugging.

**Architecture:** Delete `domain_compatible` from the candidate filter (subject already carries domain). Refactor `RetrievalEngine::retrieve` so embedding search always executes over `allowed_capabilities`, `compatible_ids` becomes a per-candidate `shape_score(plan, cap) -> f32` used in ranking, and the final `retain` gate is removed. Extend the job repository to merge a `retrieval_trace` JSON subtree into `state_json` after each retrieval pass.

**Tech Stack:** Rust (edition 2024), axum 0.8, sqlx 0.9 (postgres+pgvector), serde_json, tracing, cargo test. Deps in workspace `Cargo.toml` — no new deps for phase 1.

## Global Constraints

- Layer order stays `route → service → repository → database` (from `CLAUDE.md`).
- No `sqlx` calls in handlers/services — repository layer only.
- All HTTP responses use the envelope `{ success, data, error }`; errors via `ApiError`.
- Schema changes only via `migrations/*.sql`; phase 1 introduces NO migration (uses existing `state_json jsonb`).
- Do not widen `allowed_capabilities` auth boundary — office scope and capability scope stay enforced.
- `AGENTS.md` "Ponytail Mode": smallest correct change; do not preemptively refactor unrelated code.
- Rust identifiers in English; prose comments in English (tracing structured fields also English).
- Commits via HEREDOC on `master` are OK for this repo (per session context); do NOT `--no-verify` or skip hooks.

## Phase scope

This plan implements sub-issues **04, 01, 06** from `docs/issues/retrieval-pipeline-rework/`. Phases 2 (issues 03, 02, 05) and phase 3 (issue 07) will get their own plan after this one lands and the baseline eval numbers are captured.

## File map

| Path | Change |
|---|---|
| `crates/chat/src/assistant/retrieval.rs` | Remove `domain_compatible` from filter (Task 1); refactor `retrieve` and add `shape_score` (Task 2); expose `plan_snapshot` helper for trace (Task 3). |
| `crates/chat/src/assistant/evidence.rs` | Adjust `MIN_SELECT_SCORE` if needed after new score distribution (Task 2 verification). |
| `crates/chat/src/assistant/runtime/mod.rs` | Build `retrieval_trace` JSON after retrieval and pass to persistence (Task 3). Existing `tracing::info!` calls stay. |
| `crates/chat/src/chat/repository/job.rs` | New method `merge_retrieval_trace(job_id, user_id, trace)` — pure jsonb merge, no revision bump (Task 3). |
| `crates/chat/tests/retrieval_scoring.rs` | NEW — unit + fixture tests for phase 1 (Tasks 1-3). |

---

## Task 1: Issue 04 — Drop redundant domain strict filter

**Files:**
- Modify: `crates/chat/src/assistant/retrieval.rs:120-137` (delete `domain_compatible` call and, if unused elsewhere, the function itself).
- Test: `crates/chat/tests/retrieval_scoring.rs` (new file — first test lives here).

**Interfaces:**
- Consumes: `compatible_ids(plan, catalog) -> Vec<String>` (existing).
- Produces: `compatible_ids` behavior identical except domain is no longer used to reject candidates. Signature unchanged.

- [ ] **Step 1: Confirm branch is not master**

```bash
git status
git rev-parse --abbrev-ref HEAD
```

If on `master`, create a feature branch first:
```bash
git checkout -b feat/retrieval-phase-1
```

- [ ] **Step 2: Create the test file with a failing test**

Create `crates/chat/tests/retrieval_scoring.rs`:

```rust
use chat::assistant::evidence::RetrievalPlan;
use chat::assistant::retrieval::compatible_ids;
use chat::assistant::{
    AssistantConstraints, AssistantDomain, AssistantEntity, AssistantIntent, AssistantIntentKind,
    AssistantLanguage, ContextReference, RequestGrouping, RequestOperation, RequestOutput,
    RequestPii, RequestShape, RequestSubject,
};
use chat::knowledge::model::{Capability, KnowledgeCatalog};

fn make_intent(domain: AssistantDomain, subject: RequestSubject) -> AssistantIntent {
    AssistantIntent {
        intent: AssistantIntentKind::ReportRequest,
        domain,
        request_shape: RequestShape {
            operation: RequestOperation::Rank,
            subject,
            grouping: RequestGrouping::None,
            output: RequestOutput::Ranking,
            pii: RequestPii::ClientIdentity,
        },
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: AssistantConstraints::default(),
        context_reference: ContextReference::None,
        source: None,
        confidence: 0.9,
        reason: "test".into(),
    }
}

fn make_capability(id: &str, domain: &str, subject: RequestSubject) -> Capability {
    Capability {
        id: id.into(),
        status: "approved_mvp".into(),
        domain: domain.into(),
        display_name: Some(id.into()),
        description: Some(format!("test capability {id}")),
        data_areas: vec![],
        required_api_capability: None,
        query_id: format!("{id}.query"),
        metrics: vec!["savings.account_count".into()],
        output_mode: "top_n".into(),
        request_shape: RequestShape {
            operation: RequestOperation::Rank,
            subject,
            grouping: RequestGrouping::None,
            output: RequestOutput::Ranking,
            pii: RequestPii::ClientIdentity,
        },
        examples: vec![],
        supported_intents: vec![],
        unsupported_intents: vec![],
        required_parameters: vec![],
        optional_parameters: vec![],
        defaults: Default::default(),
        guards: Default::default(),
        pii: Default::default(),
        cost_class: "low".into(),
        clarification: Default::default(),
        checks: vec![],
    }
}

fn catalog_with(capability: Capability) -> KnowledgeCatalog {
    KnowledgeCatalog {
        root_path: Default::default(),
        query_path: Default::default(),
        data_areas: vec![],
        domains: vec![],
        schemas: vec![],
        metrics: vec![],
        capabilities: vec![capability],
        queries: vec![],
        policies: vec![],
        responses: vec![],
        classification: Default::default(),
    }
}

#[test]
fn domain_mismatch_does_not_exclude_capability_when_subject_matches() {
    // Regression for issue 04: router misclassifies domain as Savings for
    // "top clients by savings account" queries while subject is correctly Client.
    // Previously this filtered out client_top_n_by_savings_account_count.
    let intent = make_intent(AssistantDomain::Savings, RequestSubject::Client);
    let plan = RetrievalPlan::new("top 3 clients by savings account", &intent, false, vec![
        "client_top_n_by_savings_account_count".to_string(),
    ]);
    let catalog = catalog_with(make_capability(
        "client_top_n_by_savings_account_count",
        "client",
        RequestSubject::Client,
    ));

    let compat = compatible_ids(&plan, &catalog);
    assert_eq!(
        compat,
        vec!["client_top_n_by_savings_account_count".to_string()],
        "capability with domain=client must survive when plan.domain=Savings and subject matches"
    );
}
```

- [ ] **Step 3: Run test — expect failure**

```bash
cargo test -p chat --test retrieval_scoring domain_mismatch_does_not_exclude_capability_when_subject_matches -- --nocapture
```

Expected: `assertion failed` — compat vector is empty because `domain_compatible` filters out capability with `domain=client` when plan says `Savings`.

- [ ] **Step 4: Delete `domain_compatible` from the filter chain**

Edit `crates/chat/src/assistant/retrieval.rs`. Find `compatible_ids` (~line 120):

```rust
pub fn compatible_ids(plan: &RetrievalPlan, catalog: &KnowledgeCatalog) -> Vec<String> {
    catalog
        .capabilities
        .iter()
        .filter(|cap| matches!(cap.status.as_str(), "approved_mvp" | "active"))
        .filter(|cap| plan.allow_all_capabilities || plan.allowed_capabilities.contains(&cap.id))
        .filter(|cap| domain_compatible(plan, &cap.domain))   // <-- REMOVE THIS LINE
        .filter(|cap| shape_compatible(&plan.request_shape, &cap.request_shape))
        .filter(|cap| metric_compatible(plan, &cap.metrics))
        .filter(|cap| parameters_feasible(plan, &cap.required_parameters))
        .map(|cap| cap.id.clone())
        .collect()
}
```

Remove the `.filter(|cap| domain_compatible(plan, &cap.domain))` line.

Then delete the `domain_compatible` function itself (lines 134-137) — it will be unused and cause a dead_code warning that `-D warnings` in CI treats as an error.

- [ ] **Step 5: Run test — expect pass**

```bash
cargo test -p chat --test retrieval_scoring domain_mismatch_does_not_exclude_capability_when_subject_matches -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Run full workspace check to catch collateral damage**

```bash
cargo check --workspace
cargo test -p chat --lib
```

Both must be clean. If any prior test asserted `domain_compatible` behavior, it will now fail — those tests were asserting the wrong behavior and should be updated to assert the new (relaxed) behavior with an inline comment referencing issue 04.

- [ ] **Step 7: Commit**

```bash
git add crates/chat/src/assistant/retrieval.rs crates/chat/tests/retrieval_scoring.rs
git commit -m "$(cat <<'EOF'
fix(retrieval): drop redundant domain strict filter (issue 04)

Subject already implies domain (client/office/savings_*). The separate
domain equality check produced false negatives when the router picked
domain by noun mention (e.g. "top clients by savings account" -> Savings)
while subject was correctly Client. See docs/issues/retrieval-pipeline-rework/04-drop-domain-filter.md.
EOF
)"
```

---

## Task 2: Issue 01 — Invert retrieval, shape as score

**Files:**
- Modify: `crates/chat/src/assistant/retrieval.rs` — refactor `retrieve`, add `shape_score`, keep `compatible_ids` (used by trace).
- Modify: `crates/chat/src/assistant/evidence.rs:115` — keep `MIN_SELECT_SCORE = 0.25`; verify still appropriate.
- Test: `crates/chat/tests/retrieval_scoring.rs` — add scoring assertions.

**Interfaces:**
- Consumes: existing `RetrievalPlan`, `Evidence`, `KnowledgeRepository::search_hybrid_by_source_type`, `KnowledgeCatalog`.
- Produces: `RetrievalEngine::retrieve` never returns `Ok(vec![])` when catalog is non-empty and any embedding lookup succeeds. Signature unchanged. New public helper `shape_score(plan: &RetrievalPlan, cap: &Capability) -> f32` returning `[0.0, 1.0]`.

- [ ] **Step 1: Write the failing scoring test**

Append to `crates/chat/tests/retrieval_scoring.rs`:

```rust
use chat::assistant::retrieval::shape_score;

#[test]
fn shape_score_ranks_full_match_over_partial_match() {
    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let plan = RetrievalPlan::new("top clients", &intent, false, vec![]);

    let full = make_capability("full", "client", RequestSubject::Client);
    let mut partial = make_capability("partial", "client", RequestSubject::Office);
    // partial mismatches subject only

    let full_score = shape_score(&plan, &full);
    let partial_score = shape_score(&plan, &partial);

    assert!(full_score > partial_score, "full={full_score} partial={partial_score}");
    assert!((0.0..=1.0).contains(&full_score));
    assert!((0.0..=1.0).contains(&partial_score));
}

#[test]
fn retrieve_returns_candidates_when_no_shape_matches_but_catalog_non_empty() {
    // Regression for issue 01: previously an empty compatible_ids collapsed
    // the entire pipeline. Now retrieve must still surface catalog_fallback
    // candidates, letting downstream (reranker / evaluator) decide.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Organization, RequestSubject::Office);
    let mut shape = intent.request_shape.clone();
    shape.operation = RequestOperation::RandomSample;
    let mut intent = intent;
    intent.request_shape = shape;

    let plan = RetrievalPlan::new("berikan 3 office", &intent, false, vec![
        "organization_office_summary".to_string(),
    ]);
    let mut cap = make_capability("organization_office_summary", "organization", RequestSubject::Office);
    cap.request_shape.operation = RequestOperation::Summary;
    let catalog = catalog_with(cap);
    let catalog = std::sync::Arc::new(catalog);

    let evidence = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            RetrievalEngine::retrieve(&plan, None, None, Some(&catalog)).await
        })
        .expect("retrieve should not error");

    assert!(
        !evidence.is_empty(),
        "shape mismatch alone must not collapse retrieval to empty"
    );
    assert_eq!(evidence[0].capability_id, "organization_office_summary");
}
```

- [ ] **Step 2: Run tests — expect failure**

```bash
cargo test -p chat --test retrieval_scoring -- --nocapture
```

Expected: `shape_score` test fails to compile (unknown fn); `retrieve_returns_candidates` fails because current `retrieve` returns `[]` when `compatible_ids` is empty.

- [ ] **Step 3: Add `shape_score` helper in retrieval.rs**

Add near the top of the `pub` section of `crates/chat/src/assistant/retrieval.rs`:

```rust
/// Score in [0.0, 1.0] measuring how many request_shape dimensions match.
/// 5 dimensions weighted equally; each match contributes 0.2. PII match
/// includes the ClientIdentity -> ConditionalClientIdentity relaxation
/// already used by `pii_compatible`.
pub fn shape_score(plan: &RetrievalPlan, capability: &crate::knowledge::model::Capability) -> f32 {
    let request = &plan.request_shape;
    let cap = &capability.request_shape;
    let mut hits = 0u8;
    if enum_compatible(&request.operation, &cap.operation, &RequestOperation::Unknown) { hits += 1; }
    if enum_compatible(&request.subject,   &cap.subject,   &RequestSubject::Unknown)   { hits += 1; }
    if enum_compatible(&request.grouping,  &cap.grouping,  &RequestGrouping::Unknown)  { hits += 1; }
    if enum_compatible(&request.output,    &cap.output,    &RequestOutput::Unknown)    { hits += 1; }
    if pii_compatible(&request.pii, &cap.pii) { hits += 1; }
    (hits as f32) / 5.0
}
```

Also make sure `RequestOperation`, `RequestSubject`, `RequestGrouping`, `RequestOutput` are in scope at the top of the file (already imported per current source).

- [ ] **Step 4: Refactor `RetrievalEngine::retrieve` to semantic-first**

Replace the body of `RetrievalEngine::retrieve` in `crates/chat/src/assistant/retrieval.rs`:

```rust
impl RetrievalEngine {
    pub async fn retrieve(
        plan: &RetrievalPlan,
        llm: Option<&SharedLlmClient>,
        knowledge: Option<&KnowledgeRepository>,
        catalog: Option<&Arc<KnowledgeCatalog>>,
    ) -> Result<Vec<Evidence>> {
        // Auth boundary: restrict to caller's allowed_capabilities.
        // Catalog-wide search is NOT the same as widening auth.
        let search_ids = allowed_ids(plan);

        let mut evidence: Vec<Evidence> = Vec::new();

        if let (Some(llm), Some(knowledge)) = (llm, knowledge) {
            let embedding = llm
                .embed(
                    crate::assistant::llm::LlmPurpose::EvidenceRetrieval,
                    &plan.query_text,
                )
                .await?
                .vector;
            evidence.extend(
                knowledge
                    .search_hybrid_by_source_type(
                        "capability",
                        embedding,
                        &keyword_terms(&plan.query_text),
                        search_ids.as_deref(),
                        &plan.metadata_filters,
                        16,
                    )
                    .await?
                    .into_iter()
                    .map(Evidence::from),
            );
        }

        if let Some(catalog) = catalog {
            evidence.extend(catalog_fallback(plan, catalog));
        }

        // Boost each candidate by shape match against the plan (up to +0.30).
        if let Some(catalog) = catalog {
            let shape_boost = 0.30;
            for item in evidence.iter_mut() {
                if let Some(cap) = catalog.capabilities.iter().find(|c| c.id == item.capability_id) {
                    let score = shape_score(plan, cap);
                    item.score = (item.score + score * shape_boost).min(0.99);
                }
            }
        }

        Ok(merge(evidence))
    }
}
```

Notes:
- Deleted the early `return Ok(Vec::new())` gate.
- Deleted the final `evidence.retain(...)` gate.
- Kept `allowed_ids(plan)` for the auth boundary (unchanged behavior for `search_hybrid_by_source_type`).
- Increased `search_hybrid_by_source_type` limit from 8 to 16 to give the reranker (issue 02) more headroom later.
- `catalog_fallback` and `merge` reused as-is.

- [ ] **Step 5: Update `catalog_fallback` to no longer require domain match**

In the same file, in `catalog_fallback`, remove the `metadata_filters.get("domain")` filter (~line 93):

```rust
    catalog
        .capabilities
        .iter()
        .filter(|cap| matches!(cap.status.as_str(), "approved_mvp" | "active"))
        .filter(|cap| plan.allow_all_capabilities || plan.allowed_capabilities.iter().any(|id| id == &cap.id))
        // domain filter removed — subject/shape/reranker handle relevance
        .map(|cap| { ... existing scoring ... })
```

(Keep the rest of `catalog_fallback` identical. Removing only the domain filter.)

- [ ] **Step 6: Run tests**

```bash
cargo test -p chat --test retrieval_scoring -- --nocapture
cargo test -p chat --lib
cargo test -p chat --test assistant_retrieval_evidence
```

Expected: all three new tests pass. Existing `assistant_retrieval_evidence.rs` may need a threshold nudge if a test asserted `evidence.is_empty()` under a scenario that now returns catalog_fallback rows — inspect any failure and update the assertion to reflect the new (correct) behavior.

- [ ] **Step 7: Verify the three failing production queries by unit fixture**

Add to `crates/chat/tests/retrieval_scoring.rs`:

```rust
#[test]
fn top_n_by_savings_account_count_selected_for_rank_query() {
    // Query from prod log 2026-07-17: "3 clients where have the most savings account for this year"
    let intent = make_intent(AssistantDomain::Savings, RequestSubject::Client); // domain misclassified — must not matter
    let plan = RetrievalPlan::new(
        "3 clients where have the most savings account for this year",
        &intent,
        false,
        vec!["client_top_n_by_savings_account_count".to_string(), "savings_deposit_total".to_string()],
    );
    let mut target = make_capability("client_top_n_by_savings_account_count", "client", RequestSubject::Client);
    target.description = Some("Top clients by number of active savings accounts".into());
    let mut distractor = make_capability("savings_deposit_total", "savings", RequestSubject::SavingsTransaction);
    distractor.request_shape.operation = RequestOperation::Total;
    distractor.request_shape.output = RequestOutput::Scalar;

    let catalog = std::sync::Arc::new(KnowledgeCatalog {
        capabilities: vec![target, distractor],
        ..catalog_with(make_capability("_", "_", RequestSubject::Client))
    });
    let evidence = tokio::runtime::Runtime::new().unwrap().block_on(async {
        chat::assistant::retrieval::RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
            .await
            .unwrap()
    });
    assert_eq!(evidence[0].capability_id, "client_top_n_by_savings_account_count");
}
```

- [ ] **Step 8: Run the new fixture test**

```bash
cargo test -p chat --test retrieval_scoring top_n_by_savings_account_count_selected_for_rank_query -- --nocapture
```

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/chat/src/assistant/retrieval.rs crates/chat/tests/retrieval_scoring.rs
git commit -m "$(cat <<'EOF'
refactor(retrieval): invert to semantic-first, shape as score (issue 01)

- Embedding hybrid search now always runs over caller's allowed_capabilities.
- shape_score(plan, capability) -> f32 replaces the strict enum-equality gate;
  applied as an additive boost (up to +0.30) on top of cosine + hits.
- Removed the empty-compatible_ids early return and final retain filter.
- Removed the domain metadata_filter in catalog_fallback (subject covers domain).
- Kept allowed_capabilities as the auth boundary; no widening of caller visibility.

See docs/issues/retrieval-pipeline-rework/01-semantic-first-retrieval.md.
EOF
)"
```

---

## Task 3: Issue 06 — Persist retrieval trace to `state_json`

**Files:**
- Modify: `crates/chat/src/chat/repository/job.rs` — add `merge_retrieval_trace` method.
- Modify: `crates/chat/src/assistant/runtime/mod.rs` — build trace JSON after retrieval, pass to persistence via existing pipeline plumbing.
- Test: `crates/chat/tests/retrieval_scoring.rs` — assert trace shape via a unit test on the builder helper.

**Interfaces:**
- Consumes: `JobRepository` (already injected into `JobService`), existing `state_json jsonb` column, existing `RetrievalPlan`, `Evidence`, `EvidenceDecision` serialization.
- Produces: `JobRepository::merge_retrieval_trace(job_id, user_id, trace_json)` — pure jsonb merge, no revision bump. New pub fn `build_retrieval_trace(intent, plan, evidence, decision) -> serde_json::Value` in `runtime/mod.rs`.

- [ ] **Step 1: Write the trace-shape unit test**

Append to `crates/chat/tests/retrieval_scoring.rs`:

```rust
#[test]
fn build_retrieval_trace_emits_expected_top_level_keys() {
    use chat::assistant::evidence::{Evidence, EvidenceDecision};
    use chat::assistant::runtime::build_retrieval_trace;

    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let plan = RetrievalPlan::new("top 3 clients", &intent, false, vec!["capability_a".into()]);
    let evidence = vec![Evidence {
        capability_id: "capability_a".into(),
        title: "Cap A".into(),
        score: 0.82,
        source_type: "capability".into(),
        metadata: serde_json::json!({}),
        conflicting: false,
    }];
    let decision = EvidenceDecision::Select { capability_id: "capability_a".into() };

    let trace = build_retrieval_trace(&intent, &plan, &evidence, &decision);

    let obj = trace.as_object().expect("trace must be a JSON object");
    for key in ["router_intent", "plan", "candidates", "decision"] {
        assert!(obj.contains_key(key), "missing key {key}");
    }
    let candidates = trace["candidates"].as_array().expect("candidates array");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["capability_id"], "capability_a");
    assert_eq!(trace["decision"]["kind"], "select");
    assert_eq!(trace["decision"]["capability_id"], "capability_a");
}
```

- [ ] **Step 2: Run test — expect compilation failure**

```bash
cargo test -p chat --test retrieval_scoring build_retrieval_trace_emits_expected_top_level_keys -- --nocapture
```

Expected: `build_retrieval_trace` unresolved.

- [ ] **Step 3: Implement `build_retrieval_trace` in runtime/mod.rs**

Add near the bottom of `crates/chat/src/assistant/runtime/mod.rs` (before the `#[cfg(test)]` block):

```rust
pub fn build_retrieval_trace(
    intent: &AssistantIntent,
    plan: &crate::assistant::evidence::RetrievalPlan,
    evidence: &[crate::assistant::evidence::Evidence],
    decision: &crate::assistant::evidence::EvidenceDecision,
) -> serde_json::Value {
    use crate::assistant::evidence::EvidenceDecision;

    let candidates: Vec<_> = evidence
        .iter()
        .take(10)
        .map(|e| serde_json::json!({
            "capability_id": e.capability_id,
            "title": e.title,
            "score": e.score,
            "source_type": e.source_type,
        }))
        .collect();

    let decision_json = match decision {
        EvidenceDecision::Select { capability_id } => serde_json::json!({
            "kind": "select",
            "capability_id": capability_id,
        }),
        EvidenceDecision::Clarify => serde_json::json!({ "kind": "clarify" }),
        EvidenceDecision::UnsupportedInDomain => serde_json::json!({ "kind": "unsupported_in_domain" }),
        EvidenceDecision::OutOfDomain => serde_json::json!({ "kind": "out_of_domain" }),
        EvidenceDecision::BlockedByPolicy => serde_json::json!({ "kind": "blocked_by_policy" }),
    };

    serde_json::json!({
        "router_intent": {
            "intent": intent.intent,
            "domain": intent.domain,
            "request_shape": intent.request_shape,
            "confidence": intent.confidence,
        },
        "plan": {
            "query_text": plan.query_text,
            "allowed_capability_count": plan.allowed_capabilities.len(),
            "allow_all_capabilities": plan.allow_all_capabilities,
        },
        "candidates": candidates,
        "decision": decision_json,
    })
}
```

- [ ] **Step 4: Run the trace test — expect pass**

```bash
cargo test -p chat --test retrieval_scoring build_retrieval_trace_emits_expected_top_level_keys -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Add repository method to merge trace into state_json**

In `crates/chat/src/chat/repository/job.rs`, add a new method to `impl JobRepository`:

```rust
pub async fn merge_retrieval_trace(
    &self,
    job_id: Uuid,
    user_id: Uuid,
    trace: serde_json::Value,
) -> Result<()> {
    let patch = serde_json::json!({ "retrieval_trace": trace });
    sqlx::query(
        r#"
        UPDATE chat_jobs
        SET state_json = state_json || $1::jsonb,
            updated_at = now()
        WHERE id = $2 AND user_id = $3
        "#,
    )
    .bind(patch)
    .bind(job_id)
    .bind(user_id)
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

Note: no `state_revision` bump. Trace is an audit-only append; it must not race the classification/plan/policy pipeline which owns the revision counter.

- [ ] **Step 6: Wire the trace write into the runtime path**

In `crates/chat/src/assistant/runtime/mod.rs`, near the existing tracing logs after the decision is computed (currently around the `evidence decision` `tracing::info!` block), locate the persistence path that already has `job_id` and `user_id` in scope. Find the closest `JobRepository` accessor — this is passed via the higher-level pipeline. If `runtime::run` does NOT have `JobRepository`, we thread the trace up as part of `GraphResult`:

Add a new field to whatever result struct is returned to the pipeline (search for `GraphResult` or the `graph_result` helper):

```bash
grep -n "graph_result\|GraphResult" crates/chat/src/assistant/runtime/mod.rs | head -10
```

Then, after the retrieval decision, call `build_retrieval_trace` and attach the value to the memory JSON — reuse the existing `memory.warnings` / `memory.retrieval_evidence` persistence path. Search for where `chat_job_memory` writes happen; add a new field there, OR (simpler for phase 1) route the trace through `JobService` which already has `JobRepository`.

Concretely: the `execute_report_pipeline` (or equivalent — grep will tell you the exact name) that owns `job_id + user_id + JobRepository` — call:

```rust
job_repository
    .merge_retrieval_trace(job_id, user_id, build_retrieval_trace(&intent, &plan, &evidence, &decision))
    .await
    .ok(); // best-effort; do not fail the request on trace write
```

immediately after `EvidenceEvaluator.evaluate(...)` in the current runtime flow.

- [ ] **Step 7: Integration smoke test**

Replay one of the three production queries via curl (server already running per session context):

```bash
curl -s -X POST http://127.0.0.1:3007/chat/jobs \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $API_KEY" \
  -d '{"message":"give me 3 clients where have the most savings account for this year"}' | jq '.data.id'
```

Then fetch:

```bash
curl -s http://127.0.0.1:3007/chat/jobs/<id> -H "X-API-Key: $API_KEY" | jq '.data.state_json.retrieval_trace'
```

Expected: JSON object with `router_intent`, `plan`, `candidates`, `decision`. Candidates non-empty. Decision kind `select` (given Tasks 1 & 2 already landed).

- [ ] **Step 8: Run full workspace tests**

```bash
cargo test --workspace --no-fail-fast
```

Expected: green. Any pre-existing flake unrelated to this plan is out of scope — note it and continue.

- [ ] **Step 9: Commit**

```bash
git add crates/chat/src/assistant/runtime/mod.rs crates/chat/src/chat/repository/job.rs crates/chat/tests/retrieval_scoring.rs
git commit -m "$(cat <<'EOF'
feat(assistant): persist retrieval trace to chat_jobs.state_json (issue 06)

Emits { router_intent, plan, candidates[<=10], decision } under
state_json.retrieval_trace after every retrieval pass. Best-effort write
(does not fail the request). No schema change — existing jsonb column,
no state_revision bump.

Debugging the next unsupported response is now a single GET /chat/jobs/{id}.
See docs/issues/retrieval-pipeline-rework/06-retrieval-trace-observability.md.
EOF
)"
```

---

## Self-review checklist (do this before handing off)

- [ ] All three sub-issues (04, 01, 06) covered? Yes — Tasks 1, 2, 3.
- [ ] Placeholder scan: any "TBD", "TODO", "add appropriate...", "similar to Task N"? Grep the plan and remove.
- [ ] Type consistency: `shape_score`, `build_retrieval_trace`, `merge_retrieval_trace` — names match between tasks and between plan and referenced source? Yes.
- [ ] Auth boundary preserved? Yes — `allowed_ids(plan)` still used in embedding search; `allowed_capabilities` still filters catalog_fallback.
- [ ] Migration needed? No.
- [ ] New workspace deps? No.

## Out of scope for phase 1

- LLM reranker (issue 02) — needs its own plan after phase 1 lands.
- Catalog browse primitives (issue 03).
- Schemars structured contracts (issue 05).
- Eval harness (issue 07).

## Phase 2 preview (not part of this plan)

After phase 1 merges to `master` and staging shows retrieval_trace populated correctly for 24h, write phase 2 plan covering issues 03 + 02 + 05. Phase 3 (issue 07) after phase 2.
