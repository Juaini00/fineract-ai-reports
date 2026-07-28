use chat::assistant::execution::plan::{
    AnswerPlan, EvidenceEvaluation, ExecutionPlan, ExecutionPlanType, PolicyDecision,
    PolicyDecisionStatus, RetrievalPlan,
};
use chat::assistant::{
    AssistantConstraints, AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage,
    ContextReference, ResponseBuilder, ToolResult,
};
use chat::knowledge::model::{KnowledgeCatalog, QueryKnowledge, QueryOutputField, Sensitivity};
use serde_json::json;

#[test]
fn structured_response_renders_markdown_and_preserves_refs() {
    let response = ResponseBuilder::from_tool_result(
        &intent(),
        &plan(),
        &policy(false),
        &ToolResult {
            tool_name: "approved_catalog_sql".into(),
            ok: true,
            rows: vec![json!({
                "client_id": 7,
                "display_name": "Tony Secret",
                "office_name": "HQ",
                "status_label": "active"
            })],
            summary: None,
            error: None,
            evidence_refs: vec!["client_name_lookup".into()],
        },
        &catalog(),
    );

    assert_eq!(
        response.message,
        "Found one matching client in your authorized office scope."
    );
    assert_eq!(response.evidence_refs[0].id, "client_name_lookup");
    let markdown = response.rendered_markdown.as_deref().unwrap();
    assert!(markdown.contains("office name"));
    assert!(!markdown.contains("Tony Secret"));
    assert!(
        !serde_json::to_string(&response.table)
            .unwrap()
            .contains("Tony Secret")
    );
    assert_eq!(response.warnings[0].code, "pii_hidden");
}

#[test]
fn client_lookup_messages_are_ambiguity_aware() {
    for (rows, expected) in [
        (
            vec![],
            "No matching client was found in your authorized office scope.",
        ),
        (
            vec![json!({ "client_id": 1 })],
            "Found one matching client in your authorized office scope.",
        ),
        (
            vec![json!({ "client_id": 1 }), json!({ "client_id": 2 })],
            "Found 2 matching clients. Please use the table to disambiguate.",
        ),
    ] {
        let response = ResponseBuilder::from_tool_result(
            &intent(),
            &plan(),
            &policy(true),
            &ToolResult {
                tool_name: "approved_catalog_sql".into(),
                ok: true,
                rows,
                summary: None,
                error: None,
                evidence_refs: vec![],
            },
            &catalog(),
        );
        assert_eq!(response.message, expected);
    }
}

fn intent() -> AssistantIntent {
    AssistantIntent {
        intent: AssistantIntentKind::DataLookup,
        domain: AssistantDomain::Client,
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: AssistantConstraints::default(),
        context_reference: ContextReference::None,
        source: None,
        confidence: 1.0,
        reason: "test".into(),
    }
}

fn plan() -> ExecutionPlan {
    ExecutionPlan {
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
    }
}

fn policy(can_view_pii: bool) -> PolicyDecision {
    PolicyDecision {
        status: PolicyDecisionStatus::Allowed,
        reason: None,
        office_ids: vec![1],
        can_view_pii,
    }
}

fn catalog() -> KnowledgeCatalog {
    KnowledgeCatalog {
        root_path: Default::default(),
        query_path: Default::default(),
        data_areas: Vec::new(),
        domains: Vec::new(),
        schemas: Vec::new(),
        metrics: Vec::new(),
        capabilities: Vec::new(),
        queries: vec![QueryKnowledge {
            id: "client.name_lookup".into(),
            database: "fineract".into(),
            sql_file: "client.sql".into(),
            data_areas: Vec::new(),
            tables: Vec::new(),
            metrics: Vec::new(),
            parameters: Vec::new(),
            output_fields: vec![
                field("client_id", "bigint", "public_business"),
                field("display_name", "string", "pii"),
                field("office_name", "string", "public_business"),
                field("status_label", "string", "public_business"),
            ],
            timeout_ms: None,
        }],
        policies: Vec::new(),
        responses: Vec::new(),
        parameter_inputs: Vec::new(),
        classification: Default::default(),
    }
}

fn field(name: &str, kind: &str, sensitivity: &str) -> QueryOutputField {
    QueryOutputField {
        name: name.into(),
        kind: kind.into(),
        sensitivity: match sensitivity {
            "public_business" => Sensitivity::PublicBusiness,
            "pii" => Sensitivity::Pii,
            other => panic!("unsupported test sensitivity {other}"),
        },
    }
}
