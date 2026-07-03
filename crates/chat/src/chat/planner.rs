use serde::{Deserialize, Serialize};
use serde_json::Value;

use app_core::auth::model::ClientContext;

use crate::chat::classifier::{ClassificationOutcome, ClassificationResult};
use crate::knowledge::model::KnowledgeCatalog;
use crate::policy::authorization::{
    effective_office_scope, ensure_capability_allowed, ensure_pii_allowed,
};

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
    pub requires_policy_check: bool,
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
        requires_policy_check: true,
    })
}

pub fn evaluate_policy(
    client: &ClientContext,
    plan: Option<&ExecutionPlan>,
    catalog: &KnowledgeCatalog,
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

    let output_requires_pii = catalog
        .queries
        .iter()
        .find(|query| query.id == plan.query_id)
        .is_some_and(|query| {
            query
                .output_fields
                .iter()
                .any(|field| field.sensitivity == "pii")
        });
    if let Err(error) = ensure_pii_allowed(client, output_requires_pii) {
        return blocked(error.to_string());
    }

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
