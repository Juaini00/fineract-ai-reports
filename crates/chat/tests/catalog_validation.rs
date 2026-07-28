//! Pure integration test: no DB, no HTTP.
//! Loads the real `knowledge/` + `queries/` from the workspace and runs the
//! same validator startup uses. This is the fastest guardrail against any
//! YAML/SQL drift and doesn't need Postgres or Fineract.

use app_core::auth::model::PrincipalContext;
use chat::assistant::ClarificationFieldType;
use chat::assistant::execution::plan::{
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
fn catalog_parameter_inputs_define_expected_mappings() {
    let catalog = load_catalog();

    for (id, parameters, field_type) in [
        (
            "date_range",
            &["from_date", "to_date"][..],
            ClarificationFieldType::DateRange,
        ),
        ("limit", &["limit"][..], ClarificationFieldType::Integer),
        ("search", &["search"][..], ClarificationFieldType::Text),
    ] {
        let input = catalog
            .parameter_inputs
            .iter()
            .find(|input| input.id == id)
            .unwrap_or_else(|| panic!("missing parameter input {id}"));
        assert_eq!(input.parameters, parameters);
        assert_eq!(input.field_type, field_type);
    }
}

#[test]
fn catalog_rejects_parameter_input_overlap() {
    let mut catalog = load_catalog();
    catalog
        .parameter_inputs
        .iter_mut()
        .find(|input| input.id == "date_range")
        .expect("date range input")
        .parameters
        .push("limit".into());

    let error = KnowledgeValidator::validate(&catalog).expect_err("overlap must fail");
    assert!(error.to_string().contains("covered more than once"));
}

#[test]
fn catalog_rejects_capability_required_parameter_mismatch() {
    let mut catalog = load_catalog();
    let capability = catalog
        .capabilities
        .iter_mut()
        .find(|capability| capability.id == "savings_deposit_top_n")
        .expect("capability");
    // Drop the last policy so at least one query-required user parameter is
    // no longer covered by the capability's declared policies.
    capability.parameter_policies.pop();
    capability.required_parameters.pop();

    let error = KnowledgeValidator::validate(&catalog).expect_err("mismatch must fail");
    assert!(error.to_string().contains("does not cover"));
}

#[test]
fn catalog_rejects_default_limit_above_maximum() {
    let mut catalog = load_catalog();
    let capability = catalog
        .capabilities
        .iter_mut()
        .find(|capability| capability.id == "office_list_basic")
        .expect("capability");
    capability.defaults.default_limit = Some(201);

    let error = KnowledgeValidator::validate(&catalog).expect_err("default limit must fail");
    assert!(error.to_string().contains("default_limit"));
}

#[test]
fn real_catalog_has_retrievable_capabilities_and_queries() {
    let catalog = load_catalog();

    assert!(!catalog.capabilities.is_empty());
    assert!(!catalog.queries.is_empty());
    for capability in catalog
        .capabilities
        .iter()
        .filter(|c| c.status == "approved_mvp")
    {
        assert!(
            catalog
                .queries
                .iter()
                .any(|query| query.id == capability.query_id),
            "missing query {} for capability {}",
            capability.query_id,
            capability.id
        );
    }

    let documents = RetrievalDocumentBuilder::build(&catalog);
    assert!(!documents.is_empty());
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
fn strictly_overdue_savings_charge_capability_has_an_approved_contract() {
    let catalog = load_catalog();
    let capability = catalog
        .capabilities
        .iter()
        .find(|item| item.id == "savings_strictly_overdue_charges_clients")
        .expect("strictly-overdue savings charge capability");
    assert_eq!(capability.status, "approved_mvp");
    assert_eq!(
        capability.query_id,
        "savings.strictly_overdue_charges_clients"
    );

    let query = catalog
        .queries
        .iter()
        .find(|item| item.id == capability.query_id)
        .expect("strictly-overdue savings charge query");
    assert!(query.parameters.iter().any(|parameter| {
        parameter.name == "as_of_date" && parameter.kind == "date" && !parameter.required
    }));
    assert!(
        query
            .output_fields
            .iter()
            .any(|field| field.name == "days_overdue")
    );

    let sql = std::fs::read_to_string(workspace_root().join(&query.sql_file))
        .expect("read strictly-overdue approved SQL");
    assert!(
        sql.contains("sac.charge_due_date < $2::date"),
        "strict-overdue SQL must exclude same-day, future, and undated charges"
    );
    assert!(
        sql.contains("c.office_id = ANY($1::bigint[])"),
        "office scope must remain inside approved SQL"
    );
    assert!(sql.contains("LIMIT $3"), "limit must remain bound");
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
        ("client_name_lookup", "client.name_lookup", "list"),
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

    let hidden = evaluate_policy(&client(false), Some(&plan), &catalog);
    assert_eq!(hidden.status, PolicyDecisionStatus::Allowed);
    assert!(!hidden.can_view_pii);

    let allowed = evaluate_policy(&client(true), Some(&plan), &catalog);
    assert_eq!(allowed.status, PolicyDecisionStatus::Allowed);
    assert!(allowed.can_view_pii);
}

#[test]
fn client_name_lookup_policy_requires_capability_and_marks_pii_visibility() {
    let catalog = load_catalog();
    let plan = ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: "client".into(),
        capability: "client_name_lookup".into(),
        query_id: "client.name_lookup".into(),
        output_mode: "list".into(),
        params: json!({ "search": "Tony" }),
        retrieval_plan: RetrievalPlan::default(),
        evidence_evaluation: EvidenceEvaluation::default(),
        answer_plan: AnswerPlan::default(),
        requires_policy_check: true,
    };
    let mut client = client(false);

    let missing_capability = evaluate_policy(&client, Some(&plan), &catalog);
    assert_eq!(missing_capability.status, PolicyDecisionStatus::Blocked);

    client.capability_ids.push("client_name_lookup".into());
    let pii_hidden = evaluate_policy(&client, Some(&plan), &catalog);
    assert_eq!(pii_hidden.status, PolicyDecisionStatus::Allowed);
    assert!(!pii_hidden.can_view_pii);

    client.can_view_pii = true;
    let allowed = evaluate_policy(&client, Some(&plan), &catalog);
    assert_eq!(allowed.status, PolicyDecisionStatus::Allowed);
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

fn client(can_view_pii: bool) -> PrincipalContext {
    PrincipalContext {
        user_id: Uuid::new_v4(),
        role: "admin".into(),
        office_ids: vec![1, 2, 3],
        capability_ids: vec!["savings_deposit_top_n".into()],
        can_view_pii,
        legacy_api_key_id: None,
    }
}

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}
