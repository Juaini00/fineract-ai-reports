use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::assistant::legacy_pipeline::model::{QuantityConstraint, ResolvedConstraints};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalPlanStrict {
    pub vector_query: String,
    pub keyword_query: String,
    pub graph_query: String,
    pub metadata_filter: BTreeMap<String, String>,
}

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
    let raw: RawLayeredResponse = serde_json::from_str(content)?;
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

pub fn build_retrieval_plan(
    domain: &str,
    constraints: &ResolvedConstraints,
) -> RetrievalPlanStrict {
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

pub fn select_capability_id(
    evidence: &[crate::assistant::legacy_pipeline::model::RetrievalEvidence],
) -> Option<String> {
    evidence
        .iter()
        .filter(|item| item.source_type == "capability")
        .max_by(|left, right| left.score.total_cmp(&right.score))
        .map(|item| item.source_id.clone())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::legacy_pipeline::model::{
        QuantityConstraint, ResolvedConstraints, RetrievalEvidence,
    };

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
        let score = keyword_score(
            "savings activity",
            "list savings account activity transactions",
        );
        assert!(score > 0.0);
    }

    #[test]
    fn selects_highest_scored_capability() {
        let selected = select_capability_id(&[
            evidence("capability", "low", 0.2),
            evidence("capability", "high", 0.9),
            evidence("query", "ignored", 1.0),
        ]);
        assert_eq!(selected.as_deref(), Some("high"));
    }

    fn evidence(source_type: &str, source_id: &str, score: f32) -> RetrievalEvidence {
        RetrievalEvidence {
            source_type: source_type.to_string(),
            source_id: source_id.to_string(),
            title: source_id.to_string(),
            score,
            metadata_json: serde_json::json!({}),
        }
    }
}

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
        assert_eq!(
            plan.graph_hint.as_deref(),
            Some("client -> activation_date")
        );
        assert!((plan.confidence - 0.83).abs() < 1e-4);
    }

    #[test]
    fn rejects_missing_layer_field() {
        let json = r#"{"layers":{"domain":"x","capability":"y","query":"z"},"confidence":0.7}"#;

        assert!(parse_layered_retrieval_response(json).is_err());
    }

    #[test]
    fn rejects_out_of_range_confidence() {
        let json = r#"{"layers":{"domain":"x","capability":"y","query":"z","keyword":"k"},"confidence":1.5}"#;

        assert!(parse_layered_retrieval_response(json).is_err());
    }
}
