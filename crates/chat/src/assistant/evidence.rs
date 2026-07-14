use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    assistant::{AssistantDomain, AssistantEntity, AssistantIntent, AssistantIntentKind},
    knowledge::index::repository::RetrievedKnowledgeCandidate,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RetrievalPlan {
    pub query_text: String,
    pub domain: AssistantDomain,
    pub intent: AssistantIntentKind,
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

pub struct EvidenceEvaluator {
    strong_capability_score: f32,
}

impl Default for EvidenceEvaluator {
    fn default() -> Self {
        Self {
            strong_capability_score: 0.60,
        }
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
                metric_match(plan, &allowed)
                    .or_else(|| {
                        allowed
                            .as_slice()
                            .first()
                            .filter(|_| !evidence.iter().any(|item| item.conflicting))
                            .filter(|item| item.score >= self.strong_capability_score)
                            .map(|item| EvidenceDecision::Select {
                                capability_id: item.capability_id.clone(),
                            })
                    })
                    .unwrap_or(if evidence.is_empty() {
                        EvidenceDecision::UnsupportedInDomain
                    } else {
                        EvidenceDecision::Clarify
                    })
            }
            _ => EvidenceDecision::Clarify,
        }
    }
}

fn metric_match(plan: &RetrievalPlan, evidence: &[&Evidence]) -> Option<EvidenceDecision> {
    let terms = plan
        .entities
        .iter()
        .filter(|entity| {
            matches!(
                entity.entity_type,
                crate::assistant::AssistantEntityType::Metric
            )
        })
        .flat_map(|entity| {
            entity
                .canonical
                .as_deref()
                .unwrap_or(&entity.value)
                .split(|ch: char| !ch.is_alphanumeric())
                .filter(|part| part.len() > 2)
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return None;
    }
    evidence
        .iter()
        .copied()
        .find(|item| terms.iter().all(|term| item.capability_id.contains(term)))
        .map(|item| EvidenceDecision::Select {
            capability_id: item.capability_id.clone(),
        })
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
            EvidenceEvaluator::default()
                .evaluate(&plan(AssistantIntentKind::ReportRequest), &[evidence(0.8)]),
            EvidenceDecision::Select {
                capability_id: "savings_deposit_total".into()
            }
        );
    }

    #[test]
    fn weak_evidence_clarifies() {
        assert_eq!(
            EvidenceEvaluator::default()
                .evaluate(&plan(AssistantIntentKind::DataLookup), &[evidence(0.2)]),
            EvidenceDecision::Clarify
        );
    }

    #[test]
    fn out_of_domain_stays_out_of_domain() {
        assert_eq!(
            EvidenceEvaluator::default()
                .evaluate(&plan(AssistantIntentKind::OutOfDomain), &[evidence(0.9)]),
            EvidenceDecision::OutOfDomain
        );
    }
}
