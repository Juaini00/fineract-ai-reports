use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    assistant::{AssistantDomain, AssistantIntent, AssistantIntentKind},
    knowledge::index::repository::RetrievedKnowledgeCandidate,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RetrievalPlan {
    pub query_text: String,
    pub domain: AssistantDomain,
    pub intent: AssistantIntentKind,
    pub allow_all_capabilities: bool,
    pub allowed_capabilities: Vec<String>,
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
            allow_all_capabilities,
            allowed_capabilities,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    pub capability_id: String,
    pub title: String,
    pub score: f32,
    pub source_type: String,
    pub metadata: serde_json::Value,
}

impl From<RetrievedKnowledgeCandidate> for Evidence {
    fn from(candidate: RetrievedKnowledgeCandidate) -> Self {
        Self {
            capability_id: candidate.source_id,
            title: candidate.title,
            score: (1.0 - candidate.distance as f32).clamp(0.0, 1.0),
            source_type: candidate.source_type,
            metadata: candidate.metadata_json,
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
            AssistantIntentKind::UnsupportedInDomain | AssistantIntentKind::UnsafeRequest => {
                EvidenceDecision::UnsupportedInDomain
            }
            AssistantIntentKind::ReportRequest
            | AssistantIntentKind::DataLookup
            | AssistantIntentKind::FollowUp => evidence
                .first()
                .filter(|item| item.score >= self.strong_capability_score)
                .map(|item| EvidenceDecision::Select {
                    capability_id: item.capability_id.clone(),
                })
                .unwrap_or(EvidenceDecision::Clarify),
            _ => EvidenceDecision::Clarify,
        }
    }
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
