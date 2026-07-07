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
        return decision(
            false,
            Some("vector_index_stale"),
            evidence.len(),
            source_types,
        );
    }

    for required in ["capability", "query", "policy", "response"] {
        if !source_types
            .iter()
            .any(|source_type| source_type == required)
        {
            return decision(
                false,
                Some(format!("missing_required_evidence:{required}")),
                evidence.len(),
                source_types,
            );
        }
    }

    decision(true, None::<String>, evidence.len(), source_types)
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

#[cfg(test)]
mod tests {
    use super::*;

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
