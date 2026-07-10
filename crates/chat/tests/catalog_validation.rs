//! Pure integration test: no DB, no HTTP.
//! Loads the real `knowledge/` + `queries/` from the workspace and runs the
//! same validator startup uses. This is the fastest guardrail against any
//! YAML/SQL drift and doesn't need Postgres or Fineract.

use app_core::auth::model::ClientContext;
use chat::chat::planner::{
    AnswerPlan, EvidenceEvaluation, ExecutionPlan, ExecutionPlanType, PolicyDecisionStatus,
    RetrievalPlan, evaluate_policy,
};
use chat::knowledge::catalog::loader::KnowledgeLoader;
use chat::knowledge::catalog::validator::KnowledgeValidator;
use chat::knowledge::retrieval::RetrievalDocumentBuilder;
use serde_json::json;
use uuid::Uuid;

#[test]
fn real_catalog_loads_and_passes_validation() {
    // Arrange
    let workspace_root = workspace_root();

    // Act
    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load knowledge catalog");
    KnowledgeValidator::validate(&catalog).expect("validate knowledge catalog");

    // Assert — every runtime category must be populated
    assert!(!catalog.data_areas.is_empty(), "data_areas empty");
    assert!(!catalog.domains.is_empty(), "domains empty");
    assert!(!catalog.metrics.is_empty(), "metrics empty");
    assert!(!catalog.capabilities.is_empty(), "capabilities empty");
    assert!(!catalog.queries.is_empty(), "queries empty");
    assert!(!catalog.policies.is_empty(), "policies empty");
    assert!(!catalog.responses.is_empty(), "responses empty");
}

#[test]
fn real_catalog_matches_documented_scenario_counts() {
    let catalog = load_catalog();

    assert_eq!(catalog.data_areas.len(), 13);
    assert_eq!(catalog.domains.len(), 7);
    assert_eq!(catalog.metrics.len(), 8);
    assert_eq!(catalog.capabilities.len(), 25);
    assert_eq!(catalog.queries.len(), 25);
    assert_eq!(catalog.policies.len(), 6);
    assert_eq!(catalog.responses.len(), 3);

    let documents = RetrievalDocumentBuilder::build(&catalog);
    assert_eq!(documents.len(), 101);
}

#[test]
fn approved_catalog_includes_foundation_capabilities() {
    let catalog = load_catalog();

    for (capability_id, query_id) in [
        ("organization_office_summary", "organization.office_summary"),
        ("client_lifecycle_summary", "client.lifecycle_summary"),
    ] {
        let capability = catalog
            .capabilities
            .iter()
            .find(|item| item.id == capability_id)
            .unwrap_or_else(|| panic!("missing capability {capability_id}"));
        assert_eq!(capability.status, "approved_mvp");
        assert_eq!(capability.query_id, query_id);

        let query = catalog
            .queries
            .iter()
            .find(|item| item.id == query_id)
            .unwrap_or_else(|| panic!("missing query {query_id}"));
        assert_eq!(query.database, "fineract");
        assert!(query.sql_file.ends_with(".sql"));
        assert!(!query.sql_file.contains(".."));
    }
}

