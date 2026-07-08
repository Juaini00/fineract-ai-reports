# Full RAG Blueprint Strict Implementation Plan

> **Note (2026-07-07):** Task 4 (Retrieval) is superseded by the LQR overlay when
> `LQR_ENABLED=true`. See `docs/superpowers/plans/2026-07-07-rag-lqr-overlay.md`.
> The flat retrieval remains as fallback until the rollout flips the default.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current vector-first classifier shortcut with the strict `Modern_RAG_Architecture_Blueprint.md` runtime pipeline.

**Architecture:** Keep the existing crate boundaries. Add focused modules inside `crates/chat/src/chat/pipeline/` for semantic parsing, routing, resolving, retrieval planning, hybrid retrieval, evidence evaluation, answer planning, and LLM answer generation. `JobService` becomes orchestration glue and must not bypass the pipeline with deterministic capability shortcuts.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL/pgvector, Voyage embeddings, OpenAI-compatible LLM chat completions, serde/serde_json, chrono.

## Global Constraints

- Do not add new crates.
- Do not let the LLM generate SQL.
- Do not index Fineract transactional rows in pgvector.
- `LLM_API_KEY` is required for chat reporting in strict mode.
- `VOYAGEAI_API_KEY` is required for vector retrieval in strict mode.
- Runtime retrieval requires the latest catalog version to be `embedded` and to match the loaded catalog content hash.
- Vector results never execute directly; execution still goes through approved catalog SQL and policy guard.
- Keep route -> service -> repository -> database. Do not put SQLx calls directly in route handlers.
- Use the envelope `{ "success": bool, "data": ..., "error": ... }` for API responses.
- MVP user-facing language is English only.

---

## File Structure

- Create `crates/chat/src/chat/pipeline/mod.rs`: exports strict pipeline modules and the main `run_strict_pipeline` function.
- Create `crates/chat/src/chat/pipeline/model.rs`: shared stage structs persisted into `chat_jobs.state_json`.
- Create `crates/chat/src/chat/pipeline/parser.rs`: LLM semantic parser and JSON schema validation.
- Create `crates/chat/src/chat/pipeline/router.rs`: deterministic route decision from parser output.
- Create `crates/chat/src/chat/pipeline/resolver.rs`: date, quantity, office, PII, and report parameter resolver.
- Create `crates/chat/src/chat/pipeline/retrieval.rs`: retrieval planner, keyword scorer, graph traversal, hybrid merge, reranker.
- Create `crates/chat/src/chat/pipeline/evidence.rs`: evidence evaluator and stale-index checks.
- Create `crates/chat/src/chat/pipeline/answer.rs`: answer planner, structured draft builder, LLM prose generator, grounding validation.
- Modify `crates/chat/src/chat/mod.rs`: add `pub mod pipeline;`.
- Modify `crates/chat/src/chat/llm.rs`: add parser, retrieval planner, and answer-generation LLM methods while keeping OpenAI-compatible transport local.
- Modify `crates/chat/src/chat/service/job.rs`: remove pre-parser `classify_savings_activity_list` shortcut from the main path and call strict pipeline.
- Modify `crates/chat/src/knowledge/index/repository.rs`: add latest embedded catalog metadata lookup and broader source-type retrieval.
- Modify `crates/chat/src/knowledge/index/sync.rs`: expose catalog content hash helper for loaded catalog checks.
- Modify `crates/chat/src/chat/executor.rs`: support optional `limit` for list-all behavior by binding `NULL` when the query metadata marks `limit.required=false`.
- Modify `crates/chat/src/chat/formatter/activity.rs`: keep structured response authoritative and avoid truncation claims when `limit` is absent.
- Modify `knowledge/queries/savings/activity_list.yaml`: keep `limit.required=false` and document `all` behavior.
- Modify `docs/Modern_RAG_Architecture_Blueprint.md`: align enum YAML path and strict operational behavior if implementation confirms current schema path.

---

### Task 1: Strict Pipeline Model

**Files:**
- Create: `crates/chat/src/chat/pipeline/model.rs`
- Create: `crates/chat/src/chat/pipeline/mod.rs`
- Modify: `crates/chat/src/chat/mod.rs`
- Test: `crates/chat/src/chat/pipeline/model.rs`

**Interfaces:**
- Produces: `StrictPipelineState`, `ParsedIntent`, `ResolvedConstraints`, `QuantityConstraint`, `RouteDecision`, `StrictPipelineError`.
- Consumes: existing `ClientContext`, `ClassificationResult`, `ExecutionPlan`, `PolicyDecision` in later tasks.

- [ ] **Step 1: Write the model tests**

