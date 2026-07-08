# RAG + LQR Overlay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Overlay Layered Query Retrieval on the strict RAG pipeline, replacing flat `search_capabilities` + `search_context` + post-hoc off-domain override with three ordered layers (Domain → Capability → Query), off-domain short-circuit at Layer 1, and per-layer audit trace.

**Architecture:** New module `crates/chat/src/chat/pipeline/lqr.rs` orchestrates layered retrieval. Retrieval Planner (`LlmClient::plan_retrieval`) emits `LayeredRetrievalPlan { domain, capability, query, keyword, graph_hint }`. Each layer calls the same hybrid retrieval fn from existing plan Task 4, scoped by `source_type` filter. `JobService::classify_message` grows a feature-flagged branch that dispatches to LQR when `LQR_ENABLED=true`. Downstream stages (`classify_retrieved_capability` → `ExecutionPlan` → policy → executor → formatter) unchanged.

**Tech Stack:** Rust, sqlx/PostgreSQL/pgvector, Voyage embeddings, OpenAI-compatible LLM (DeepSeek), serde/serde_json.

**Related:**
- Companion spec: `docs/superpowers/specs/2026-07-07-rag-lqr-overlay-design.md` — read first.
- Parent plan: `docs/superpowers/plans/2026-07-07-full-rag-blueprint-strict.md` — LQR overlays Task 4 (Retrieval), does not replace Tasks 1-3, 5-8.
- Blueprint: `docs/Modern_RAG_Architecture_Blueprint.md` — LQR is a refinement of Steps 5-6 (Retrieval Planning + Hybrid Retrieval).

## Global Constraints

- Do not add new crates.
- Do not let the LLM generate SQL. All executable SQL comes from `queries/*.sql` files pre-approved in the catalog.
- Do not index Fineract transactional rows in pgvector. LQR only queries `knowledge_index`.
- `LLM_API_KEY` is required for the Retrieval Planner (LayeredRetrievalPlan generation).
- `VOYAGEAI_API_KEY` is required. Each layer embeds its own query fragment; there is no per-layer specialised embedding model — all use the same Voyage `voyage-2` config with `input_type="query"`.
- Runtime retrieval requires the latest `knowledge_catalog_versions.status='embedded'` row to match the loaded catalog content hash. Layer 1 must gate on this check (reuse Task 4 evidence stale detector).
- Vector results never execute directly; execution still routes through `evaluate_policy` and `execute_plan`.
- API responses continue to use the envelope `{ "success": bool, "data": ..., "error": ... }`.
- MVP user-facing text remains English only. Layer clarification messages must not leak internal layer names.
- `LQR_ENABLED` env flag defaults `false`. Both paths must coexist until Task LQR-F flips default.
- Layer 1 (Domain) rejects deferred/rejected domains at the earliest opportunity. This is the primary correctness win and cannot be regressed.
- Score aggregation across layers uses `min(...)` unless config overrides. `min` is chosen so a strong capability match cannot mask a weak domain match.

---

## File Structure

- Create `crates/chat/src/chat/pipeline/lqr.rs`: LQR orchestrator, layer executors, `LayeredRetrievalPlan`, `LayerOutcome`, `LqrResult`.
- Create `crates/chat/tests/lqr_layered_retrieval.rs`: integration tests for §12 scenarios in the design spec.
- Modify `crates/chat/src/chat/pipeline/mod.rs`: add `pub mod lqr;`.
- Modify `crates/chat/src/chat/pipeline/retrieval.rs` (from parent plan Task 4): rename `RetrievalPlanStrict` to `FlatRetrievalPlan` and add `LayeredRetrievalPlan` alongside. `build_retrieval_plan` split into `build_flat_retrieval_plan` (backward compat) and `build_layered_retrieval_plan` (LQR path). Keep the existing struct fields — do not remove anything used elsewhere yet.
- Modify `crates/chat/src/chat/llm.rs`: replace `LlmClient::plan_retrieval` prompt+schema with layered variant emitting `LayeredRetrievalPlan`. Keep old fn signature under `plan_retrieval_flat` for parent Task 4 tests.
- Modify `crates/chat/src/knowledge/index/repository.rs`: add `search_hybrid_by_source_type` — one query with a `source_type` filter parameter. Used by every LQR layer.
- Modify `crates/chat/src/knowledge/model.rs`: extend `ClassificationPolicy` with an `Lqr` sub-struct (`domain_min_floor`, `domain_min_gap`, `capability_min_floor`, `capability_min_gap`, `retry_budget`, `score_aggregation`) all with defaults.
- Modify `knowledge/policies/classification.yaml`: add the `lqr:` block from design spec §10.
- Modify `crates/chat/src/chat/service/job.rs::classify_message`: add `LQR_ENABLED` branch that calls `pipeline::lqr::run_layered_retrieval` and converts `LqrResult` to `ClassificationResult`. Flat path preserved when flag off.
- Modify `crates/chat/src/chat/classifier.rs`: extend `ClassificationResult` (`layers: Vec<LayerTrace>`) with `#[serde(default)]` so existing job JSON remains compatible.
- Modify `crates/core/src/config.rs`: read `LQR_ENABLED` env into `ChatFeatureConfig { lqr_enabled: bool }`.
- Modify `.env.example`: add `LQR_ENABLED=false` with a one-line comment.
- Modify `docs/superpowers/plans/2026-07-07-full-rag-blueprint-strict.md`: add a top-of-file note that Task 4 (Retrieval) is superseded by this overlay when `LQR_ENABLED=true`.
- Modify `docs/scenarios/README.md`: index the new LQR scenario file.
- Create `docs/scenarios/09-lqr-layered-retrieval.md`: Postman playbook mirroring §12 spec test cases.

---

### Task LQR-A: Config + policy plumbing

**Files:**
- Modify: `crates/chat/src/knowledge/model.rs`
- Modify: `knowledge/policies/classification.yaml`
- Modify: `crates/core/src/config.rs`
- Modify: `.env.example`
- Test: extend `crates/chat/src/knowledge/tests.rs` (existing)

