use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::knowledge::catalog::{loader::KnowledgeLoader, validator::KnowledgeValidator};

/// A minimal `ExecutionPlan` for a capability — enough for `evaluate_policy`,
/// which only reads `plan.capability` (capability grant) and the principal's
/// office scope. Constructed directly rather than through a planner: these are
/// policy/security contract tests, not planner-shape tests.
fn plan_for(capability: &str) -> ExecutionPlan {
    ExecutionPlan {
        domain: "savings".to_string(),
        capability: capability.to_string(),
        query_id: "savings.deposit_total".to_string(),
        dataset_selection: None,
        output_mode: "table".to_string(),
        params: json!({}),
        retrieval_plan: RetrievalPlan::default(),
        evidence_evaluation: EvidenceEvaluation::default(),
        requires_policy_check: true,
    }
}

#[test]
fn allows_policy_for_configured_client() {
    let catalog = catalog();
    let plan = plan_for("savings_deposit_total");
    let decision = evaluate_policy(&client(), Some(&plan), &catalog);

    assert_eq!(decision.status, PolicyDecisionStatus::Allowed);
    assert_eq!(decision.office_ids, vec![1, 2]);
}

#[test]
fn blocks_policy_for_missing_capability() {
    let mut client = client();
    client.capability_ids.clear();
    let catalog = catalog();
    let plan = plan_for("savings_deposit_total");
    let decision = evaluate_policy(&client, Some(&plan), &catalog);

    assert_eq!(decision.status, PolicyDecisionStatus::Blocked);
}

#[test]
fn policy_reflects_principal_pii_visibility() {
    let mut client = client();
    client
        .capability_ids
        .push("savings_deposit_top_n".to_string());
    let catalog = catalog();
    let plan = plan_for("savings_deposit_top_n");
    let decision = evaluate_policy(&client, Some(&plan), &catalog);

    assert_eq!(decision.status, PolicyDecisionStatus::Allowed);
    assert!(!decision.can_view_pii);
}

#[test]
fn absent_plan_is_not_applicable() {
    let catalog = catalog();
    let decision = evaluate_policy(&client(), None, &catalog);
    assert_eq!(decision.status, PolicyDecisionStatus::NotApplicable);
}

fn client() -> PrincipalContext {
    PrincipalContext {
        user_id: Uuid::nil(),
        role: "admin".to_string(),
        office_ids: vec![1, 2],
        capability_ids: vec!["savings_deposit_total".to_string()],
        can_view_pii: false,
        legacy_api_key_id: None,
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