Add this test module at the bottom of `crates/chat/src/chat/pipeline/model.rs` when creating the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantity_all_serializes_as_all_mode_without_value() {
        let quantity = QuantityConstraint::All;
        let json = serde_json::to_value(&quantity).unwrap();
        assert_eq!(json["mode"], "all");
        assert!(json.get("value").is_none() || json["value"].is_null());
    }

    #[test]
    fn pipeline_state_records_stage_outputs() {
        let mut state = StrictPipelineState::default();
        state.parser = Some(serde_json::json!({ "intent": "report" }));
        state.route = Some(serde_json::json!({ "workflow": "report" }));
        assert_eq!(state.parser.as_ref().unwrap()["intent"], "report");
        assert_eq!(state.route.as_ref().unwrap()["workflow"], "report");
    }
}
```

- [ ] **Step 2: Run the tests and verify they fail because the module does not exist**

Run: `cargo test -p chat chat::pipeline::model::tests`

Expected: compile failure mentioning `pipeline` or `model` is missing.

- [ ] **Step 3: Create the pipeline module exports**

Create `crates/chat/src/chat/pipeline/mod.rs`:

```rust
pub mod answer;
pub mod evidence;
pub mod model;
pub mod parser;
pub mod resolver;
pub mod retrieval;
pub mod router;
```

Modify `crates/chat/src/chat/mod.rs` and add:

```rust
pub mod pipeline;
```

- [ ] **Step 4: Create `model.rs` with shared structs**

Create `crates/chat/src/chat/pipeline/model.rs`:

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StrictPipelineState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_context: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_plan: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_evidence: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reranker: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_evaluation: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer_plan: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_answer: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParsedIntentKind {
    Report,
    ClarificationAnswer,
    Unsupported,
    ToolAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedIntent {
    pub intent: ParsedIntentKind,
    pub domain: Option<String>,
    #[serde(default)]
    pub entities: Vec<ParsedEntity>,
    pub constraints: ParsedConstraints,
    pub requires_retrieval: bool,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedEntity {
    pub entity_type: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParsedConstraints {
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub quantity: Option<QuantityConstraint>,
    pub currency_code: Option<String>,
    pub product_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum QuantityConstraint {
    All,
    Default,
    Limit { value: i64 },
    TopN { value: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteDecision {
    Report,
    Clarify,
    Unsupported,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResolvedConstraints {
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub quantity: Option<QuantityConstraint>,
    pub currency_code: Option<String>,
    pub product_ids: Option<Vec<i64>>,
    pub office_scope: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalEvidence {
    pub source_type: String,
    pub source_id: String,
    pub title: String,
    pub score: f32,
    pub metadata_json: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrictPipelineError {
    pub code: String,
    pub message: String,
}
```

Include the tests from Step 1 at the bottom of the file.

- [ ] **Step 5: Run model tests**

Run: `cargo test -p chat chat::pipeline::model::tests`
Expected: PASS.

---

### Task 2: LLM Semantic Parser

**Files:**
- Create: `crates/chat/src/chat/pipeline/parser.rs`
- Modify: `crates/chat/src/chat/llm.rs`
- Test: `crates/chat/src/chat/pipeline/parser.rs`, `crates/chat/src/chat/llm.rs`

**Interfaces:**
- Consumes: `ParsedIntent`, `ParsedIntentKind`, `QuantityConstraint` from Task 1.
- Produces: `parse_semantic_response(content: &str) -> anyhow::Result<ParsedIntent>` and `LlmPlannerClient::parse_intent(&self, message: &str, context: &serde_json::Value) -> Result<ParsedIntent>`.

- [ ] **Step 1: Write parser tests**

Create `crates/chat/src/chat/pipeline/parser.rs` with this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::pipeline::model::{ParsedIntentKind, QuantityConstraint};

    #[test]
    fn parses_all_activity_intent() {
        let parsed = parse_semantic_response(
            r#"{
              "intent":"report",
              "domain":"savings",
              "entities":[{"type":"capability_hint","value":"savings activity"}],
              "constraints":{
                "from_date":"2026-07-01",
                "to_date":"2026-07-07",
                "quantity":{"mode":"all"},
                "currency_code":null,
                "product_ids":null
              },
              "requires_retrieval":true,
              "confidence":0.91
            }"#,
        )
        .unwrap();

        assert_eq!(parsed.intent, ParsedIntentKind::Report);
        assert_eq!(parsed.domain.as_deref(), Some("savings"));
        assert_eq!(parsed.constraints.quantity, Some(QuantityConstraint::All));
    }

    #[test]
    fn rejects_malformed_parser_json() {
        let error = parse_semantic_response("not-json").unwrap_err();
        assert!(error.to_string().contains("parse semantic parser JSON"));
    }
}
```

- [ ] **Step 2: Run parser tests and verify failure**

Run: `cargo test -p chat chat::pipeline::parser::tests`
Expected: compile failure because `parse_semantic_response` does not exist.

- [ ] **Step 3: Implement parser JSON validation**

Add above the tests in `parser.rs`:

```rust
use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::chat::pipeline::model::{
    ParsedConstraints, ParsedEntity, ParsedIntent, ParsedIntentKind, QuantityConstraint,
};

#[derive(Debug, Deserialize)]
struct RawParsedIntent {
    intent: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    entities: Vec<RawEntity>,
    #[serde(default)]
    constraints: RawConstraints,
    #[serde(default)]
    requires_retrieval: bool,
    confidence: f32,
}