#[test]
fn approved_catalog_includes_all_client_and_organization_capabilities() {
    let catalog = load_catalog();

    for (capability_id, query_id, output_mode) in [
        (
            "client_lifecycle_summary",
            "client.lifecycle_summary",
            "summary",
        ),
        (
            "client_top_n_by_savings_balance",
            "client.top_n_by_savings_balance",
            "top_n",
        ),
        (
            "client_top_n_by_savings_account_count",
            "client.top_n_by_savings_account_count",
            "top_n",
        ),
        (
            "client_top_n_by_deposit_volume",
            "client.top_n_by_deposit_volume",
            "top_n",
        ),
        (
            "client_summary_by_office",
            "client.summary_by_office",
            "top_n",
        ),
        (
            "client_activation_monthly_breakdown",
            "client.activation_monthly_breakdown",
            "monthly_breakdown",
        ),
        (
            "client_activation_top_n_offices",
            "client.activation_top_n_offices",
            "top_n",
        ),
        (
            "organization_office_summary",
            "organization.office_summary",
            "summary",
        ),
        (
            "organization_hierarchy_summary",
            "organization.hierarchy_summary",
            "summary",
        ),
        (
            "organization_office_client_summary",
            "organization.office_client_summary",
            "top_n",
        ),
        (
            "organization_office_savings_summary",
            "organization.office_savings_summary",
            "top_n",
        ),
        (
            "organization_office_activity_ranking",
            "organization.office_activity_ranking",
            "top_n",
        ),
        (
            "organization_office_hierarchy_tree",
            "organization.office_hierarchy_tree",
            "top_n",
        ),
        (
            "organization_office_dormant",
            "organization.office_dormant",
            "top_n",
        ),
        (
            "organization_office_opening_monthly_breakdown",
            "organization.office_opening_monthly_breakdown",
            "monthly_breakdown",
        ),
    ] {
        let capability = catalog
            .capabilities
            .iter()
            .find(|item| item.id == capability_id)
            .unwrap_or_else(|| panic!("missing capability {capability_id}"));
        assert_eq!(capability.status, "approved_mvp", "{capability_id}");
        assert_eq!(capability.query_id, query_id, "{capability_id}");
        assert_eq!(capability.output_mode, output_mode, "{capability_id}");
        assert!(
            catalog.queries.iter().any(|query| query.id == query_id),
            "missing query {query_id}"
        );
    }
}

#[test]
fn every_approved_capability_maps_to_an_approved_query() {
    let workspace_root = workspace_root();
    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load knowledge catalog");
    KnowledgeValidator::validate(&catalog).expect("validate knowledge catalog");

    for capability in catalog
        .capabilities
        .iter()
        .filter(|c| c.status == "approved_mvp")
    {
        let query = catalog
            .queries
            .iter()
            .find(|q| q.id == capability.query_id)
            .unwrap_or_else(|| {
                panic!(
                    "approved capability {} references unknown query_id {}",
                    capability.id, capability.query_id
                )
            });
        assert_eq!(
            query.database, "fineract",
            "capability {} targets non-fineract database",
            capability.id
        );
    }
}

#[test]
fn retrieval_documents_cover_all_capabilities() {
    let catalog = load_catalog();

    let documents = RetrievalDocumentBuilder::build(&catalog);
    assert!(!documents.is_empty());

    for capability in &catalog.capabilities {
        assert!(
            documents.iter().any(|d| d.source_id == capability.id),
            "capability {} missing from retrieval documents",
            capability.id
        );
    }
}

#[test]
fn pii_policy_uses_selected_query_output_fields() {
    let catalog = load_catalog();
    let plan = ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: "savings".into(),
        capability: "savings_deposit_top_n".into(),
        query_id: "savings.deposit_top_n".into(),
        output_mode: "top_n".into(),
        params: json!({}),
        retrieval_plan: RetrievalPlan::default(),
        evidence_evaluation: EvidenceEvaluation::default(),
        answer_plan: AnswerPlan::default(),
        requires_policy_check: true,
    };

    let blocked = evaluate_policy(&client(false), Some(&plan), &catalog);
    assert_eq!(blocked.status, PolicyDecisionStatus::Blocked);

    let allowed = evaluate_policy(&client(true), Some(&plan), &catalog);
    assert_eq!(allowed.status, PolicyDecisionStatus::Allowed);
    assert!(allowed.can_view_pii);
}

fn load_catalog() -> chat::knowledge::model::KnowledgeCatalog {
    let workspace_root = workspace_root();
    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load knowledge catalog");
    KnowledgeValidator::validate(&catalog).expect("validate knowledge catalog");
    catalog
}

fn client(can_view_pii: bool) -> ClientContext {
    ClientContext {
        api_key_id: Uuid::new_v4(),
        name: "scenario-test".into(),
        owner: "integration-tests".into(),
        key_prefix: "air_test".into(),
        allowed_office_ids: vec![1, 2, 3],
        allowed_capabilities: vec!["savings_deposit_top_n".into()],
        allow_all_offices: false,
        allow_all_capabilities: false,
        can_view_pii,
        expires_at: None,
    }
}

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}