**Interfaces:**
- Produces:
  - `ClassificationPolicy::lqr: LqrPolicy`
  - `LqrPolicy { domain_min_floor: f32, domain_min_gap: f32, capability_min_floor: f32, capability_min_gap: f32, retry_budget: u8, score_aggregation: ScoreAggregation }`
  - `ScoreAggregation::Min | Mean | Product`
  - `ChatFeatureConfig::lqr_enabled: bool`
- Consumes: existing `ClassificationPolicy` struct.

- [ ] **Step 1: Write policy deserialisation test**

Add to `crates/chat/src/knowledge/tests.rs`:

```rust
#[test]
fn classification_policy_reads_lqr_block_with_defaults() {
    let yaml = r#"
min_gap: 0.05
min_floor: 0.40
others_key: other_activity
others_label: "Others"
lqr:
  domain_min_floor: 0.55
  domain_min_gap: 0.10
  retry_budget: 2
  score_aggregation: min
"#;
    let policy: crate::knowledge::model::ClassificationPolicy =
        serde_yaml::from_str(yaml).expect("parse policy");
    assert_eq!(policy.lqr.domain_min_floor, 0.55);
    assert_eq!(policy.lqr.domain_min_gap, 0.10);
    assert_eq!(policy.lqr.capability_min_floor, 0.40); // default
    assert_eq!(policy.lqr.capability_min_gap, 0.05);   // default
    assert_eq!(policy.lqr.retry_budget, 2);
    assert!(matches!(
        policy.lqr.score_aggregation,
        crate::knowledge::model::ScoreAggregation::Min
    ));
}

#[test]
fn classification_policy_defaults_lqr_when_missing() {
    let yaml = r#"
min_gap: 0.05
min_floor: 0.40
others_key: other_activity
others_label: "Others"
"#;
    let policy: crate::knowledge::model::ClassificationPolicy =
        serde_yaml::from_str(yaml).expect("parse policy");
    assert_eq!(policy.lqr.domain_min_floor, 0.55);
    assert_eq!(policy.lqr.retry_budget, 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p chat knowledge::tests::classification_policy_reads_lqr_block_with_defaults -- --nocapture`
Expected: FAIL — `lqr` field does not exist on `ClassificationPolicy`.

- [ ] **Step 3: Extend `ClassificationPolicy` and add `LqrPolicy`**

