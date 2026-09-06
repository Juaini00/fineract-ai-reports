use std::collections::BTreeMap;

use anyhow::Result;
use app_core::auth::model::PrincipalContext;
use chrono::NaiveDate;
use serde_json::json;

use crate::assistant::llm::planner_client::LlmPlannerClient;
use crate::assistant::understanding::classifier::{ClarificationOption, ClassificationCandidate};
use crate::knowledge::embedding::VoyageEmbeddingClient;
use crate::knowledge::index::repository::{KnowledgeRepository, RetrievedKnowledgeCandidate};
use crate::knowledge::model::{KnowledgeCatalog, LqrPolicy, ScoreAggregation};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LayerTrace {
    pub layer: String,
    pub winner: Option<String>,
    pub confidence: f32,
    pub candidates: Vec<ClassificationCandidate>,
}

pub struct LqrInputs<'a> {
    pub message: &'a str,
    pub client: &'a PrincipalContext,
    pub llm: &'a LlmPlannerClient,
    pub embedding_client: &'a VoyageEmbeddingClient,
    pub repository: &'a KnowledgeRepository,
    pub catalog: &'a KnowledgeCatalog,
    pub today: NaiveDate,
}

#[derive(Debug, Clone)]
pub enum LqrOutcome {
    Matched {
        capability_id: String,
        confidence: f32,
    },
    Ambiguous {
        options: Vec<ClarificationOption>,
        confidence: f32,
    },
    Unsupported {
        reason: String,
    },
}