#[derive(Debug, Deserialize)]
struct RawEntity {
    #[serde(rename = "type")]
    entity_type: String,
    value: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawConstraints {
    from_date: Option<String>,
    to_date: Option<String>,
    quantity: Option<RawQuantity>,
    currency_code: Option<String>,
    product_ids: Option<Vec<i64>>,
}

#[derive(Debug, Deserialize)]
struct RawQuantity {
    mode: String,
    value: Option<i64>,
}

pub fn parse_semantic_response(content: &str) -> Result<ParsedIntent> {
    let raw: RawParsedIntent = serde_json::from_str(content).context("parse semantic parser JSON")?;
    if !(0.0..=1.0).contains(&raw.confidence) {
        bail!("semantic parser confidence must be between 0 and 1");
    }

    let intent = match raw.intent.as_str() {
        "report" => ParsedIntentKind::Report,
        "clarification_answer" => ParsedIntentKind::ClarificationAnswer,
        "unsupported" => ParsedIntentKind::Unsupported,
        "tool_action" => ParsedIntentKind::ToolAction,
        other => bail!("unsupported semantic parser intent {other}"),
    };

    let quantity = match raw.constraints.quantity {
        None => None,
        Some(raw) => Some(match raw.mode.as_str() {
            "all" => QuantityConstraint::All,
            "default" => QuantityConstraint::Default,
            "limit" => QuantityConstraint::Limit {
                value: positive_quantity(raw.value, "limit")?,
            },
            "top_n" => QuantityConstraint::TopN {
                value: positive_quantity(raw.value, "top_n")?,
            },
            other => bail!("unsupported quantity mode {other}"),
        }),
    };

    Ok(ParsedIntent {
        intent,
        domain: raw.domain,
        entities: raw
            .entities
            .into_iter()
            .map(|entity| ParsedEntity {
                entity_type: entity.entity_type,
                value: entity.value,
            })
            .collect(),
        constraints: ParsedConstraints {
            from_date: raw.constraints.from_date,
            to_date: raw.constraints.to_date,
            quantity,
            currency_code: raw.constraints.currency_code,
            product_ids: raw.constraints.product_ids,
        },
        requires_retrieval: raw.requires_retrieval,
        confidence: raw.confidence,
    })
}

fn positive_quantity(value: Option<i64>, mode: &str) -> Result<i64> {
    let Some(value) = value else {
        bail!("quantity {mode} requires value");
    };
    if value < 1 {
        bail!("quantity {mode} value must be positive");
    }
    Ok(value)
}
```

- [ ] **Step 4: Add LLM client method**

Modify `crates/chat/src/chat/llm.rs` imports:

```rust
use crate::chat::pipeline::model::ParsedIntent;
use crate::chat::pipeline::parser::parse_semantic_response;
```

Add this method inside `impl LlmPlannerClient`:

```rust
pub async fn parse_intent(
    &self,
    message: &str,
    context: &serde_json::Value,
) -> Result<ParsedIntent> {
    if !self.is_enabled() {
        bail!("LLM_API_KEY is required for semantic parser");
    }

    let system = "You are the semantic parser for a reporting RAG pipeline. Return only JSON. Extract intent, domain, entities, date constraints, and quantity. Do not choose SQL. Do not invent capability ids.";
    let user = json!({
        "user_message": message,
        "context": context,
        "response_schema": {
            "intent": "report | clarification_answer | unsupported | tool_action",
            "domain": "savings | client | organization | unknown",
            "entities": [{ "type": "capability_hint | product | currency | office | date_period", "value": "string" }],
            "constraints": {
                "from_date": "YYYY-MM-DD or null",
                "to_date": "YYYY-MM-DD or null",
                "quantity": { "mode": "all | limit | top_n | default", "value": "integer when needed" },
                "currency_code": "string or null",
                "product_ids": "array of integers or null"
            },
            "requires_retrieval": true,
            "confidence": "number between 0 and 1"
        }
    })
    .to_string();

    let content = self.chat_json(system, user, "semantic parser").await?;
    parse_semantic_response(&content)
}
```

Add private helper inside `impl LlmPlannerClient` and refactor `choose_capability` later only if needed:

```rust
async fn chat_json(&self, system: &str, user: String, operation: &str) -> Result<String> {
    let response = self
        .http
        .post(&self.url)
        .bearer_auth(&self.api_key)
        .json(&json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
            "temperature": self.temperature,
            "max_tokens": self.max_output_tokens,
            "response_format": { "type": "json_object" }
        }))
        .send()
        .await
        .with_context(|| format!("request {} {operation}", self.provider))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("{} {operation} failed with status {status}: {body}", self.provider);
    }

    let payload: ChatCompletionResponse = response
        .json()
        .await
        .with_context(|| format!("parse {} {operation} response", self.provider))?;
    payload
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .with_context(|| format!("{} {operation} returned no choice", self.provider))
}
```

- [ ] **Step 5: Run parser tests**

Run: `cargo test -p chat chat::pipeline::parser::tests`
Expected: PASS.

---

### Task 3: Router And Constraint Resolver

**Files:**
- Create: `crates/chat/src/chat/pipeline/router.rs`
- Create: `crates/chat/src/chat/pipeline/resolver.rs`
- Test: both new files

**Interfaces:**
- Consumes: `ParsedIntent`, `QuantityConstraint`, `RouteDecision`.
- Produces: `route_intent(parsed: &ParsedIntent) -> RouteDecision` and `resolve_constraints(parsed: &ParsedIntent) -> anyhow::Result<ResolvedConstraints>`.

- [ ] **Step 1: Write router tests**

Create `router.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::pipeline::model::{ParsedConstraints, ParsedIntent, ParsedIntentKind, RouteDecision};

    fn parsed(intent: ParsedIntentKind, confidence: f32) -> ParsedIntent {
        ParsedIntent {
            intent,
            domain: Some("savings".to_string()),
            entities: Vec::new(),
            constraints: ParsedConstraints::default(),
            requires_retrieval: true,
            confidence,
        }
    }

    #[test]
    fn report_routes_to_report_when_confident() {
        assert_eq!(route_intent(&parsed(ParsedIntentKind::Report, 0.8)), RouteDecision::Report);
    }

    #[test]
    fn low_confidence_report_routes_to_clarify() {
        assert_eq!(route_intent(&parsed(ParsedIntentKind::Report, 0.3)), RouteDecision::Clarify);
    }

    #[test]
    fn tool_action_routes_to_unsupported() {
        assert_eq!(route_intent(&parsed(ParsedIntentKind::ToolAction, 0.9)), RouteDecision::Unsupported);
    }
}
```

- [ ] **Step 2: Implement router**

Add above tests:

```rust
use crate::chat::pipeline::model::{ParsedIntent, ParsedIntentKind, RouteDecision};