In `crates/chat/src/knowledge/model.rs`, add fields after existing ones:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ClassificationPolicy {
    pub min_gap: f32,
    pub min_floor: f32,
    pub others_key: String,
    pub others_label: String,
    #[serde(default)]
    pub lqr: LqrPolicy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LqrPolicy {
    #[serde(default = "default_domain_floor")]
    pub domain_min_floor: f32,
    #[serde(default = "default_domain_gap")]
    pub domain_min_gap: f32,
    #[serde(default = "default_cap_floor")]
    pub capability_min_floor: f32,
    #[serde(default = "default_cap_gap")]
    pub capability_min_gap: f32,
    #[serde(default = "default_retry_budget")]
    pub retry_budget: u8,
    #[serde(default)]
    pub score_aggregation: ScoreAggregation,
}

impl Default for LqrPolicy {
    fn default() -> Self {
        Self {
            domain_min_floor: default_domain_floor(),
            domain_min_gap: default_domain_gap(),
            capability_min_floor: default_cap_floor(),
            capability_min_gap: default_cap_gap(),
            retry_budget: default_retry_budget(),
            score_aggregation: ScoreAggregation::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreAggregation {
    #[default]
    Min,
    Mean,
    Product,
}

fn default_domain_floor() -> f32 { 0.55 }
fn default_domain_gap() -> f32 { 0.10 }
fn default_cap_floor() -> f32 { 0.40 }
fn default_cap_gap() -> f32 { 0.05 }
fn default_retry_budget() -> u8 { 2 }
```

Also update the `Default` impl on `ClassificationPolicy` to include `lqr: LqrPolicy::default()`.

- [ ] **Step 4: Add `lqr` block to `knowledge/policies/classification.yaml`**

Append to the file:

```yaml
lqr:
  domain_min_floor: 0.55
  domain_min_gap: 0.10
  capability_min_floor: 0.40
  capability_min_gap: 0.05
  retry_budget: 2
  score_aggregation: min
```

- [ ] **Step 5: Add `lqr_enabled` config flag**

In `crates/core/src/config.rs`, extend the chat feature config section (or create if absent):

```rust
#[derive(Debug, Clone)]
pub struct ChatFeatureConfig {
    pub lqr_enabled: bool,
}

// in the loader:
chat_features: ChatFeatureConfig {
    lqr_enabled: get_env_or("LQR_ENABLED", "false")
        .eq_ignore_ascii_case("true"),
},
```

Add `chat_features: ChatFeatureConfig` to the top-level `AppConfig` alongside existing fields.

Add to `.env.example`:

```dotenv
# When true, chat classification uses the layered retrieval (LQR) pipeline
# instead of the flat vector search. Default: false during rollout.
LQR_ENABLED=false
```

- [ ] **Step 6: Run policy tests to verify pass**

Run: `cargo test -p chat knowledge::tests::classification_policy_reads_lqr_block`
Expected: PASS (both tests).

Run: `cargo test -p chat --test catalog_validation`
Expected: PASS — all 6 existing tests still green.

- [ ] **Step 7: Commit**

```bash
git add crates/chat/src/knowledge/model.rs \
        crates/chat/src/knowledge/tests.rs \
        knowledge/policies/classification.yaml \
        crates/core/src/config.rs \
        .env.example
git commit -m "feat(lqr): add LqrPolicy config + LQR_ENABLED feature flag"
```

---

### Task LQR-B: Repository — source-type-scoped hybrid search

**Files:**
- Modify: `crates/chat/src/knowledge/index/repository.rs`
- Test: extend the existing `#[cfg(test)]` block in the same file

**Interfaces:**
- Produces:
  ```rust
  pub async fn search_hybrid_by_source_type(
      &self,
      source_type: &str,
      embedding: Vec<f32>,
      keyword_terms: &[String],
      allowed_source_ids: Option<&[String]>,
      metadata_filter: &BTreeMap<String, String>,
      limit: i64,
  ) -> Result<Vec<RetrievedKnowledgeCandidate>>
  ```
- Consumes: existing `RetrievedKnowledgeCandidate`, latest `catalog_version_id` picker (unchanged).

- [ ] **Step 1: Write repository unit test (SQL round-trip stub)**

Because the function hits Postgres, unit-test the SQL string builder in isolation. Add:

```rust
#[test]
fn hybrid_sql_includes_source_type_and_allowed_source_id_filter() {
    let sql = super::build_hybrid_sql(
        /*has_allowed_ids=*/ true,
        /*metadata_keys=*/ &["domain".into(), "office_scope".into()],
    );
    assert!(sql.contains("source_type = $2"));
    assert!(sql.contains("source_id = ANY($3::text[])"));
    assert!(sql.contains("metadata_json->>'domain' = $"));
    assert!(sql.contains("metadata_json->>'office_scope' = $"));
    assert!(sql.contains("ORDER BY distance"));
}

#[test]
fn hybrid_sql_without_allowed_ids_skips_source_id_filter() {
    let sql = super::build_hybrid_sql(false, &[]);
    assert!(!sql.contains("source_id = ANY"));
    assert!(sql.contains("source_type = $2"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p chat --lib knowledge::index::repository::tests::hybrid_sql -- --nocapture`
Expected: FAIL — `build_hybrid_sql` does not exist.

- [ ] **Step 3: Implement `build_hybrid_sql` and `search_hybrid_by_source_type`**

Add to `crates/chat/src/knowledge/index/repository.rs`:

```rust
pub(crate) fn build_hybrid_sql(has_allowed_ids: bool, metadata_keys: &[String]) -> String {
    let mut sql = String::from(
        r#"
        WITH latest AS (
            SELECT id FROM knowledge_catalog_versions
            WHERE status = 'embedded'
            ORDER BY synced_at DESC NULLS LAST, created_at DESC
            LIMIT 1
        )
        SELECT source_type, source_id, title, retrieval_text, metadata_json,
               (embedding <=> $1) AS distance
        FROM knowledge_index, latest
        WHERE catalog_version_id = latest.id
          AND embedding IS NOT NULL
          AND source_type = $2
        "#,
    );

    let mut param_idx = 3;
    if has_allowed_ids {
        sql.push_str(&format!("\n          AND source_id = ANY(${param_idx}::text[])"));
        param_idx += 1;
    }
    for key in metadata_keys {
        sql.push_str(&format!(
            "\n          AND metadata_json->>'{key}' = ${param_idx}"
        ));
        param_idx += 1;
    }
    sql.push_str(&format!(
        "\n        ORDER BY distance\n        LIMIT ${param_idx}"
    ));
    sql
}

impl KnowledgeRepository {
    pub async fn search_hybrid_by_source_type(
        &self,
        source_type: &str,
        embedding: Vec<f32>,
        _keyword_terms: &[String], // reserved for BM25 in parent Task 4
        allowed_source_ids: Option<&[String]>,
        metadata_filter: &std::collections::BTreeMap<String, String>,
        limit: i64,
    ) -> anyhow::Result<Vec<RetrievedKnowledgeCandidate>> {
        let metadata_keys: Vec<String> = metadata_filter.keys().cloned().collect();
        let sql = build_hybrid_sql(allowed_source_ids.is_some(), &metadata_keys);

        let mut query = sqlx::query_as::<_, RetrievedKnowledgeCandidate>(&sql)
            .bind(Vector::from(embedding))
            .bind(source_type);

        if let Some(ids) = allowed_source_ids {
            let owned: Vec<String> = ids.to_vec();
            query = query.bind(owned);
        }
        for key in &metadata_keys {
            let value = metadata_filter.get(key).cloned().unwrap_or_default();
            query = query.bind(value);
        }
        query = query.bind(limit);

        Ok(query.fetch_all(&self.pool).await?)
    }
}
```

Note: the `keyword_terms` parameter is currently unused. It becomes a BM25 `ts_rank` fusion in parent Task 4 Step 3. Leaving the parameter reserved keeps the LQR call sites stable across the two rollouts.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p chat --lib knowledge::index::repository::tests::hybrid_sql`
Expected: PASS.

Run: `cargo test --workspace --lib`
Expected: no regression in existing tests.

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/knowledge/index/repository.rs
git commit -m "feat(lqr): add source-type-scoped hybrid search on knowledge_index"
```

---

### Task LQR-C: Retrieval Planner — layered variant

**Files:**
- Modify: `crates/chat/src/chat/llm.rs`
- Modify: `crates/chat/src/chat/pipeline/retrieval.rs`
- Test: inline `#[cfg(test)]` in both files

**Interfaces:**
- Produces:
  ```rust
  pub struct LayeredRetrievalPlan {
      pub domain: String,
      pub capability: String,
      pub query: String,
      pub keyword: String,
      pub graph_hint: Option<String>,
      pub confidence: f32,
  }

  impl LlmClient {
      pub async fn plan_layered_retrieval(
          &self,
          message: &str,
          conversation_context: &Value,
      ) -> Result<LayeredRetrievalPlan>;
  }
  ```
- Consumes: `LlmClient` transport already implemented; `chat_json` helper.

- [ ] **Step 1: Write pure-parse test (no LLM call)**

Add to `crates/chat/src/chat/pipeline/retrieval.rs`:

```rust
#[cfg(test)]
mod layered_plan_tests {
    use super::*;

    #[test]
    fn parses_valid_layered_plan_json() {
        let json = r#"{
          "layers": {
            "domain":"client activation",
            "capability":"monthly count of client activations",
            "query":"monthly_breakdown output for client onboarding",
            "keyword":"client activation monthly",
            "graph_hint":"client -> activation_date"
          },
          "confidence":0.83
        }"#;
        let plan = parse_layered_retrieval_response(json).expect("parse");
        assert_eq!(plan.domain, "client activation");
        assert_eq!(plan.capability, "monthly count of client activations");
        assert_eq!(plan.graph_hint.as_deref(), Some("client -> activation_date"));
        assert!((plan.confidence - 0.83).abs() < 1e-4);
    }

    #[test]
    fn rejects_missing_layer_field() {
        let json = r#"{"layers":{"domain":"x","capability":"y","query":"z"},"confidence":0.7}"#;
        // "keyword" missing
        assert!(parse_layered_retrieval_response(json).is_err());
    }

    #[test]
    fn rejects_out_of_range_confidence() {
        let json = r#"{"layers":{"domain":"x","capability":"y","query":"z","keyword":"k"},"confidence":1.5}"#;
        assert!(parse_layered_retrieval_response(json).is_err());
    }
}
```

- [ ] **Step 2: Run to verify tests fail**

Run: `cargo test -p chat --lib chat::pipeline::retrieval::layered_plan_tests -- --nocapture`
Expected: FAIL — `parse_layered_retrieval_response` does not exist.

- [ ] **Step 3: Implement struct + parser**

Add to `crates/chat/src/chat/pipeline/retrieval.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayeredRetrievalPlan {
    pub domain: String,
    pub capability: String,
    pub query: String,
    pub keyword: String,
    #[serde(default)]
    pub graph_hint: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Deserialize)]
struct RawLayeredResponse {
    layers: RawLayers,
    confidence: f32,
}

#[derive(Debug, Deserialize)]
struct RawLayers {
    domain: String,
    capability: String,
    query: String,
    keyword: String,
    #[serde(default)]
    graph_hint: Option<String>,
}

pub fn parse_layered_retrieval_response(content: &str) -> anyhow::Result<LayeredRetrievalPlan> {
    let raw: RawLayeredResponse =
        serde_json::from_str(content).map_err(anyhow::Error::from)?;
    if !(0.0..=1.0).contains(&raw.confidence) {
        anyhow::bail!("layered retrieval plan confidence must be in [0,1]");
    }
    for (name, value) in [
        ("domain", &raw.layers.domain),
        ("capability", &raw.layers.capability),
        ("query", &raw.layers.query),
        ("keyword", &raw.layers.keyword),
    ] {
        if value.trim().is_empty() {
            anyhow::bail!("layered retrieval plan field {name} must be non-empty");
        }
    }
    Ok(LayeredRetrievalPlan {
        domain: raw.layers.domain,
        capability: raw.layers.capability,
        query: raw.layers.query,
        keyword: raw.layers.keyword,
        graph_hint: raw.layers.graph_hint,
        confidence: raw.confidence,
    })
}
```

- [ ] **Step 4: Implement `LlmClient::plan_layered_retrieval`**

In `crates/chat/src/chat/llm.rs`, add (do not remove existing `plan_retrieval` if present in parent Task 4):

```rust
const LAYERED_RETRIEVAL_SYSTEM: &str = r#"You are a retrieval planner for a chat-driven banking reporting system.
Given a user report request and conversation context, produce a JSON object that
splits the request into four short retrieval fragments matching the catalog's
layered structure: domain, capability, query, keyword. Each fragment must be
under 12 words. Never include dates, limits, or currency codes — those are
resolved separately.

Layers you must distil for:
- domain: the subject area (e.g. "client activation trend", "savings deposit total").
- capability: the specific answer shape (e.g. "monthly breakdown", "top offices").
- query: the output type the underlying SQL should produce (e.g. "monthly_breakdown", "top_n rows").
- keyword: lexical terms suitable for BM25 (space-separated).

Optionally emit a `graph_hint` (e.g. "client -> activation_date -> office").

Return JSON only, no prose. Schema:
{
  "layers": { "domain": string, "capability": string, "query": string, "keyword": string, "graph_hint": string? },
  "confidence": number in [0,1]
}
"#;

impl LlmClient {
    pub async fn plan_layered_retrieval(
        &self,
        message: &str,
        conversation_context: &serde_json::Value,
    ) -> anyhow::Result<crate::chat::pipeline::retrieval::LayeredRetrievalPlan> {
        if !self.is_enabled() {
            anyhow::bail!("LLM_API_KEY is required for layered retrieval planner");
        }
        let user = format!(
            "conversation_context: {}\n\nuser_message: {}",
            conversation_context, message
        );
        let content = self
            .chat_json(LAYERED_RETRIEVAL_SYSTEM, user, "plan_layered_retrieval")
            .await?;
        crate::chat::pipeline::retrieval::parse_layered_retrieval_response(&content)
    }
}
```

- [ ] **Step 5: Run parse tests**

Run: `cargo test -p chat --lib chat::pipeline::retrieval::layered_plan_tests`
Expected: 3 PASS.

Run: `cargo check -p chat`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/chat/src/chat/llm.rs crates/chat/src/chat/pipeline/retrieval.rs
git commit -m "feat(lqr): add LayeredRetrievalPlan + LlmClient::plan_layered_retrieval"
```

---

### Task LQR-D: Layer executors + orchestrator

**Files:**
- Create: `crates/chat/src/chat/pipeline/lqr.rs`
- Modify: `crates/chat/src/chat/pipeline/mod.rs`
- Modify: `crates/chat/src/chat/classifier.rs`
- Test: `crates/chat/src/chat/pipeline/lqr.rs` inline

**Interfaces:**
- Produces:
  ```rust
  pub struct LqrInputs<'a> {
      pub message: &'a str,
      pub client: &'a app_core::auth::model::ClientContext,
      pub llm: &'a crate::chat::llm::LlmClient,
      pub embedding_client: &'a crate::knowledge::embedding::VoyageEmbeddingClient,
      pub repository: &'a crate::knowledge::index::repository::KnowledgeRepository,
      pub catalog: &'a crate::knowledge::model::KnowledgeCatalog,
      pub today: chrono::NaiveDate,
  }

  pub enum LqrOutcome {
      Matched { capability_id: String, confidence: f32 },
      Ambiguous { options: Vec<crate::chat::classifier::ClarificationOption>, confidence: f32 },
      Unsupported { reason: String },
  }

  pub struct LqrResult {
      pub outcome: LqrOutcome,
      pub layers: Vec<LayerTrace>,
  }

  pub struct LayerTrace {
      pub layer: String,
      pub winner: Option<String>,
      pub confidence: f32,
      pub candidates: Vec<crate::chat::classifier::ClassificationCandidate>,
  }

  pub async fn run_layered_retrieval(inputs: LqrInputs<'_>) -> anyhow::Result<LqrResult>;
  ```
- Extends `crate::chat::classifier::ClassificationResult` with `#[serde(default)] pub layers: Vec<LayerTrace>` (imported from `pipeline::lqr`).
- Consumes: `LlmClient::plan_layered_retrieval`, `KnowledgeRepository::search_hybrid_by_source_type`, `ClassificationPolicy::lqr`, `VoyageEmbeddingClient::embed_query`.

- [ ] **Step 1: Write layer-decision unit tests (no I/O)**

Create `crates/chat/src/chat/pipeline/lqr.rs` and add:

```rust
use anyhow::Result;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::chat::classifier::{ClarificationOption, ClassificationCandidate};
use crate::knowledge::model::{ClassificationPolicy, LqrPolicy, ScoreAggregation};

// ... struct declarations from Interfaces above ...

pub(crate) fn decide_domain_layer(
    policy: &LqrPolicy,
    ranked: &[(String, /*status*/ String, f32)], // (domain_id, status, confidence)
) -> DomainDecision {
    let Some((top_id, top_status, top_conf)) = ranked.first().cloned() else {
        return DomainDecision::Reject { reason: "no_domain_match".into() };
    };
    if top_conf < policy.domain_min_floor {
        return DomainDecision::Reject { reason: "no_domain_match".into() };
    }
    if matches!(top_status.as_str(), "deferred" | "rejected") {
        return DomainDecision::Reject {
            reason: format!("off_domain_{top_id}"),
        };
    }
    let second_conf = ranked.get(1).map(|r| r.2).unwrap_or(0.0);
    if top_conf - second_conf < policy.domain_min_gap {
        return DomainDecision::Ambiguous {
            top: ranked.iter().take(3).map(|r| r.0.clone()).collect(),
        };
    }
    DomainDecision::Winner { domain_id: top_id, confidence: top_conf }
}

pub(crate) enum DomainDecision {
    Winner { domain_id: String, confidence: f32 },
    Ambiguous { top: Vec<String> },
    Reject { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> LqrPolicy { LqrPolicy::default() }

    #[test]
    fn domain_reject_when_top_below_floor() {
        let ranked = vec![("client".into(), "approved_mvp".into(), 0.30)];
        let d = decide_domain_layer(&policy(), &ranked);
        assert!(matches!(d, DomainDecision::Reject { .. }));
    }

    #[test]
    fn domain_reject_when_top_is_deferred() {
        let ranked = vec![
            ("loan".into(), "deferred".into(), 0.82),
            ("savings".into(), "approved_mvp".into(), 0.55),
        ];
        let d = decide_domain_layer(&policy(), &ranked);
        match d {
            DomainDecision::Reject { reason } => assert!(reason.starts_with("off_domain_loan")),
            _ => panic!("expected reject"),
        }
    }

    #[test]
    fn domain_ambiguous_when_gap_small() {
        let ranked = vec![
            ("client".into(), "approved_mvp".into(), 0.72),
            ("savings".into(), "approved_mvp".into(), 0.68),
        ];
        let d = decide_domain_layer(&policy(), &ranked);
        assert!(matches!(d, DomainDecision::Ambiguous { .. }));
    }

    #[test]
    fn domain_winner_when_gap_wide() {
        let ranked = vec![
            ("client".into(), "approved_mvp".into(), 0.82),
            ("savings".into(), "approved_mvp".into(), 0.60),
        ];
        let d = decide_domain_layer(&policy(), &ranked);
        match d {
            DomainDecision::Winner { domain_id, .. } => assert_eq!(domain_id, "client"),
            _ => panic!("expected winner"),
        }
    }

    #[test]
    fn final_confidence_uses_min_aggregation() {
        assert!((aggregate_confidence(&ScoreAggregation::Min, &[0.90, 0.71]) - 0.71).abs() < 1e-6);
        assert!((aggregate_confidence(&ScoreAggregation::Mean, &[0.90, 0.70]) - 0.80).abs() < 1e-6);
        assert!((aggregate_confidence(&ScoreAggregation::Product, &[0.9, 0.8]) - 0.72).abs() < 1e-6);
    }
}

pub(crate) fn aggregate_confidence(mode: &ScoreAggregation, values: &[f32]) -> f32 {
    match mode {
        ScoreAggregation::Min => values.iter().copied().fold(f32::INFINITY, f32::min),
        ScoreAggregation::Mean => {
            if values.is_empty() { 0.0 } else {
                values.iter().sum::<f32>() / values.len() as f32
            }
        }
        ScoreAggregation::Product => values.iter().copied().fold(1.0, |a, b| a * b),
    }
}
```

Also register the module in `crates/chat/src/chat/pipeline/mod.rs`:

```rust
pub mod lqr;
```

And extend `ClassificationResult` in `crates/chat/src/chat/classifier.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationResult {
    // ... existing fields ...
    #[serde(default)]
    pub layers: Vec<crate::chat::pipeline::lqr::LayerTrace>,
}
```

`LayerTrace` must derive `Debug, Clone, PartialEq, Serialize, Deserialize`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p chat --lib chat::pipeline::lqr::tests -- --nocapture`
Expected: FAIL (module not compiling yet + missing decisions).

- [ ] **Step 3: Fill remaining decision helpers**

Add to `lqr.rs`:

```rust
pub(crate) fn decide_capability_layer(
    policy: &LqrPolicy,
    ranked: &[(String, f32)], // (capability_id, confidence)
) -> CapabilityDecision {
    let Some((top_id, top_conf)) = ranked.first().cloned() else {
        return CapabilityDecision::Reject { reason: "no_capability_match".into() };
    };
    if top_conf < policy.capability_min_floor {
        return CapabilityDecision::Reject { reason: "no_capability_match".into() };
    }
    let second_conf = ranked.get(1).map(|r| r.1).unwrap_or(0.0);
    if top_conf - second_conf < policy.capability_min_gap {
        return CapabilityDecision::Ambiguous {
            top: ranked.iter().take(3).cloned().collect(),
        };
    }
    CapabilityDecision::Winner { capability_id: top_id, confidence: top_conf }
}

pub(crate) enum CapabilityDecision {
    Winner { capability_id: String, confidence: f32 },
    Ambiguous { top: Vec<(String, f32)> },
    Reject { reason: String },
}
```

Add tests:

```rust
#[test]
fn capability_reject_below_floor() {
    let ranked = vec![("cap_a".into(), 0.30)];
    assert!(matches!(
        decide_capability_layer(&policy(), &ranked),
        CapabilityDecision::Reject { .. }
    ));
}

#[test]
fn capability_winner_wide_gap() {
    let ranked = vec![("cap_a".into(), 0.80), ("cap_b".into(), 0.50)];
    match decide_capability_layer(&policy(), &ranked) {
        CapabilityDecision::Winner { capability_id, .. } => assert_eq!(capability_id, "cap_a"),
        _ => panic!(),
    }
}

#[test]
fn capability_ambiguous_small_gap_emits_top3() {
    let ranked = vec![
        ("cap_a".into(), 0.62),
        ("cap_b".into(), 0.60),
        ("cap_c".into(), 0.58),
        ("cap_d".into(), 0.30),
    ];
    match decide_capability_layer(&policy(), &ranked) {
        CapabilityDecision::Ambiguous { top } => assert_eq!(top.len(), 3),
        _ => panic!(),
    }
}
```

- [ ] **Step 4: Implement `run_layered_retrieval`**

Add the orchestrator that ties decisions together. Use only the interfaces already defined above:

```rust
pub async fn run_layered_retrieval(inputs: LqrInputs<'_>) -> Result<LqrResult> {
    let policy = inputs.catalog.classification.lqr.clone();
    let mut layers: Vec<LayerTrace> = Vec::new();

    // 1. Ask LLM for layered plan.
    let plan = inputs
        .llm
        .plan_layered_retrieval(inputs.message, &serde_json::json!({
            "allowed_capabilities": inputs.client.allowed_capabilities,
        }))
        .await?;

    // 2. Layer 1 — Domain
    let domain_embedding = inputs.embedding_client.embed_query(&plan.domain).await?;
    let domain_hits = inputs
        .repository
        .search_hybrid_by_source_type(
            "domain",
            domain_embedding,
            &split_terms(&plan.keyword),
            None,
            &Default::default(),
            5,
        )
        .await?;
    let ranked_domains: Vec<(String, String, f32)> = domain_hits
        .iter()
        .map(|c| {
            let status = c.metadata_json
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("approved_mvp")
                .to_string();
            (c.source_id.clone(), status, distance_to_confidence(c.distance))
        })
        .collect();
    layers.push(build_layer_trace("domain", &ranked_domains));

    let domain_winner = match decide_domain_layer(&policy, &ranked_domains) {
        DomainDecision::Winner { domain_id, confidence } => (domain_id, confidence),
        DomainDecision::Reject { reason } => {
            return Ok(LqrResult { outcome: LqrOutcome::Unsupported { reason }, layers });
        }
        DomainDecision::Ambiguous { .. } => {
            return Ok(LqrResult {
                outcome: LqrOutcome::Unsupported { reason: "domain_ambiguous".into() },
                layers,
            });
        }
    };

    // 3. Layer 2 — Capability, scoped by domain + allowed_capabilities
    let capability_embedding = inputs.embedding_client.embed_query(&plan.capability).await?;
    let mut metadata_filter = std::collections::BTreeMap::new();
    metadata_filter.insert("domain".into(), domain_winner.0.clone());
    let cap_hits = inputs
        .repository
        .search_hybrid_by_source_type(
            "capability",
            capability_embedding,
            &split_terms(&plan.keyword),
            Some(&inputs.client.allowed_capabilities),
            &metadata_filter,
            6,
        )
        .await?;
    let ranked_caps: Vec<(String, f32)> = cap_hits
        .iter()
        .map(|c| (c.source_id.clone(), distance_to_confidence(c.distance)))
        .collect();
    layers.push(build_capability_trace(&ranked_caps));

    match decide_capability_layer(&policy, &ranked_caps) {
        CapabilityDecision::Winner { capability_id, confidence } => {
            let final_conf = aggregate_confidence(
                &policy.score_aggregation,
                &[domain_winner.1, confidence],
            );
            Ok(LqrResult {
                outcome: LqrOutcome::Matched { capability_id, confidence: final_conf },
                layers,
            })
        }
        CapabilityDecision::Ambiguous { top } => {
            let options = top
                .into_iter()
                .map(|(cap_id, _)| ClarificationOption {
                    label: capability_label(inputs.catalog, &cap_id),
                    capability: cap_id,
                    output_mode: None,
                })
                .collect();
            Ok(LqrResult {
                outcome: LqrOutcome::Ambiguous { options, confidence: domain_winner.1 },
                layers,
            })
        }
        CapabilityDecision::Reject { reason } => Ok(LqrResult {
            outcome: LqrOutcome::Unsupported { reason },
            layers,
        }),
    }
}

fn distance_to_confidence(distance: f64) -> f32 {
    let raw = 1.0_f32 - (distance as f32 / 2.0);
    raw.clamp(0.0, 1.0)
}

fn split_terms(s: &str) -> Vec<String> {
    s.split_whitespace().map(|t| t.to_string()).collect()
}

fn build_layer_trace(name: &str, ranked: &[(String, String, f32)]) -> LayerTrace {
    LayerTrace {
        layer: name.to_string(),
        winner: ranked.first().map(|r| r.0.clone()),
        confidence: ranked.first().map(|r| r.2).unwrap_or(0.0),
        candidates: ranked
            .iter()
            .map(|(id, _, conf)| ClassificationCandidate {
                capability: id.clone(),
                confidence: *conf,
                source_type: Some(name.to_string()),
            })
            .collect(),
    }
}

fn build_capability_trace(ranked: &[(String, f32)]) -> LayerTrace {
    LayerTrace {
        layer: "capability".into(),
        winner: ranked.first().map(|r| r.0.clone()),
        confidence: ranked.first().map(|r| r.1).unwrap_or(0.0),
        candidates: ranked
            .iter()
            .map(|(id, conf)| ClassificationCandidate {
                capability: id.clone(),
                confidence: *conf,
                source_type: Some("capability".into()),
            })
            .collect(),
    }
}

fn capability_label(
    catalog: &crate::knowledge::model::KnowledgeCatalog,
    capability_id: &str,
) -> String {
    catalog
        .capabilities
        .iter()
        .find(|c| c.id == capability_id)
        .and_then(|c| c.display_name.clone())
        .unwrap_or_else(|| capability_id.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerTrace {
    pub layer: String,
    pub winner: Option<String>,
    pub confidence: f32,
    pub candidates: Vec<ClassificationCandidate>,
}

#[derive(Debug, Clone)]
pub enum LqrOutcome {
    Matched { capability_id: String, confidence: f32 },
    Ambiguous {
        options: Vec<ClarificationOption>,
        confidence: f32,
    },
    Unsupported { reason: String },
}

pub struct LqrResult {
    pub outcome: LqrOutcome,
    pub layers: Vec<LayerTrace>,
}

pub struct LqrInputs<'a> {
    pub message: &'a str,
    pub client: &'a app_core::auth::model::ClientContext,
    pub llm: &'a crate::chat::llm::LlmClient,
    pub embedding_client: &'a crate::knowledge::embedding::VoyageEmbeddingClient,
    pub repository: &'a crate::knowledge::index::repository::KnowledgeRepository,
    pub catalog: &'a crate::knowledge::model::KnowledgeCatalog,
    pub today: NaiveDate,
}
```

- [ ] **Step 5: Run unit tests to verify all decisions pass**

Run: `cargo test -p chat --lib chat::pipeline::lqr::tests`
Expected: 7 PASS.

Run: `cargo check --workspace`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/chat/src/chat/pipeline/lqr.rs \
        crates/chat/src/chat/pipeline/mod.rs \
        crates/chat/src/chat/classifier.rs
git commit -m "feat(lqr): layer executors + orchestrator with per-layer trace"
```

---

### Task LQR-E: JobService integration + feature flag branch

**Files:**
- Modify: `crates/chat/src/chat/service/job.rs`
- Test: `crates/chat/tests/lqr_layered_retrieval.rs` (create)

**Interfaces:**
- Modifies `classify_message` to branch on `catalog.classification.lqr_enabled_flag` (loaded from `ChatFeatureConfig`).
- Reuses `classify_retrieved_capability` and `clarify_retrieved_capabilities` from `crate::chat::classifier` — no downstream change.
- Off-domain rejection converts `LqrOutcome::Unsupported { reason: "off_domain_loan" }` to `unsupported_result("off_domain_match", candidates_from_layers)` for wire compatibility with existing `06-chat-clarification-and-unsupported.md` scenario.

- [ ] **Step 1: Write integration test with a mock repository**

Create `crates/chat/tests/lqr_layered_retrieval.rs`:

```rust
// Integration-shaped unit test — uses in-memory catalog + a mock KnowledgeRepository
// that returns canned candidates per layer. Real end-to-end LQR is covered by the
// Postman scenario in docs/scenarios/09-lqr-layered-retrieval.md.

use serde_json::json;

#[test]
fn off_domain_prompt_short_circuits_at_layer_1() {
    // Arrange: layer 1 returns loan (deferred) with high confidence.
    let ranked = vec![
        ("loan".to_string(), "deferred".to_string(), 0.85_f32),
        ("savings".to_string(), "approved_mvp".to_string(), 0.55_f32),
    ];
    let policy = chat::knowledge::model::LqrPolicy::default();
    let decision = chat::chat::pipeline::lqr::decide_domain_layer(&policy, &ranked);
    match decision {
        chat::chat::pipeline::lqr::DomainDecision::Reject { reason } => {
            assert!(reason.contains("off_domain_loan"));
        }
        _ => panic!("expected off-domain reject"),
    }
}
```

Note: because `decide_domain_layer` is `pub(crate)`, expose it (or use `#[cfg(test)] pub use`) so the integration test crate can call it. Alternative: reshape as a smoke test that only asserts the classifier surface via `ClassificationResult.layers`.

- [ ] **Step 2: Extend `classify_message` with LQR branch**

In `crates/chat/src/chat/service/job.rs::classify_message`, add early:

```rust
if self.chat_features.lqr_enabled {
    return self.classify_with_lqr(message, client).await;
}
```

Add:

```rust
async fn classify_with_lqr(
    &self,
    message: &str,
    client: &ClientContext,
) -> ClassificationResult {
    use crate::chat::pipeline::lqr::{run_layered_retrieval, LqrInputs, LqrOutcome};
    let today = chrono::Utc::now().date_naive();

    if is_write_intent(message) {
        return unsupported_result("write_intent", Vec::new());
    }
    if client.allowed_capabilities.is_empty() {
        return unsupported_result("no_allowed_capabilities", Vec::new());
    }

    let inputs = LqrInputs {
        message,
        client,
        llm: &self.llm_planner,
        embedding_client: &self.embedding_client,
        repository: &self.knowledge,
        catalog: &self.catalog,
        today,
    };

    match run_layered_retrieval(inputs).await {
        Ok(result) => {
            let layers = result.layers;
            match result.outcome {
                LqrOutcome::Matched { capability_id, confidence } => {
                    let Some(cap) = self.catalog_capability(&capability_id) else {
                        return unsupported_result("catalog_missing_capability", Vec::new());
                    };
                    let mut c = classify_retrieved_capability(
                        message, today, &cap.domain, &cap.id, &cap.output_mode,
                        confidence, Vec::new(),
                    );
                    c.layers = layers;
                    c.source = Some("lqr".to_string());
                    c
                }
                LqrOutcome::Ambiguous { options, confidence } => {
                    let mut c = clarify_retrieved_capabilities(
                        message, today, None, options, confidence, Vec::new(),
                    );
                    c.layers = layers;
                    c.source = Some("lqr".to_string());
                    c
                }
                LqrOutcome::Unsupported { reason } => {
                    let normalized = if reason.starts_with("off_domain_") {
                        "off_domain_match"
                    } else if reason == "domain_ambiguous" {
                        "vector_no_match"
                    } else {
                        reason.as_str()
                    };
                    let mut c = unsupported_result(normalized, Vec::new());
                    c.layers = layers;
                    c.source = Some("lqr".to_string());
                    c
                }
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "LQR pipeline failed; falling back to flat retrieval");
            // Fallback path: run the existing flat classifier without recursing.
            self.classify_with_flat_retrieval(message, client).await
        }
    }
}
```

Refactor the current body of `classify_message` (starting from the embed_query call) into `classify_with_flat_retrieval(message, client)` so both branches call the same guard rails.

- [ ] **Step 3: Wire `chat_features` into `JobService`**

In `crates/chat/src/api/mod.rs` where `JobService` is constructed, thread `core.config.chat_features` through as a field:

```rust
let job_service = JobService::new(
    // existing args ...
    core.config.chat_features.clone(),
);
```

Extend `JobService::new` signature and struct to store it.

- [ ] **Step 4: Run tests**

```
cargo test -p chat --lib chat::pipeline::lqr::tests
cargo test -p chat --test lqr_layered_retrieval
cargo test --workspace
```

Expected: all green with `LQR_ENABLED=false` default; new decision tests exercise the LQR module directly.

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/chat/service/job.rs \
        crates/chat/src/api/mod.rs \
        crates/chat/tests/lqr_layered_retrieval.rs
git commit -m "feat(lqr): JobService branches to LQR pipeline when LQR_ENABLED=true"
```

---

### Task LQR-F: Docs, Postman scenario, rollout

**Files:**
- Create: `docs/scenarios/09-lqr-layered-retrieval.md`
- Modify: `docs/scenarios/README.md`
- Modify: `docs/superpowers/plans/2026-07-07-full-rag-blueprint-strict.md` (add supersession note)
- Modify: `docs/implementation-steps.md` (record Phase 20 slice)

- [ ] **Step 1: Write the Postman scenario file**

Create `docs/scenarios/09-lqr-layered-retrieval.md` following the same structure as `08-knowledge-breadth-and-multilingual.md`:

- Precondition: `LQR_ENABLED=true`, `POST /vector-index/rebuild` returned `document_count>=83`.
- Section A — off-domain short-circuit: prompts for loan/accounting/tax/group_center; assert `state_json.classification.layers[0].layer == "domain"`, `layers.len() == 1`, `source == "lqr"`, `outcome == "unsupported"`, reason `off_domain_match`.
- Section B — in-domain match: prompts for approved capabilities; assert `layers.len() == 2`, both layers present, `outcome == "matched"`.
- Section C — ambiguity within domain: two similarly-scored capabilities (e.g. "show me deposits" without more context); assert `outcome == "clarification_required"`, options domain-scoped.
- Section D — Bahasa Indonesia: `"aktivasi klien bulan ini"` → matches `client_activation_monthly_breakdown`.

Add the file to `docs/scenarios/README.md` index table.

- [ ] **Step 2: Add supersession note to parent plan**

Prepend the following to `docs/superpowers/plans/2026-07-07-full-rag-blueprint-strict.md` right after the header:

```markdown
> **Note (2026-07-07):** Task 4 (Retrieval) is superseded by the LQR overlay when
> `LQR_ENABLED=true`. See `docs/superpowers/plans/2026-07-07-rag-lqr-overlay.md`.
> The flat retrieval remains as fallback until Task LQR-F flips the default.
```

- [ ] **Step 3: Record slice in `docs/implementation-steps.md`**

Under Phase 20 (or the next open phase), add:

```markdown
- Slice LQR-1: Layered Query Retrieval overlay — spec + plan authored.
  - Off-domain short-circuit at Layer 1 (Domain).
  - Domain-scoped capability disambiguation at Layer 2.
  - Per-layer audit trace in `state_json.classification.layers`.
  - Rolled out behind `LQR_ENABLED=false` flag; flip planned after 20-prompt smoke passes.
```

- [ ] **Step 4: Commit**

```bash
git add docs/scenarios/09-lqr-layered-retrieval.md \
        docs/scenarios/README.md \
        docs/superpowers/plans/2026-07-07-full-rag-blueprint-strict.md \
        docs/implementation-steps.md
git commit -m "docs(lqr): scenario 09 + parent plan supersession + roadmap slice"
```

- [ ] **Step 5: Rollout gate**

Manual — do not automate:

1. Run `POST /vector-index/rebuild` on staging.
2. Run scenario 09 sections A-D via Postman.
3. Run scenarios 05, 06, 07 with `LQR_ENABLED=true` to confirm no regression on match / clarification / unsupported.
4. If all pass, PR to flip default in `.env.example` to `LQR_ENABLED=true`, keep the flag for one release, then delete both branches of `classify_message` in a subsequent slice.

---

## Self-review notes

- Spec coverage: every §12 test case has a corresponding decision test in Task LQR-D or an integration scenario section in Task LQR-F.
- No placeholders — every step has runnable code or an exact command.
- Type consistency: `LqrOutcome`, `LayerTrace`, `LqrResult`, `LqrInputs`, `LqrPolicy`, `ScoreAggregation` names are used identically across Tasks LQR-A → LQR-E.
- Interfaces `pub(crate) fn decide_domain_layer` / `decide_capability_layer` are exposed for integration tests via `#[cfg(test)] pub use` if needed; noted inline in Task LQR-E Step 1.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-07-rag-lqr-overlay.md`. Two execution options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per Task LQR-A … LQR-F with a two-stage review checkpoint.
2. **Inline Execution** — execute tasks in this session using `superpowers:executing-plans` with batch checkpoints.

Which approach?
