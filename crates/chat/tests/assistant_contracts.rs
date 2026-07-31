use chat::assistant::{
    assistant_contract_names, assistant_contract_schemas, clarification::*, intent::*, memory::*,
    response::*, tool::*,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

#[test]
fn assistant_contracts_schemas_cover_boundary_roots() {
    let names = assistant_contract_names();
    for expected in [
        "AssistantIntent",
        "SourceIntentSnapshot",
        "ClarificationPayload",
        "ClarificationView",
        "ClarificationField",
        "ClarificationChoice",
        "ClarificationOutcome",
        "PendingClarification",
        "GraphState",
        "TerminalState",
        "GraphTransition",
        "RetrievalPlan",
        "Evidence",
        "RerankerDecision",
        "ContextWindow",
        "JobMemory",
        "SessionMemory",
        "MemoryDelta",
        "ToolRequest",
        "ToolResult",
        "ToolValidationError",
        "AssistantResponse",
    ] {
        assert!(names.contains(&expected), "missing {expected}");
    }
    for (name, schema) in assistant_contract_schemas() {
        assert!(schema.get("$schema").is_some(), "{name} has no $schema");
    }
}

#[test]
fn assistant_contracts_representative_round_trip() {
    let intent = AssistantIntent {
        intent: AssistantIntentKind::ReportRequest,
        domain: AssistantDomain::Savings,
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        canonical_query_en: String::new(),
        entities: vec![AssistantEntity {
            entity_type: AssistantEntityType::PersonName,
            value: "Tony".into(),
            canonical: Some("Tony".into()),
            confidence: Some(0.9),
        }],
        constraints: AssistantConstraints {
            quantity: Some(Quantity::TopN { value: 10 }),
            ..Default::default()
        },
        context_reference: ContextReference::None,
        source: None,
        confidence: 0.91,
        reason: "top_n savings request".into(),
    };
    let intent: AssistantIntent =
        serde_json::from_value(serde_json::to_value(intent).unwrap()).unwrap();
    assert_eq!(
        intent.constraints.quantity,
        Some(Quantity::TopN { value: 10 })
    );

    let source = SourceIntentSnapshot {
        prompt: "top 10 clients".into(),
        normalized_prompt: None,
        intent: AssistantIntentKind::ReportRequest,
        domain: AssistantDomain::Client,
        request_shape: Default::default(),
        entities: Vec::new(),
        constraints: AssistantConstraints::default(),
        context_reference: ContextReference::None,
        confidence: 0.8,
        reason: "ambiguous ranking".into(),
    };
    let pending = PendingClarification {
        payload: ClarificationPayload {
            version: CLARIFICATION_VERSION_1,
            id: uuid::Uuid::new_v4(),
            revision: 0,
            kind: ClarificationKind::SelectOption,
            question: "Which report should I use?".into(),
            options: vec![ClarificationOption {
                id: "client_top_n_by_savings_balance".into(),
                label: "Top clients by savings balance".into(),
                description: None,
                fields: Vec::new(),
            }],
            fields: Vec::new(),
            attempt: 1,
            source_intent: Some(source.clone()),
            allow_free_text: true,
            is_missing_execution_parameters: false,
        },
        source_intent: Some(source),
        created_at: None,
    };
    let pending: PendingClarification =
        serde_json::from_value(serde_json::to_value(pending).unwrap()).unwrap();
    assert!(pending.source_intent.is_some());

    let response = AssistantResponse {
        response_type: AssistantResponseType::Table,
        title: Some("Savings".into()),
        message: "Found 1 row.".into(),
        sections: Vec::new(),
        table: Some(ResponseTable {
            columns: vec![
                TableColumn {
                    key: "name".into(),
                    label: "Name".into(),
                    kind: TableColumnKind::Text,
                    hidden: false,
                },
                TableColumn {
                    key: "account_no".into(),
                    label: "Account".into(),
                    kind: TableColumnKind::Text,
                    hidden: true,
                },
            ],
            rows: vec![json!({"name":"Tony"})],
        }),
        cards: Vec::new(),
        options: Vec::new(),
        clarification: None,
        warnings: vec![ResponseWarning {
            code: "pii_hidden".into(),
            message: "PII hidden".into(),
        }],
        actions: Vec::new(),
        evidence_refs: vec![EvidenceReference {
            id: "capability:client_top_n_by_savings_balance".into(),
            source_type: "capability".into(),
            label: None,
        }],
        rendered_markdown: Some("|Name|".into()),
    };
    assert!(
        serde_json::to_string(&response)
            .unwrap()
            .contains("pii_hidden")
    );

    let tool = ToolRequest {
        tool_name: "execute_query".into(),
        capability_id: Some("client_top_n_by_savings_balance".into()),
        query_id: Some("client.top_n_by_savings_balance".into()),
        params: json!({"top_n": 10}),
        evidence_refs: vec!["capability:client_top_n_by_savings_balance".into()],
    };
    let _: ToolRequest = serde_json::from_value(serde_json::to_value(tool).unwrap()).unwrap();

    let job = JobMemory {
        job_id: Uuid::nil(),
        graph_state: "complete".into(),
        terminal_state: None,
        current_user_message_metadata: json!({}),
        intent: None,
        source_intent: None,
        retrieval_plan: json!({}),
        retrieval_evidence: json!({}),
        evidence_decision: json!({}),
        selected_capability: None,
        selected_tool: None,
        tool_params: json!({}),
        policy_decision: json!({}),
        execution_summary: json!({}),
        structured_response: None,
        pending_clarification: None,
        planner_snapshot_id: None,
        warnings: json!([]),
        revision: 1,
    };
    let session = SessionMemory {
        session_id: Uuid::nil(),
        summary: None,
        active_domain: Some("savings".into()),
        pending_clarification: None,
        pending_clarification_source_intent: None,
        pending: None,
        entities: json!({}),
        relevant_jobs: json!([]),
        context_warnings: json!([]),
        revision: 1,
    };
    let _: JobMemory = serde_json::from_value(serde_json::to_value(job).unwrap()).unwrap();
    let _: SessionMemory = serde_json::from_value(serde_json::to_value(session).unwrap()).unwrap();
}

#[test]
fn clarification_view_select_option_round_trips_conditional_fields() {
    let view = ClarificationView {
        version: CLARIFICATION_VERSION_1,
        id: Uuid::nil(),
        revision: 2,
        kind: ClarificationKind::SelectOption,
        question: "Which report should I use?".into(),
        options: vec![ClarificationChoice {
            id: "monthly".into(),
            label: "Monthly report".into(),
            description: None,
            fields: vec![ClarificationField {
                key: "date_range".into(),
                label: "Date range".into(),
                field_type: ClarificationFieldType::DateRange,
                required: true,
                value: None,
                default_value: None,
                help_text: None,
                validation: Default::default(),
                errors: Vec::new(),
            }],
        }],
        fields: Vec::new(),
        allow_free_text: true,
    };
    let parsed: ClarificationView =
        serde_json::from_value(serde_json::to_value(&view).unwrap()).unwrap();
    assert_eq!(parsed, view);
    assert!(parsed.validate().is_ok());
}

#[test]
fn old_assistant_response_json_deserializes_without_clarification() {
    let response: AssistantResponse = serde_json::from_value(json!({
        "response_type": "summary", "title": null, "message": "Hello",
        "sections": [], "table": null, "cards": [], "options": [], "warnings": [],
        "actions": [], "evidence_refs": [], "rendered_markdown": null
    }))
    .unwrap();
    assert!(response.clarification.is_none());
}

#[test]
fn clarification_view_validation_rejects_invalid_shapes() {
    let base = ClarificationView {
        version: CLARIFICATION_VERSION_1,
        id: Uuid::nil(),
        revision: 0,
        kind: ClarificationKind::SelectOption,
        question: "Choose".into(),
        options: Vec::new(),
        fields: Vec::new(),
        allow_free_text: false,
    };
    assert!(base.validate().is_err());
    let collect = ClarificationView {
        kind: ClarificationKind::CollectFields,
        ..base.clone()
    };
    assert!(collect.validate().is_err());
    let free_text = ClarificationView {
        kind: ClarificationKind::FreeText,
        fields: vec![ClarificationField {
            key: "text".into(),
            label: "Text".into(),
            field_type: ClarificationFieldType::Text,
            required: true,
            value: None,
            default_value: None,
            help_text: None,
            validation: Default::default(),
            errors: Vec::new(),
        }],
        ..base
    };
    assert!(free_text.validate().is_err());
}

#[derive(Debug, Deserialize)]
struct GoldenScenario {
    prompt: String,
    expected_intent: String,
    expected_domain: String,
    expected_entities: serde_json::Value,
    expected_constraints: serde_json::Value,
    expected_response_type: String,
}

#[test]
fn assistant_contracts_golden_scenarios_parse_and_cover_matrix() {
    let raw = include_str!("../../../tests/golden/assistant_scenarios.jsonl");
    let scenarios = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<GoldenScenario>(line).unwrap())
        .collect::<Vec<_>>();
    for needle in [
        "hello",
        "help",
        "Tony",
        "top 10",
        "laptop",
        "account numbers",
    ] {
        assert!(
            scenarios.iter().any(|item| item.prompt.contains(needle)),
            "missing {needle}"
        );
    }
    for item in scenarios {
        assert!(!item.expected_intent.is_empty());
        assert!(!item.expected_domain.is_empty());
        assert!(item.expected_entities.is_array());
        assert!(item.expected_constraints.is_object());
        assert!(!item.expected_response_type.is_empty());
    }
}
