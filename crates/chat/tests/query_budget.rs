use std::path::PathBuf;

use chat::assistant::execution::plan::{
    EvidenceEvaluation, ExecutionPlan, ExecutionPlanType, PolicyDecision, PolicyDecisionStatus,
    RetrievalPlan,
};
use chat::assistant::{
    AssistantConstraints, AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage,
    ContextReference, ResponseBuilder, tool_request_from_plan, tool_result_from_execution,
};
use chat::execution::repository::{ExecutionLimits, execute_plan};
use chat::knowledge::catalog::loader::KnowledgeLoader;
use sqlx::PgPool;

const CAPABILITY_ID: &str = "savings_deposit_top_n";
const QUERY_ID: &str = "savings.deposit_top_n";

#[tokio::test]
async fn declared_hard_cap_truncates_and_warns() {
    let Some(pool) = fineract_pool().await else {
        return;
    };
    let mut catalog = catalog();
    catalog
        .capabilities
        .iter_mut()
        .find(|capability| capability.id == CAPABILITY_ID)
        .expect("approved capability")
        .parameter_policies
        .iter_mut()
        .find(|policy| policy.name == "limit")
        .expect("limit policy")
        .hard_cap = Some(2);
    let plan = plan(100);
    let policy = policy();

    let result = execute_plan(
        &pool,
        &catalog,
        &plan,
        &policy,
        ExecutionLimits {
            default_timeout_ms: 3_000,
            global_max_rows: 50_000,
        },
    )
    .await
    .expect("approved query executes");

    assert!(result["rows"].as_array().expect("rows").len() <= 2);
    assert_eq!(result["truncated"], true, "fixture must exceed the cap");
    let request = tool_request_from_plan(&plan, Vec::new());
    let tool_result = tool_result_from_execution(&request, result);
    let response =
        ResponseBuilder::from_tool_result(&intent(), &plan, &policy, &tool_result, &catalog);
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "result_truncated")
    );
}

#[tokio::test]
async fn global_backstop_truncates_capability_without_hard_cap() {
    let Some(pool) = fineract_pool().await else {
        return;
    };
    let mut catalog = catalog();
    catalog
        .capabilities
        .iter_mut()
        .find(|capability| capability.id == CAPABILITY_ID)
        .expect("approved capability")
        .parameter_policies
        .iter_mut()
        .find(|policy| policy.name == "limit")
        .expect("limit policy")
        .hard_cap = None;
    let result = execute_plan(
        &pool,
        &catalog,
        &plan(100),
        &policy(),
        ExecutionLimits {
            default_timeout_ms: 3_000,
            global_max_rows: 2,
        },
    )
    .await
    .expect("approved query executes");

    assert!(result["rows"].as_array().expect("rows").len() <= 2);
    assert_eq!(
        result["truncated"], true,
        "fixture must exceed the backstop"
    );
}

async fn fineract_pool() -> Option<PgPool> {
    let Ok(url) = std::env::var("FINERACT_DATABASE_URL") else {
        eprintln!("skipping: FINERACT_DATABASE_URL unset");
        return None;
    };
    Some(PgPool::connect(&url).await.expect("connect fineract"))
}

fn catalog() -> chat::knowledge::model::KnowledgeCatalog {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
        .load()
        .expect("load catalog")
}

fn plan(limit: i64) -> ExecutionPlan {
    ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: "savings".into(),
        capability: CAPABILITY_ID.into(),
        query_id: QUERY_ID.into(),
        output_mode: "top_n".into(),
        params: serde_json::json!({
            "from_date": "2020-01-01",
            "to_date": "2030-01-01",
            "limit": limit,
        }),
        retrieval_plan: RetrievalPlan::default(),
        evidence_evaluation: EvidenceEvaluation::default(),
        requires_policy_check: true,
    }
}

fn policy() -> PolicyDecision {
    PolicyDecision {
        status: PolicyDecisionStatus::Allowed,
        reason: None,
        office_ids: vec![1, 2, 3, 4, 40],
        can_view_pii: true,
    }
}

fn intent() -> AssistantIntent {
    AssistantIntent {
        intent: AssistantIntentKind::DataLookup,
        domain: AssistantDomain::Savings,
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        canonical_query_en: String::new(),
        entities: Vec::new(),
        constraints: AssistantConstraints::default(),
        context_reference: ContextReference::None,
        source: None,
        confidence: 1.0,
        reason: "query budget test".into(),
    }
}