pub fn route_intent(parsed: &ParsedIntent) -> RouteDecision {
    match parsed.intent {
        ParsedIntentKind::Report | ParsedIntentKind::ClarificationAnswer => {
            if parsed.confidence < 0.40 {
                RouteDecision::Clarify
            } else {
                RouteDecision::Report
            }
        }
        ParsedIntentKind::Unsupported | ParsedIntentKind::ToolAction => RouteDecision::Unsupported,
    }
}
```

- [ ] **Step 3: Write resolver tests**

Create `resolver.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::pipeline::model::{ParsedConstraints, ParsedIntent, ParsedIntentKind, QuantityConstraint};

    fn parsed(quantity: QuantityConstraint) -> ParsedIntent {
        ParsedIntent {
            intent: ParsedIntentKind::Report,
            domain: Some("savings".to_string()),
            entities: Vec::new(),
            constraints: ParsedConstraints {
                from_date: Some("2026-07-01".to_string()),
                to_date: Some("2026-07-07".to_string()),
                quantity: Some(quantity),
                currency_code: None,
                product_ids: None,
            },
            requires_retrieval: true,
            confidence: 0.9,
        }
    }

    #[test]
    fn resolves_all_without_limit_value() {
        let resolved = resolve_constraints(&parsed(QuantityConstraint::All)).unwrap();
        assert_eq!(resolved.quantity, Some(QuantityConstraint::All));
        assert_eq!(resolved.office_scope, "authorized_scope");
    }

    #[test]
    fn rejects_missing_date_range() {
        let mut input = parsed(QuantityConstraint::Default);
        input.constraints.from_date = None;
        let error = resolve_constraints(&input).unwrap_err();
        assert!(error.to_string().contains("from_date is required"));
    }
}
```

- [ ] **Step 4: Implement resolver**

Add above tests:

```rust
use anyhow::{Result, bail};

use crate::chat::pipeline::model::{ParsedIntent, ResolvedConstraints};

pub fn resolve_constraints(parsed: &ParsedIntent) -> Result<ResolvedConstraints> {
    if parsed.constraints.from_date.is_none() {
        bail!("from_date is required");
    }
    if parsed.constraints.to_date.is_none() {
        bail!("to_date is required");
    }

    Ok(ResolvedConstraints {
        from_date: parsed.constraints.from_date.clone(),
        to_date: parsed.constraints.to_date.clone(),
        quantity: parsed.constraints.quantity.clone(),
        currency_code: parsed.constraints.currency_code.clone(),
        product_ids: parsed.constraints.product_ids.clone(),
        office_scope: "authorized_scope".to_string(),
    })
}
```

- [ ] **Step 5: Run router/resolver tests**

Run: `cargo test -p chat chat::pipeline::router::tests chat::pipeline::resolver::tests`
Expected: PASS.

---

### Task 4: Strict Index State Checks And Retrieval Evidence

**Files:**
- Modify: `crates/chat/src/knowledge/index/repository.rs`
- Modify: `crates/chat/src/knowledge/index/sync.rs`
- Create: `crates/chat/src/chat/pipeline/retrieval.rs`
- Test: `crates/chat/src/chat/pipeline/retrieval.rs`

**Interfaces:**
- Consumes: `ResolvedConstraints`, `RetrievalEvidence`.
- Produces: `RetrievalPlanStrict`, `HybridEvidenceSet`, `build_retrieval_plan`, `keyword_score`, `graph_evidence`.

- [ ] **Step 1: Write retrieval unit tests**

Create `retrieval.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::pipeline::model::{QuantityConstraint, ResolvedConstraints};

    fn constraints() -> ResolvedConstraints {
        ResolvedConstraints {
            from_date: Some("2026-07-01".to_string()),
            to_date: Some("2026-07-07".to_string()),
            quantity: Some(QuantityConstraint::All),
            currency_code: None,
            product_ids: None,
            office_scope: "authorized_scope".to_string(),
        }
    }

    #[test]
    fn retrieval_plan_uses_resolved_constraints() {
        let plan = build_retrieval_plan("savings", &constraints());
        assert!(plan.vector_query.contains("savings"));
        assert_eq!(plan.metadata_filter.get("domain").unwrap(), "savings");
        assert_eq!(plan.metadata_filter.get("quantity").unwrap(), "all");
    }

    #[test]
    fn keyword_score_counts_token_overlap() {
        let score = keyword_score("savings activity", "list savings account activity transactions");
        assert!(score > 0.0);
    }
}
```

- [ ] **Step 2: Implement retrieval structs and helpers**

Add above tests:

```rust
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::chat::pipeline::model::{QuantityConstraint, ResolvedConstraints};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalPlanStrict {
    pub vector_query: String,
    pub keyword_query: String,
    pub graph_query: String,
    pub metadata_filter: BTreeMap<String, String>,
}

pub fn build_retrieval_plan(domain: &str, constraints: &ResolvedConstraints) -> RetrievalPlanStrict {
    let mut metadata_filter = BTreeMap::new();
    metadata_filter.insert("domain".to_string(), domain.to_string());
    metadata_filter.insert("office_scope".to_string(), constraints.office_scope.clone());
    if let Some(quantity) = constraints.quantity.as_ref() {
        metadata_filter.insert("quantity".to_string(), quantity_mode(quantity).to_string());
    }

    RetrievalPlanStrict {
        vector_query: format!("{domain} reporting activity capability query"),
        keyword_query: format!("{domain} activity transactions report"),
        graph_query: format!("{domain} -> capability -> query -> data_area"),
        metadata_filter,
    }
}

pub fn keyword_score(query: &str, document: &str) -> f32 {
    let query_tokens = tokens(query);
    if query_tokens.is_empty() {
        return 0.0;
    }
    let document_tokens = tokens(document);
    let hits = query_tokens
        .iter()
        .filter(|token| document_tokens.iter().any(|candidate| candidate == *token))
        .count();
    hits as f32 / query_tokens.len() as f32
}

fn quantity_mode(quantity: &QuantityConstraint) -> &'static str {
    match quantity {
        QuantityConstraint::All => "all",
        QuantityConstraint::Default => "default",
        QuantityConstraint::Limit { .. } => "limit",
        QuantityConstraint::TopN { .. } => "top_n",
    }
}

