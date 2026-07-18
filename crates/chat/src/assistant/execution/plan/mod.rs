use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use app_core::auth::model::PrincipalContext;

use crate::assistant::understanding::classifier::{ClassificationOutcome, ClassificationResult};
use crate::knowledge::model::KnowledgeCatalog;
use crate::policy::authorization::{effective_office_scope, ensure_capability_allowed};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPlanType {
    Atomic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub plan_type: ExecutionPlanType,
    pub domain: String,
    pub capability: String,
    pub query_id: String,
    pub output_mode: String,
    pub params: Value,
    pub retrieval_plan: RetrievalPlan,
    pub evidence_evaluation: EvidenceEvaluation,
    pub answer_plan: AnswerPlan,
    pub requires_policy_check: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RetrievalPlan {
    pub vector_query: String,
    pub keyword_query: String,
    pub graph_query: String,
    pub metadata_filter: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EvidenceEvaluation {
    pub enough: bool,
    pub source_count: usize,
    pub source_types: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnswerPlan {
    pub sections: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecisionStatus {
    Allowed,
    Blocked,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub status: PolicyDecisionStatus,
    pub reason: Option<String>,
    pub office_ids: Vec<i64>,
    pub can_view_pii: bool,
}

pub fn build_execution_plan(
    classification: &ClassificationResult,
    catalog: &KnowledgeCatalog,
) -> Option<ExecutionPlan> {
    if classification.outcome != ClassificationOutcome::Matched {
        return None;
    }

    let capability = classification.capability.as_deref()?;
    let capability_knowledge = catalog
        .capabilities
        .iter()
        .find(|item| item.id == capability && item.status == "approved_mvp")?;
    let query = catalog
        .queries
        .iter()
        .find(|item| item.id == capability_knowledge.query_id)?;

    Some(ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: capability_knowledge.domain.clone(),
        capability: capability.to_string(),
        query_id: query.id.clone(),
        output_mode: capability_knowledge.output_mode.clone(),
        params: classification.params.clone(),
        retrieval_plan: build_retrieval_plan(classification, capability_knowledge),
        evidence_evaluation: evaluate_evidence(classification, catalog),
        answer_plan: build_answer_plan(&capability_knowledge.output_mode),
        requires_policy_check: true,
    })
}

fn build_retrieval_plan(
    classification: &ClassificationResult,
    capability: &crate::knowledge::model::CapabilityKnowledge,
) -> RetrievalPlan {
    let mut metadata_filter = BTreeMap::new();
    metadata_filter.insert("domain".to_string(), capability.domain.clone());
    metadata_filter.insert("capability".to_string(), capability.id.clone());
    metadata_filter.insert("query_id".to_string(), capability.query_id.clone());
    metadata_filter.insert("output_mode".to_string(), capability.output_mode.clone());
    if !capability.data_areas.is_empty() {
        metadata_filter.insert("data_areas".to_string(), capability.data_areas.join(","));
    }

    RetrievalPlan {
        vector_query: compact_terms([
            capability.domain.as_str(),
            capability.id.as_str(),
            capability.display_name.as_deref().unwrap_or(""),
            capability.description.as_deref().unwrap_or(""),
        ]),
        keyword_query: compact_terms(
            capability
                .metrics
                .iter()
                .map(String::as_str)
                .chain(capability.examples.iter().map(String::as_str))
                .chain(param_terms(&classification.params)),
        ),
        graph_query: format!(
            "{} -> {} -> {}",
            capability.domain, capability.id, capability.query_id
        ),
        metadata_filter,
    }
}

fn evaluate_evidence(
    classification: &ClassificationResult,
    catalog: &KnowledgeCatalog,
) -> EvidenceEvaluation {
    let source_count = classification.candidates.len();
    let mut source_types = classification
        .candidates
        .iter()
        .filter_map(|candidate| candidate.source_type.clone())
        .collect::<Vec<_>>();
    source_types.sort();
    source_types.dedup();

    let enough = classification.confidence >= catalog.classification.min_floor
        && classification.capability.is_some();
    EvidenceEvaluation {
        enough,
        source_count,
        source_types,
        reason: (!enough).then(|| "insufficient retrieval evidence".to_string()),
    }
}

fn build_answer_plan(output_mode: &str) -> AnswerPlan {
    let sections = match output_mode {
        "list" => ["Result", "Scope", "Evidence"].as_slice(),
        "top_n" | "monthly_top_n" => ["Ranking", "Scope", "Evidence"].as_slice(),
        "monthly_breakdown" => ["Monthly Breakdown", "Scope", "Evidence"].as_slice(),
        _ => ["Summary", "Scope", "Evidence"].as_slice(),
    };
    AnswerPlan {
        sections: sections
            .iter()
            .map(|section| (*section).to_string())
            .collect(),
    }
}

fn compact_terms<'a>(terms: impl IntoIterator<Item = &'a str>) -> String {
    terms
        .into_iter()
        .flat_map(|term| term.split(|character: char| !character.is_alphanumeric()))
        .filter(|term| term.len() >= 2)
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn param_terms(params: &Value) -> Vec<&str> {
    params
        .as_object()
        .map(|params| {
            params
                .values()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn evaluate_policy(
    client: &PrincipalContext,
    plan: Option<&ExecutionPlan>,
    _catalog: &KnowledgeCatalog,
) -> PolicyDecision {
    let Some(plan) = plan else {
        return PolicyDecision {
            status: PolicyDecisionStatus::NotApplicable,
            reason: None,
            office_ids: Vec::new(),
            can_view_pii: false,
        };
    };

    if let Err(error) = ensure_capability_allowed(client, &plan.capability) {
        return blocked(error.to_string());
    }

    let office_ids = match effective_office_scope(client, None) {
        Ok(office_ids) => office_ids,
        Err(error) => return blocked(error.to_string()),
    };

    PolicyDecision {
        status: PolicyDecisionStatus::Allowed,
        reason: None,
        office_ids,
        can_view_pii: client.can_view_pii,
    }
}

fn blocked(reason: String) -> PolicyDecision {
    PolicyDecision {
        status: PolicyDecisionStatus::Blocked,
        reason: Some(reason),
        office_ids: Vec::new(),
        can_view_pii: false,
    }
}

#[cfg(test)]
mod tests;
