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

pub fn evaluate_policy(client: &ClientContext, plan: Option<&ExecutionPlan>) -> PolicyDecision {
    let Some(plan) = plan else {
        return PolicyDecision {
            status: PolicyDecisionStatus::NotApplicable,
            reason: None,
            office_ids: Vec::new(),
        };
    };

    if let Err(error) = ensure_capability_allowed(client, &plan.capability) {
        return blocked(error.to_string());
    }

    let office_ids = match effective_office_scope(client, None) {
        Ok(office_ids) => office_ids,
        Err(error) => return blocked(error.to_string()),
    };

    if let Err(error) = ensure_pii_allowed(client, plan.output_mode == "top_n") {
        return blocked(error.to_string());
    }

    PolicyDecision {
        status: PolicyDecisionStatus::Allowed,
        reason: None,
        office_ids,
    }
}

fn blocked(reason: String) -> PolicyDecision {
    PolicyDecision {
        status: PolicyDecisionStatus::Blocked,
        reason: Some(reason),
        office_ids: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use uuid::Uuid;

    use super::*;
    use crate::chat::classifier::classify_message;
    use crate::knowledge::catalog::{loader::KnowledgeLoader, validator::KnowledgeValidator};

    #[test]
    fn builds_atomic_plan_for_total_deposit() {
        let classification = classify_message(
            "How much is the total deposit today?",
            NaiveDate::from_ymd_opt(2026, 6, 21).unwrap(),
        );

        let catalog = catalog();
        let plan = build_execution_plan(&classification, &catalog).expect("execution plan");

        assert_eq!(plan.plan_type, ExecutionPlanType::Atomic);
        assert_eq!(plan.capability, "savings_deposit_total");
        assert_eq!(plan.query_id, "savings.deposit_total");
        assert!(plan.requires_policy_check);
    }

    #[test]
    fn skips_plan_when_clarification_required() {
        let classification = classify_message(
            "How much is the total deposit?",
            NaiveDate::from_ymd_opt(2026, 6, 21).unwrap(),
        );

        let catalog = catalog();
        assert!(build_execution_plan(&classification, &catalog).is_none());
    }

    #[test]
    fn allows_policy_for_configured_client() {
        let classification = classify_message(
            "How much is the total deposit today?",
            NaiveDate::from_ymd_opt(2026, 6, 21).unwrap(),
        );
        let catalog = catalog();
        let plan = build_execution_plan(&classification, &catalog);
        let decision = evaluate_policy(&client(), plan.as_ref());

        assert_eq!(decision.status, PolicyDecisionStatus::Allowed);
        assert_eq!(decision.office_ids, vec![1, 2]);
    }

    #[test]
    fn blocks_policy_for_missing_capability() {
        let mut client = client();
        client.allowed_capabilities.clear();
        let classification = classify_message(
            "How much is the total deposit today?",
            NaiveDate::from_ymd_opt(2026, 6, 21).unwrap(),
        );
        let catalog = catalog();
        let plan = build_execution_plan(&classification, &catalog);
        let decision = evaluate_policy(&client, plan.as_ref());

        assert_eq!(decision.status, PolicyDecisionStatus::Blocked);
    }

    fn client() -> ClientContext {
        ClientContext {
            api_key_id: Uuid::nil(),
            name: "test".to_string(),
            owner: "test".to_string(),
            key_prefix: "air_test".to_string(),
            allowed_office_ids: vec![1, 2],
            allowed_capabilities: vec!["savings_deposit_total".to_string()],
            can_view_pii: false,
            expires_at: None,
        }
    }

    fn catalog() -> KnowledgeCatalog {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .and_then(|path| path.parent())
            .unwrap();
        let catalog = KnowledgeLoader::new(
            workspace_root.join("knowledge"),
            workspace_root.join("queries"),
        )
        .load()
        .unwrap();

        KnowledgeValidator::validate(&catalog).unwrap();
        catalog
    }
}