fn tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.len() >= 2)
        .map(str::to_lowercase)
        .collect()
}
```

- [ ] **Step 3: Add repository metadata method**

Modify `crates/chat/src/knowledge/index/repository.rs` by adding:

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LatestCatalogIndex {
    pub id: Uuid,
    pub content_hash: String,
    pub status: String,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<i32>,
}
```

Add method inside `impl KnowledgeRepository`:

```rust
pub async fn latest_embedded_catalog(&self) -> Result<Option<LatestCatalogIndex>> {
    let row = sqlx::query_as::<_, LatestCatalogIndex>(
        r#"
        SELECT id, content_hash, status, embedding_model, embedding_dimensions
        FROM knowledge_catalog_versions
        WHERE status = 'embedded'
        ORDER BY synced_at DESC NULLS LAST, created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(&self.pool)
    .await?;
    Ok(row)
}
```

Ensure `Uuid` is imported if not already in scope.

- [ ] **Step 4: Run retrieval tests**

Run: `cargo test -p chat chat::pipeline::retrieval::tests`
Expected: PASS.

---

### Task 5: Evidence Evaluator

**Files:**
- Create: `crates/chat/src/chat/pipeline/evidence.rs`
- Test: same file

**Interfaces:**
- Consumes: `RetrievalEvidence`, catalog hash strings.
- Produces: `EvidenceDecision`, `evaluate_evidence`.

- [ ] **Step 1: Write evidence tests**

Create `evidence.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::pipeline::model::RetrievalEvidence;

    fn evidence(source_type: &str, source_id: &str) -> RetrievalEvidence {
        RetrievalEvidence {
            source_type: source_type.to_string(),
            source_id: source_id.to_string(),
            title: source_id.to_string(),
            score: 0.9,
            metadata_json: serde_json::json!({}),
        }
    }

    #[test]
    fn accepts_complete_capability_query_policy_response_evidence() {
        let decision = evaluate_evidence(
            "abc",
            "abc",
            &[
                evidence("capability", "savings_activity_list"),
                evidence("query", "savings.activity_list"),
                evidence("policy", "savings_pii"),
                evidence("response", "savings_activity_list"),
            ],
        );
        assert!(decision.enough);
    }

    #[test]
    fn rejects_stale_index_hash() {
        let decision = evaluate_evidence("abc", "def", &[evidence("capability", "x")]);
        assert!(!decision.enough);
        assert_eq!(decision.reason.as_deref(), Some("vector_index_stale"));
    }
}
```

- [ ] **Step 2: Implement evidence evaluator**

Add above tests:

```rust
use serde::{Deserialize, Serialize};

use crate::chat::pipeline::model::RetrievalEvidence;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceDecision {
    pub enough: bool,
    pub reason: Option<String>,
    pub source_count: usize,
    pub source_types: Vec<String>,
}

pub fn evaluate_evidence(
    loaded_catalog_hash: &str,
    embedded_catalog_hash: &str,
    evidence: &[RetrievalEvidence],
) -> EvidenceDecision {
    let mut source_types = evidence
        .iter()
        .map(|item| item.source_type.clone())
        .collect::<Vec<_>>();
    source_types.sort();
    source_types.dedup();

    if loaded_catalog_hash != embedded_catalog_hash {
        return decision(false, Some("vector_index_stale"), evidence.len(), source_types);
    }

    for required in ["capability", "query", "policy", "response"] {
        if !source_types.iter().any(|source_type| source_type == required) {
            return decision(
                false,
                Some(format!("missing_required_evidence:{required}")),
                evidence.len(),
                source_types,
            );
        }
    }

    decision(true, None, evidence.len(), source_types)
}

fn decision(
    enough: bool,
    reason: Option<impl Into<String>>,
    source_count: usize,
    source_types: Vec<String>,
) -> EvidenceDecision {
    EvidenceDecision {
        enough,
        reason: reason.map(Into::into),
        source_count,
        source_types,
    }
}
```

- [ ] **Step 3: Run evidence tests**

Run: `cargo test -p chat chat::pipeline::evidence::tests`
Expected: PASS.

---

### Task 6: Optional Limit Execution For `all`

**Files:**
- Modify: `crates/chat/src/chat/executor.rs`
- Modify: `knowledge/queries/savings/activity_list.yaml`
- Test: `crates/chat/src/chat/executor.rs` or existing classifier tests

**Interfaces:**
- Consumes: `ExecutionPlan.params` where `limit` is absent for all-list requests.
- Produces: executor binds `NULL::<i64>` when `limit` parameter is optional and missing.

- [ ] **Step 1: Add executor unit test for optional integer param**

Add to `crates/chat/src/chat/executor.rs` test module:

```rust
#[test]
fn optional_integer_param_returns_none_when_missing() {
    use crate::chat::planner::{AnswerPlan, EvidenceEvaluation, ExecutionPlan, ExecutionPlanType, RetrievalPlan};
    use crate::knowledge::model::QueryParameter;

    let plan = ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: "savings".to_string(),
        capability: "savings_activity_list".to_string(),
        query_id: "savings.activity_list".to_string(),
        output_mode: "list".to_string(),
        params: serde_json::json!({}),
        retrieval_plan: RetrievalPlan::default(),
        evidence_evaluation: EvidenceEvaluation::default(),
        answer_plan: AnswerPlan::default(),
        requires_policy_check: true,
    };
    let parameter = QueryParameter {
        name: "limit".to_string(),
        kind: "integer".to_string(),
        required: false,
        source: None,
    };

    assert_eq!(integer_param(&plan, &parameter).unwrap(), None);
}
```

- [ ] **Step 2: Run test and verify it already passes or fails for visibility**

Run: `cargo test -p chat chat::executor::tests::optional_integer_param_returns_none_when_missing`
Expected: PASS if current helper already supports optional values. If it fails, fix `integer_param` only.

- [ ] **Step 3: Ensure activity list limit metadata is optional**