pub struct LqrResult {
    pub outcome: LqrOutcome,
    pub layers: Vec<LayerTrace>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DomainDecision {
    Winner { domain_id: String, confidence: f32 },
    Ambiguous { top: Vec<String> },
    Reject { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CapabilityDecision {
    Winner {
        capability_id: String,
        confidence: f32,
    },
    Ambiguous {
        top: Vec<(String, f32)>,
    },
    Reject {
        reason: String,
    },
}

pub fn decide_domain_layer(policy: &LqrPolicy, ranked: &[(String, String, f32)]) -> DomainDecision {
    let Some((top_id, top_status, top_conf)) = ranked.first().cloned() else {
        return DomainDecision::Reject {
            reason: "no_domain_match".to_string(),
        };
    };
    if top_conf < policy.domain_min_floor {
        return DomainDecision::Reject {
            reason: "no_domain_match".to_string(),
        };
    }
    if matches!(top_status.as_str(), "deferred" | "rejected") {
        return DomainDecision::Reject {
            reason: format!("off_domain_{top_id}"),
        };
    }
    let second_conf = ranked.get(1).map(|item| item.2).unwrap_or(0.0);
    if top_conf - second_conf < policy.domain_min_gap {
        return DomainDecision::Ambiguous {
            top: ranked.iter().take(3).map(|item| item.0.clone()).collect(),
        };
    }
    DomainDecision::Winner {
        domain_id: top_id,
        confidence: top_conf,
    }
}

pub fn aggregate_confidence(mode: &ScoreAggregation, values: &[f32]) -> f32 {
    match mode {
        ScoreAggregation::Min => values.iter().copied().fold(f32::INFINITY, f32::min),
        ScoreAggregation::Mean => {
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f32>() / values.len() as f32
            }
        }
        ScoreAggregation::Product => values.iter().copied().fold(1.0, |acc, value| acc * value),
    }
}

pub fn decide_capability_layer(policy: &LqrPolicy, ranked: &[(String, f32)]) -> CapabilityDecision {
    let Some((top_id, top_conf)) = ranked.first().cloned() else {
        return CapabilityDecision::Reject {
            reason: "no_capability_match".to_string(),
        };
    };
    if top_conf < policy.capability_min_floor {
        return CapabilityDecision::Reject {
            reason: "no_capability_match".to_string(),
        };
    }
    let second_conf = ranked.get(1).map(|item| item.1).unwrap_or(0.0);
    if top_conf - second_conf < policy.capability_min_gap {
        return CapabilityDecision::Ambiguous {
            top: ranked.iter().take(3).cloned().collect(),
        };
    }
    CapabilityDecision::Winner {
        capability_id: top_id,
        confidence: top_conf,
    }
}

pub async fn run_layered_retrieval(inputs: LqrInputs<'_>) -> Result<LqrResult> {
    let policy = &inputs.catalog.classification.lqr;
    let _today = inputs.today;
    let plan = inputs
        .llm
        .plan_layered_retrieval(
            inputs.message,
            &json!({ "allowed_capabilities": inputs.client.capability_ids }),
        )
        .await?;

    let domain_hits = inputs
        .repository
        .search_hybrid_by_source_type(
            "domain",
            inputs.embedding_client.embed_query(&plan.domain).await?,
            &split_terms(&plan.keyword),
            None,
            &BTreeMap::new(),
            5,
        )
        .await?;
    let all_ranked = ranked_domains(&domain_hits);
    let reachable = reachable_domains(inputs.catalog, &inputs.client.capability_ids);
    let ranked_domains: Vec<(String, String, f32)> = if reachable.is_empty() {
        all_ranked.clone()
    } else {
        all_ranked
            .iter()
            .filter(|(id, _, _)| reachable.contains(id))
            .cloned()
            .collect()
    };
    let mut layers = vec![domain_trace(&ranked_domains)];
    let (domain_id, domain_confidence) = match decide_domain_layer(policy, &ranked_domains) {
        DomainDecision::Winner {
            domain_id,
            confidence,
        } => (domain_id, confidence),
        DomainDecision::Ambiguous { .. } => {
            // All candidates were already filtered to reachable domains, so
            // ambiguity here only means Voyage scores clustered tightly — not
            // that the request is off-domain. Advance with the top domain and
            // let the capability layer + allowed_capabilities disambiguate.
            let (top_id, _, top_conf) = ranked_domains[0].clone();
            (top_id, top_conf)
        }
        DomainDecision::Reject { reason } => {
            return Ok(LqrResult {
                outcome: LqrOutcome::Unsupported { reason },
                layers,
            });
        }
    };

    let mut metadata = BTreeMap::new();
    metadata.insert("domain".to_string(), domain_id);
    let capability_hits = inputs
        .repository
        .search_hybrid_by_source_type(
            "capability",
            inputs
                .embedding_client
                .embed_query(&plan.capability)
                .await?,
            &split_terms(&plan.keyword),
            Some(&inputs.client.capability_ids),
            &metadata,
            6,
        )
        .await?;
    let ranked_capabilities = ranked_capabilities(&capability_hits);
    layers.push(capability_trace(&ranked_capabilities));

    Ok(
        match decide_capability_layer(policy, &ranked_capabilities) {
            CapabilityDecision::Winner {
                capability_id,
                confidence,
            } => {
                let query_id = capability_query_id(inputs.catalog, &capability_id)
                    .unwrap_or_else(|| capability_id.clone());
                layers.push(query_trace(&query_id));
                LqrResult {
                    outcome: LqrOutcome::Matched {
                        capability_id,
                        confidence: aggregate_confidence(
                            &policy.score_aggregation,
                            &[domain_confidence, confidence],
                        ),
                    },
                    layers,
                }
            }
            CapabilityDecision::Ambiguous { top } => LqrResult {
                outcome: LqrOutcome::Ambiguous {
                    options: top
                        .into_iter()
                        .map(|(capability, _)| ClarificationOption {
                            label: capability_label(inputs.catalog, &capability),
                            capability,
                            output_mode: None,
                        })
                        .collect(),
                    confidence: domain_confidence,
                },
                layers,
            },
            CapabilityDecision::Reject { reason } => LqrResult {
                outcome: LqrOutcome::Unsupported { reason },
                layers,
            },
        },
    )
}

fn reachable_domains(
    catalog: &KnowledgeCatalog,
    allowed_capabilities: &[String],
) -> std::collections::HashSet<String> {
    let allowed: std::collections::HashSet<&str> =
        allowed_capabilities.iter().map(String::as_str).collect();
    catalog
        .capabilities
        .iter()
        .filter(|capability| allowed.contains(capability.id.as_str()))
        .map(|capability| capability.domain.clone())
        .collect()
}

fn ranked_domains(hits: &[RetrievedKnowledgeCandidate]) -> Vec<(String, String, f32)> {
    hits.iter()
        .map(|candidate| {
            let status = candidate
                .metadata_json
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("approved_mvp")
                .to_string();
            (
                candidate.source_id.clone(),
                status,
                distance_to_confidence(candidate.distance),
            )
        })
        .collect()
}

fn ranked_capabilities(hits: &[RetrievedKnowledgeCandidate]) -> Vec<(String, f32)> {
    hits.iter()
        .map(|candidate| {
            (
                candidate.source_id.clone(),
                distance_to_confidence(candidate.distance),
            )
        })
        .collect()
}

fn domain_trace(ranked: &[(String, String, f32)]) -> LayerTrace {
    LayerTrace {
        layer: "domain".to_string(),
        winner: ranked.first().map(|item| item.0.clone()),
        confidence: ranked.first().map(|item| item.2).unwrap_or(0.0),
        candidates: ranked
            .iter()
            .map(|(id, _, confidence)| ClassificationCandidate {
                capability: id.clone(),
                confidence: *confidence,
                source_type: Some("domain".to_string()),
            })
            .collect(),
    }
}

fn capability_trace(ranked: &[(String, f32)]) -> LayerTrace {
    LayerTrace {
        layer: "capability".to_string(),
        winner: ranked.first().map(|item| item.0.clone()),
        confidence: ranked.first().map(|item| item.1).unwrap_or(0.0),
        candidates: ranked
            .iter()
            .map(|(id, confidence)| ClassificationCandidate {
                capability: id.clone(),
                confidence: *confidence,
                source_type: Some("capability".to_string()),
            })
            .collect(),
    }
}

fn query_trace(query_id: &str) -> LayerTrace {
    LayerTrace {
        layer: "query".to_string(),
        winner: Some(query_id.to_string()),
        confidence: 1.0,
        candidates: vec![ClassificationCandidate {
            capability: query_id.to_string(),
            confidence: 1.0,
            source_type: Some("query".to_string()),
        }],
    }
}

fn distance_to_confidence(distance: f64) -> f32 {
    (1.0_f32 - (distance as f32 / 2.0)).clamp(0.0, 1.0)
}

fn split_terms(value: &str) -> Vec<String> {
    value.split_whitespace().map(str::to_string).collect()
}

fn capability_label(catalog: &KnowledgeCatalog, capability_id: &str) -> String {
    catalog
        .capabilities
        .iter()
        .find(|capability| capability.id == capability_id)
        .and_then(|capability| capability.display_name.clone())
        .unwrap_or_else(|| capability_id.to_string())
}

fn capability_query_id(catalog: &KnowledgeCatalog, capability_id: &str) -> Option<String> {
    catalog
        .capabilities
        .iter()
        .find(|capability| capability.id == capability_id)
        .map(|capability| capability.query_id.clone())
}

#[cfg(test)]
mod tests {
    use crate::knowledge::model::{LqrPolicy, ScoreAggregation};

    fn policy() -> LqrPolicy {
        LqrPolicy::default()
    }

    #[test]
    fn domain_reject_when_top_below_floor() {
        let ranked = vec![("client".into(), "approved_mvp".into(), 0.30)];

        let decision = super::decide_domain_layer(&policy(), &ranked);

        assert!(matches!(decision, super::DomainDecision::Reject { .. }));
    }

    #[test]
    fn domain_reject_when_top_is_deferred() {
        let ranked = vec![
            ("loan".into(), "deferred".into(), 0.82),
            ("savings".into(), "approved_mvp".into(), 0.55),
        ];

        let decision = super::decide_domain_layer(&policy(), &ranked);

        match decision {
            super::DomainDecision::Reject { reason } => {
                assert!(reason.starts_with("off_domain_loan"))
            }
            _ => panic!("expected reject"),
        }
    }

    #[test]
    fn domain_ambiguous_when_gap_small() {
        let ranked = vec![
            ("client".into(), "approved_mvp".into(), 0.72),
            ("savings".into(), "approved_mvp".into(), 0.68),
        ];

        let decision = super::decide_domain_layer(&policy(), &ranked);

        assert!(matches!(decision, super::DomainDecision::Ambiguous { .. }));
    }

    #[test]
    fn domain_winner_when_gap_wide() {
        let ranked = vec![
            ("client".into(), "approved_mvp".into(), 0.82),
            ("savings".into(), "approved_mvp".into(), 0.60),
        ];

        let decision = super::decide_domain_layer(&policy(), &ranked);

        match decision {
            super::DomainDecision::Winner { domain_id, .. } => assert_eq!(domain_id, "client"),
            _ => panic!("expected winner"),
        }
    }

    #[test]
    fn final_confidence_uses_selected_aggregation() {
        assert!(
            (super::aggregate_confidence(&ScoreAggregation::Min, &[0.90, 0.71]) - 0.71).abs()
                < 1e-6
        );
        assert!(
            (super::aggregate_confidence(&ScoreAggregation::Mean, &[0.90, 0.70]) - 0.80).abs()
                < 1e-6
        );
        assert!(
            (super::aggregate_confidence(&ScoreAggregation::Product, &[0.9, 0.8]) - 0.72).abs()
                < 1e-6
        );
    }

    #[test]
    fn capability_reject_below_floor() {
        let ranked = vec![("cap_a".into(), 0.30)];

        let decision = super::decide_capability_layer(&policy(), &ranked);

        assert!(matches!(decision, super::CapabilityDecision::Reject { .. }));
    }

    #[test]
    fn capability_winner_wide_gap() {
        let ranked = vec![("cap_a".into(), 0.80), ("cap_b".into(), 0.50)];

        let decision = super::decide_capability_layer(&policy(), &ranked);

        match decision {
            super::CapabilityDecision::Winner { capability_id, .. } => {
                assert_eq!(capability_id, "cap_a")
            }
            _ => panic!("expected winner"),
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

        let decision = super::decide_capability_layer(&policy(), &ranked);

        match decision {
            super::CapabilityDecision::Ambiguous { top } => assert_eq!(top.len(), 3),
            _ => panic!("expected ambiguous"),
        }
    }
}
