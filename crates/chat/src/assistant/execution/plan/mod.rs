use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use app_core::auth::model::PrincipalContext;

use crate::knowledge::dataset::model::DatasetSelection;
use crate::knowledge::model::KnowledgeCatalog;
use crate::policy::authorization::{effective_office_scope, ensure_capability_allowed};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub domain: String,
    pub capability: String,
    pub query_id: String,
    #[serde(default)]
    pub dataset_selection: Option<DatasetSelection>,
    pub output_mode: String,
    pub params: Value,
    pub retrieval_plan: RetrievalPlan,
    pub evidence_evaluation: EvidenceEvaluation,
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