In `knowledge/queries/savings/activity_list.yaml`, ensure:

```yaml
  - name: limit
    type: integer
    required: false
```

Ensure required filters says:

```yaml
  - bounded by limit when provided
```

- [ ] **Step 4: Run catalog validator tests**

Run: `cargo test -p chat knowledge::catalog::validator`
Expected: PASS.

---

### Task 7: LLM Answer Generator And Grounding Validator

**Files:**
- Create: `crates/chat/src/chat/pipeline/answer.rs`
- Modify: `crates/chat/src/chat/llm.rs`
- Test: `crates/chat/src/chat/pipeline/answer.rs`

**Interfaces:**
- Consumes: structured response JSON and SQL result rows.
- Produces: `GeneratedAnswer { message, citations }` and `validate_grounded_answer`.

- [ ] **Step 1: Write answer grounding tests**

Create `answer.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_citations_to_existing_result_paths() {
        let structured = serde_json::json!({
            "answer_plan": { "coverage": { "returned_rows": 1 } },
            "structured": { "rows": [{ "transaction_id": 1 }] }
        });
        let answer = GeneratedAnswer {
            message: "One transaction.".to_string(),
            citations: vec!["structured.rows[0]".to_string(), "answer_plan.coverage".to_string()],
        };
        validate_grounded_answer(&structured, &answer).unwrap();
    }

    #[test]
    fn rejects_missing_row_citation() {
        let structured = serde_json::json!({ "structured": { "rows": [] } });
        let answer = GeneratedAnswer {
            message: "Missing.".to_string(),
            citations: vec!["structured.rows[0]".to_string()],
        };
        assert!(validate_grounded_answer(&structured, &answer).is_err());
    }
}
```

- [ ] **Step 2: Implement answer model and grounding validator**

Add above tests:

```rust
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedAnswer {
    pub message: String,
    #[serde(default)]
    pub citations: Vec<String>,
}

pub fn parse_generated_answer(content: &str) -> Result<GeneratedAnswer> {
    serde_json::from_str(content).map_err(anyhow::Error::from)
}

pub fn validate_grounded_answer(structured: &Value, answer: &GeneratedAnswer) -> Result<()> {
    if answer.message.trim().is_empty() {
        bail!("generated answer message is empty");
    }
    for citation in &answer.citations {
        if !citation_exists(structured, citation) {
            bail!("generated answer citation does not exist: {citation}");
        }
    }
    Ok(())
}

fn citation_exists(structured: &Value, citation: &str) -> bool {
    if citation == "answer_plan.coverage" {
        return structured.pointer("/answer_plan/coverage").is_some();
    }
    if let Some(index) = citation
        .strip_prefix("structured.rows[")
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|value| value.parse::<usize>().ok())
    {
        return structured
            .pointer("/structured/rows")
            .and_then(Value::as_array)
            .is_some_and(|rows| index < rows.len());
    }
    false
}
```

- [ ] **Step 3: Add LLM answer method**

Modify `crates/chat/src/chat/llm.rs` imports:

```rust
use crate::chat::pipeline::answer::{GeneratedAnswer, parse_generated_answer};
```

Add method inside `impl LlmPlannerClient`:

```rust
pub async fn generate_answer(
    &self,
    user_message: &str,
    structured: &serde_json::Value,
) -> Result<GeneratedAnswer> {
    if !self.is_enabled() {
        bail!("LLM_API_KEY is required for answer generation");
    }
    let system = "You generate grounded reporting prose. Return only JSON with message and citations. Do not add facts not present in structured input.";
    let user = json!({
        "user_message": user_message,
        "structured_response": structured,
        "response_schema": { "message": "markdown string", "citations": ["answer_plan.coverage", "structured.rows[0]"] }
    })
    .to_string();
    let content = self.chat_json(system, user, "answer generation").await?;
    parse_generated_answer(&content)
}
```

- [ ] **Step 4: Run answer tests**

Run: `cargo test -p chat chat::pipeline::answer::tests`
Expected: PASS.

---

### Task 8: Orchestrate Strict Pipeline In JobService

**Files:**
- Modify: `crates/chat/src/chat/service/job.rs`
- Modify: `crates/chat/src/chat/pipeline/mod.rs`
- Test: `crates/chat/tests/chat_jobs.rs` or new focused service unit tests

**Interfaces:**
- Consumes: parser/router/resolver/retrieval/evidence/answer modules.
- Produces: chat jobs with `state_json.strict_pipeline` containing every stage.

- [ ] **Step 1: Add strict config failure integration test**

In `crates/chat/tests/chat_jobs.rs`, add a focused test using the existing app test harness pattern from the file:

```rust
#[tokio::test]
async fn chat_reporting_requires_llm_in_strict_pipeline() {
    // Use the existing spawn helper in this file. Set LLM_API_KEY empty in the test config.
    // Create an API key with savings_activity_list allowed.
    // Submit: "show me the list of all saving activity for this month".
    // Assert final job status is failed and error_json.code == "pipeline_config_error".
}
```

If the existing harness cannot override config in this file, place this test in the existing integration harness that already constructs `AppConfig` manually.

- [ ] **Step 2: Run the new test and verify failure**

Run: `cargo test -p chat chat_reporting_requires_llm_in_strict_pipeline`
Expected: FAIL because current flow still falls back/deterministically classifies.

- [ ] **Step 3: Add strict pipeline entrypoint skeleton**

In `crates/chat/src/chat/pipeline/mod.rs`, add:

