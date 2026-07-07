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
    evidence: &[crate::chat::pipeline::model::RetrievalEvidence],
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
    use crate::chat::pipeline::model::{
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
