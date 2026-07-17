use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    assistant::{
        AssistantDomain, AssistantEntity, AssistantEntityType, AssistantIntent,
        AssistantIntentKind, RequestShape,
    },
    knowledge::index::repository::RetrievedKnowledgeCandidate,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RetrievalPlan {
    pub query_text: String,
    pub domain: AssistantDomain,
    pub intent: AssistantIntentKind,
    pub request_shape: RequestShape,
    #[serde(default)]
    pub entities: Vec<AssistantEntity>,
    #[serde(default)]
    pub constraints: serde_json::Value,
    #[serde(default)]
    pub metadata_filters: std::collections::BTreeMap<String, String>,
    pub allow_all_capabilities: bool,
    pub allowed_capabilities: Vec<String>,
    #[serde(default)]
    pub source_snippets: Vec<String>,
}

impl RetrievalPlan {
    pub fn new(
        query_text: impl Into<String>,
        intent: &AssistantIntent,
        allow_all_capabilities: bool,
        allowed_capabilities: Vec<String>,
    ) -> Self {
        Self {
            query_text: query_text.into(),
            domain: intent.domain.clone(),
            intent: intent.intent.clone(),
            request_shape: intent.request_shape.clone(),
            entities: intent.entities.clone(),
            constraints: serde_json::to_value(&intent.constraints).unwrap_or_default(),
            metadata_filters: domain_filter(&intent.domain),
            allow_all_capabilities,
            allowed_capabilities,
            source_snippets: Vec::new(),
        }
    }
}

fn domain_filter(domain: &AssistantDomain) -> std::collections::BTreeMap<String, String> {
    let mut filters = std::collections::BTreeMap::new();
    if !matches!(domain, AssistantDomain::Unknown) {
        filters.insert("domain".into(), format!("{:?}", domain).to_lowercase());
    }
    filters
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    pub capability_id: String,
    pub title: String,
    pub score: f32,
    pub source_type: String,
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub conflicting: bool,
}

impl From<RetrievedKnowledgeCandidate> for Evidence {
    fn from(candidate: RetrievedKnowledgeCandidate) -> Self {
        Self {
            capability_id: candidate.source_id,
            title: candidate.title,
            score: (1.0 - candidate.distance as f32).clamp(0.0, 1.0),
            source_type: candidate.source_type,
            metadata: candidate.metadata_json,
            conflicting: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum EvidenceDecision {
    Select { capability_id: String },
    Clarify,
    UnsupportedInDomain,
    OutOfDomain,
    BlockedByPolicy,
}

pub struct EvidenceEvaluator;

impl Default for EvidenceEvaluator {
    fn default() -> Self {
        Self
    }
}

impl EvidenceEvaluator {
    pub fn evaluate(&self, plan: &RetrievalPlan, evidence: &[Evidence]) -> EvidenceDecision {
        match plan.intent {
            AssistantIntentKind::OutOfDomain => EvidenceDecision::OutOfDomain,
            AssistantIntentKind::UnsafeRequest => EvidenceDecision::BlockedByPolicy,
            AssistantIntentKind::UnsupportedInDomain => EvidenceDecision::UnsupportedInDomain,
            AssistantIntentKind::ReportRequest
            | AssistantIntentKind::DataLookup
            | AssistantIntentKind::FollowUp => {
                let allowed = evidence
                    .iter()
                    .filter(|item| is_allowed(plan, item))
                    .collect::<Vec<_>>();
                const MIN_SELECT_SCORE: f32 = 0.25;
                let has_conflict = evidence.iter().any(|item| item.conflicting);
                let has_metric_entity = plan
                    .entities
                    .iter()
                    .any(|entity| entity.entity_type == AssistantEntityType::Metric);
                if allowed.is_empty() {
                    EvidenceDecision::UnsupportedInDomain
                } else if has_conflict || allowed[0].score < MIN_SELECT_SCORE {
                    EvidenceDecision::Clarify
                } else if allowed.len() == 1 {
                    EvidenceDecision::Select {
                        capability_id: allowed[0].capability_id.clone(),
                    }
                } else if allowed[0].score - allowed[1].score <= 0.05 || !has_metric_entity {
                    EvidenceDecision::Clarify
                } else {
                    EvidenceDecision::Select {
                        capability_id: allowed[0].capability_id.clone(),
                    }
                }
            }
            _ => EvidenceDecision::Clarify,
        }
    }
}

fn is_allowed(plan: &RetrievalPlan, evidence: &Evidence) -> bool {
    plan.allow_all_capabilities
        || plan
            .allowed_capabilities
            .iter()
            .any(|id| id == &evidence.capability_id)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::assistant::{AssistantLanguage, ContextReference};

    fn plan(intent: AssistantIntentKind) -> RetrievalPlan {
        RetrievalPlan::new(
            "show savings",
            &AssistantIntent {
                intent,
                domain: AssistantDomain::Savings,
                request_shape: Default::default(),
                language: AssistantLanguage::En,
                entities: Vec::new(),
                constraints: Default::default(),
                context_reference: ContextReference::None,
                source: None,
                confidence: 0.9,
                reason: "test".into(),
            },
            false,
            vec!["savings_deposit_total".into()],
        )
    }

    fn evidence(score: f32) -> Evidence {
        Evidence {
            capability_id: "savings_deposit_total".into(),
            title: "Savings deposit total".into(),
            score,
            source_type: "capability".into(),
            metadata: json!({}),
            conflicting: false,
        }
    }

    #[test]
    fn strong_evidence_selects_capability() {
        assert_eq!(
            EvidenceEvaluator.evaluate(&plan(AssistantIntentKind::ReportRequest), &[evidence(0.8)]),
            EvidenceDecision::Select {
                capability_id: "savings_deposit_total".into()
            }
        );
    }

    #[test]
    fn weak_evidence_clarifies() {
        assert_eq!(
            EvidenceEvaluator.evaluate(&plan(AssistantIntentKind::DataLookup), &[evidence(0.2)]),
            EvidenceDecision::Clarify
        );
    }

    #[test]
    fn out_of_domain_stays_out_of_domain() {
        assert_eq!(
            EvidenceEvaluator.evaluate(&plan(AssistantIntentKind::OutOfDomain), &[evidence(0.9)]),
            EvidenceDecision::OutOfDomain
        );
    }
}
