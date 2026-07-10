use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::chat::classifier::ClassificationCandidate;
use crate::knowledge::catalog::{loader::KnowledgeLoader, validator::KnowledgeValidator};

#[test]
fn builds_atomic_plan_for_total_deposit() {
    let classification = matched_total_deposit();

    let catalog = catalog();
    let plan = build_execution_plan(&classification, &catalog).expect("execution plan");

    assert_eq!(plan.plan_type, ExecutionPlanType::Atomic);
    assert_eq!(plan.capability, "savings_deposit_total");
    assert_eq!(plan.query_id, "savings.deposit_total");
    assert!(plan.requires_policy_check);
}

#[test]
fn plan_contains_modern_rag_stage_outputs() {
    let mut classification = matched_total_deposit();
    classification.candidates = vec![ClassificationCandidate {
        capability: "savings_deposit_total".to_string(),
        confidence: 0.86,
        source_type: Some("capability".to_string()),
    }];

    let catalog = catalog();
    let plan = build_execution_plan(&classification, &catalog).expect("execution plan");

    assert!(plan.retrieval_plan.vector_query.contains("savings"));
    assert_eq!(
        plan.retrieval_plan.metadata_filter.get("capability"),
        Some(&"savings_deposit_total".to_string())
    );
    assert!(plan.evidence_evaluation.enough);
    assert_eq!(plan.evidence_evaluation.source_types, vec!["capability"]);
    assert_eq!(
        plan.answer_plan.sections,
        vec!["Summary", "Scope", "Evidence"]
    );
}

#[test]
fn skips_plan_when_clarification_required() {
    let classification = ClassificationResult {
        outcome: ClassificationOutcome::ClarificationRequired,
        domain: Some("savings".to_string()),
        capability: None,
        confidence: 0.5,
        params: json!({}),
        clarification: Some("Please choose one of the available report options.".to_string()),
        options: Vec::new(),
        source: Some("test".to_string()),
        candidates: Vec::new(),
        layers: Vec::new(),
    };

    let catalog = catalog();
    assert!(build_execution_plan(&classification, &catalog).is_none());
}

#[test]
fn allows_policy_for_configured_client() {
    let classification = matched_total_deposit();
    let catalog = catalog();
    let plan = build_execution_plan(&classification, &catalog);
    let decision = evaluate_policy(&client(), plan.as_ref(), &catalog);

    assert_eq!(decision.status, PolicyDecisionStatus::Allowed);
    assert_eq!(decision.office_ids, vec![1, 2]);
}

#[test]
fn blocks_policy_for_missing_capability() {
    let mut client = client();
    client.allowed_capabilities.clear();
    let classification = matched_total_deposit();
    let catalog = catalog();
    let plan = build_execution_plan(&classification, &catalog);
    let decision = evaluate_policy(&client, plan.as_ref(), &catalog);

    assert_eq!(decision.status, PolicyDecisionStatus::Blocked);
}

#[test]
fn blocks_policy_when_query_output_declares_pii() {
    let classification = ClassificationResult {
        capability: Some("savings_deposit_top_n".to_string()),
        ..matched_total_deposit()
    };
    let mut client = client();
    client
        .allowed_capabilities
        .push("savings_deposit_top_n".to_string());
    let catalog = catalog();
    let plan = build_execution_plan(&classification, &catalog);
    let decision = evaluate_policy(&client, plan.as_ref(), &catalog);

    assert_eq!(decision.status, PolicyDecisionStatus::Blocked);
}

fn matched_total_deposit() -> ClassificationResult {
    ClassificationResult {
        outcome: ClassificationOutcome::Matched,
        domain: Some("savings".to_string()),
        capability: Some("savings_deposit_total".to_string()),
        confidence: 0.86,
        params: json!({
            "from_date": "2026-06-21",
            "to_date": "2026-06-21",
            "office_scope": "authorized_scope",
        }),
        clarification: None,
        options: Vec::new(),
        source: Some("test".to_string()),
        candidates: Vec::new(),
        layers: Vec::new(),
    }
}

fn client() -> ClientContext {
    ClientContext {
        api_key_id: Uuid::nil(),
        name: "test".to_string(),
        owner: "test".to_string(),
        key_prefix: "air_test".to_string(),
        allowed_office_ids: vec![1, 2],
        allowed_capabilities: vec!["savings_deposit_total".to_string()],
        allow_all_offices: false,
        allow_all_capabilities: false,
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