```rust
use anyhow::{Result, bail};
use app_core::auth::model::ClientContext;
use serde_json::json;

use crate::chat::llm::LlmPlannerClient;
use crate::chat::pipeline::model::{RouteDecision, StrictPipelineState};

pub struct StrictPipelineInput<'a> {
    pub message: &'a str,
    pub client: &'a ClientContext,
    pub llm: &'a LlmPlannerClient,
}

pub struct StrictPipelineOutput {
    pub state: StrictPipelineState,
}

pub async fn run_strict_pipeline(input: StrictPipelineInput<'_>) -> Result<StrictPipelineOutput> {
    if !input.llm.is_enabled() {
        bail!("pipeline_config_error: LLM_API_KEY is required for strict pipeline");
    }

    let mut state = StrictPipelineState {
        conversation_context: Some(json!({
            "api_key_id": input.client.api_key_id,
            "allowed_capabilities": input.client.allowed_capabilities,
            "allowed_office_ids": input.client.allowed_office_ids,
            "can_view_pii": input.client.can_view_pii,
        })),
        ..StrictPipelineState::default()
    };

    let context = state.conversation_context.clone().unwrap_or_else(|| json!({}));
    let parsed = input.llm.parse_intent(input.message, &context).await?;
    state.parser = Some(serde_json::to_value(&parsed)?);

    let route = router::route_intent(&parsed);
    state.route = Some(json!({ "decision": route }));
    if route != RouteDecision::Report {
        bail!("unsupported_request: strict pipeline did not route to report");
    }

    let resolved = resolver::resolve_constraints(&parsed)?;
    state.resolver = Some(serde_json::to_value(&resolved)?);

    Ok(StrictPipelineOutput { state })
}
```

- [ ] **Step 4: Wire JobService to call strict pipeline early**

In `classify_with_retrieval`, before any deterministic classifier code, call strict pipeline and convert config errors to `unsupported_result` only temporarily if full output integration is not complete:

```rust
match crate::chat::pipeline::run_strict_pipeline(crate::chat::pipeline::StrictPipelineInput {
    message,
    client,
    llm: &self.llm_planner,
}).await {
    Ok(output) => {
        return ClassificationResult {
            outcome: ClassificationOutcome::Unsupported,
            domain: None,
            capability: None,
            confidence: 0.0,
            params: json!({ "strict_pipeline": output.state }),
            clarification: Some("Strict pipeline reached semantic parsing; execution wiring continues in the next task.".to_string()),
            options: Vec::new(),
            source: Some("strict_pipeline".to_string()),
            candidates: Vec::new(),
        };
    }
    Err(error) if error.to_string().contains("pipeline_config_error") => {
        return unsupported_result("pipeline_config_error", Vec::new());
    }
    Err(error) => {
        warn!(error = %error, "strict pipeline failed");
        return unsupported_result("strict_pipeline_failed", Vec::new());
    }
}
```

This step intentionally stops before SQL execution so the first integration gate proves strict config behavior without mixing every stage into one unreviewable patch.

- [ ] **Step 5: Run strict config test**

Run: `cargo test -p chat chat_reporting_requires_llm_in_strict_pipeline`
Expected: PASS.

---

### Task 9: Complete Capability Selection And Execution Plan From Strict Evidence

**Files:**
- Modify: `crates/chat/src/chat/pipeline/retrieval.rs`
- Modify: `crates/chat/src/chat/pipeline/mod.rs`
- Modify: `crates/chat/src/chat/planner.rs`
- Modify: `crates/chat/src/chat/service/job.rs`
- Test: `crates/chat/tests/chat_jobs.rs`

**Interfaces:**
- Consumes: `RetrievalEvidence`, catalog capabilities/queries.
- Produces: `ClassificationResult` with source `strict_pipeline`, selected capability, params, candidates, and all stage state.

- [ ] **Step 1: Add all-list integration test**

In `crates/chat/tests/chat_jobs.rs`, add or update:

```rust
#[tokio::test]
async fn all_activity_request_does_not_apply_default_limit() {
    // Existing harness should configure fake LLM response for semantic parser:
    // intent=report, domain=savings, from_date=2026-07-01, to_date=2026-07-07,
    // quantity.mode=all.
    // Existing harness should use an embedded test index or repository fixture.
    // Submit: "show me the list of all saving activity for this month".
    // Assert state_json.classification.source == "strict_pipeline".
    // Assert execution_plan.params has no "limit" key.
}
```

- [ ] **Step 2: Run test and verify failure**

Run: `cargo test -p chat all_activity_request_does_not_apply_default_limit`
Expected: FAIL because strict evidence is not yet selecting/executing capability.

- [ ] **Step 3: Add helper to convert strict result to ClassificationResult**

In `pipeline/mod.rs`, add:

```rust
fn quantity_to_params(
    resolved: &model::ResolvedConstraints,
) -> serde_json::Map<String, serde_json::Value> {
    let mut params = serde_json::Map::new();
    params.insert("office_scope".to_string(), json!("authorized_scope"));
    if let Some(from_date) = resolved.from_date.as_ref() {
        params.insert("from_date".to_string(), json!(from_date));
    }
    if let Some(to_date) = resolved.to_date.as_ref() {
        params.insert("to_date".to_string(), json!(to_date));
    }
    if let Some(currency_code) = resolved.currency_code.as_ref() {
        params.insert("currency_code".to_string(), json!(currency_code));
    }
    if let Some(product_ids) = resolved.product_ids.as_ref() {
        params.insert("product_ids".to_string(), json!(product_ids));
    }
    match resolved.quantity.as_ref() {
        Some(model::QuantityConstraint::Limit { value })
        | Some(model::QuantityConstraint::TopN { value }) => {
            params.insert("limit".to_string(), json!(value));
        }
        Some(model::QuantityConstraint::Default) => {
            params.insert("limit".to_string(), json!(10));
        }
        Some(model::QuantityConstraint::All) | None => {}
    }
    params
}
```

- [ ] **Step 4: Select capability from evidence**

Implement a minimal selector in `retrieval.rs`:

```rust
pub fn select_capability_id(evidence: &[crate::chat::pipeline::model::RetrievalEvidence]) -> Option<String> {
    evidence
        .iter()
        .filter(|item| item.source_type == "capability")
        .max_by(|left, right| left.score.total_cmp(&right.score))
        .map(|item| item.source_id.clone())
}
```

- [ ] **Step 5: Replace temporary unsupported strict result with selected classification**

In `run_strict_pipeline`, after resolver, build retrieval plan/evidence/evaluation. Then return selected capability data. Keep this as a small vertical slice for `savings_activity_list` first:

```rust
// Build retrieval plan from parsed domain + resolved constraints.
// Run vector/keyword/graph retrieval helpers.
// Require selected capability id.
// Set params from quantity_to_params.
```

Do not add new capability-specific branches except catalog lookup by selected capability id.

- [ ] **Step 6: Run all-list integration test**

Run: `cargo test -p chat all_activity_request_does_not_apply_default_limit`
Expected: PASS.

---

### Task 10: Replace Deterministic Prose With LLM Grounded Answer

**Files:**
- Modify: `crates/chat/src/chat/service/job.rs`
- Modify: `crates/chat/src/chat/formatter/mod.rs`
- Modify: `crates/chat/src/chat/pipeline/answer.rs`
- Test: `crates/chat/src/chat/pipeline/answer.rs`, `crates/chat/tests/chat_jobs.rs`

**Interfaces:**
- Consumes: structured response JSON from existing formatter.
- Produces: assistant message whose metadata has authoritative structured payload and generated `message`.

- [ ] **Step 1: Add generated response integration assertion**

In `crates/chat/tests/chat_jobs.rs`, add assertion to the strict successful chat test:

```rust
assert_eq!(assistant.metadata_json["report_response"]["message"], "Generated grounded response from fixture LLM.");
assert!(assistant.metadata_json["report_response"]["structured"].is_object());
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test -p chat all_activity_request_does_not_apply_default_limit`
Expected: FAIL because formatter still owns final prose.

- [ ] **Step 3: Generate structured draft first, then LLM prose**

In `execute_and_finish`, after `format_report_response` returns stringified JSON, parse it to `serde_json::Value`, call `self.llm_planner.generate_answer`, validate with `validate_grounded_answer`, replace only the `message` field, and store/send the merged JSON string.

Use this exact shape:

```rust
let mut structured_response: serde_json::Value = serde_json::from_str(&content)?;
let generated = self
    .llm_planner
    .generate_answer(&plan.capability, &structured_response)
    .await?;
crate::chat::pipeline::answer::validate_grounded_answer(&structured_response, &generated)?;
if let Some(object) = structured_response.as_object_mut() {
    object.insert("message".to_string(), json!(generated.message));
    object.insert("generated_citations".to_string(), json!(generated.citations));
}
let content = serde_json::to_string(&structured_response)?;
```

If this exact code needs the original user message, retrieve it from job state or pass it through pipeline state rather than using capability.

- [ ] **Step 4: Run answer and chat tests**

Run: `cargo test -p chat chat::pipeline::answer::tests all_activity_request_does_not_apply_default_limit`
Expected: PASS.

---

### Task 11: Documentation And Verification

**Files:**
- Modify: `docs/Modern_RAG_Architecture_Blueprint.md`
- Modify: `docs/rag-architecture.md`
- Test: full focused suite

**Interfaces:**
- Consumes: implemented strict behavior.
- Produces: docs matching runtime.

- [ ] **Step 1: Update blueprint implementation status**

In `docs/Modern_RAG_Architecture_Blueprint.md`, update the dataset path section from:

```text
knowledge/domain/savings-transaction-types.yaml
```

to:

```text
knowledge/schema/fineract/enums/savings_transaction_type.yaml
```

Keep the rule that enum buckets are YAML-loaded and not hardcoded.

- [ ] **Step 2: Update RAG architecture current state**

In `docs/rag-architecture.md`, update the Blueprint Alignment Status table so strict implemented stages are marked Done only after tests pass. Keep graph retrieval as Done only if the bounded catalog traversal exists.

- [ ] **Step 3: Run formatting and tests**

Run:

```bash
cargo fmt --check
cargo test -p chat chat::pipeline
cargo test -p chat knowledge::catalog::validator
cargo test -p chat all_activity_request_does_not_apply_default_limit
```

Expected: all pass.

- [ ] **Step 4: Rebuild vector index locally**

Start the app and run:

```bash
curl -X POST http://127.0.0.1:3007/vector-index/rebuild \
  -H "Authorization: Bearer $API_KEY"
```

Expected: response `success=true`, `embedding_model="voyage-3-large"`, and `document_count` equals loaded catalog documents.

- [ ] **Step 5: Verify strict runtime manually**

Run the chat request through Postman or local HTTP:

```json
{ "message": "show me the list of all saving activity for this month" }
```

Expected:

- `state_json.strict_pipeline.parser` exists.
- `state_json.strict_pipeline.retrieval_plan` exists.
- `state_json.strict_pipeline.evidence_evaluation.enough == true`.
- `execution_plan.params.limit` is absent.
- `result_json.row_count` is greater than `10` for the current local Fineract data.
- assistant `metadata_json.report_response.message` is generated by LLM and grounded by citations.

---

## Self-Review

- Spec coverage: strict parser, router, resolver, retrieval planner, vector/keyword/graph/metadata, hybrid/reranker, evidence evaluator, answer planner, SQL policy, LLM answer generator, and grounded response are covered by tasks.
- Scope: this is large but one core pipeline. Tasks are staged so each task has an independently testable deliverable.
- Type consistency: shared types are introduced in Task 1 and consumed by later tasks.
- Known implementation risk: existing integration test harness may need a fake LLM client seam. If the current `LlmPlannerClient` is hard to fake without new crates, add a test-only constructor under `#[cfg(test)]` rather than adding a new production abstraction.
