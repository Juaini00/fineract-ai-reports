use super::*;
use crate::chat::pipeline::answer::GeneratedAnswer;

#[test]
fn capability_option_label_uses_requested_period_not_catalog_example() {
    let capability = CapabilityKnowledge {
        id: "savings_deposit_total".to_string(),
        status: "approved_mvp".to_string(),
        domain: "savings".to_string(),
        query_id: "savings.deposit_total".to_string(),
        output_mode: "total".to_string(),
        display_name: None,
        description: None,
        data_areas: Vec::new(),
        metrics: Vec::new(),
        examples: vec!["What is the total deposit this month?".to_string()],
        required_parameters: Vec::new(),
        optional_parameters: Vec::new(),
    };

    assert_eq!(
        capability_option_label(&capability, "Show customer savings activity this week"),
        "Total deposit this week"
    );
}

#[test]
fn generic_savings_activity_prompt_is_not_deterministic_list_request() {
    assert!(!is_savings_activity_request(
        "Show customer savings activity this week"
    ));
}

#[test]
fn explicit_savings_transactions_prompt_is_deterministic_list_request() {
    assert!(is_savings_activity_request(
        "List savings transactions this week"
    ));
}

#[test]
fn lqr_runs_only_when_enabled_and_llm_configured() {
    assert!(should_try_lqr(true, true));
    assert!(!should_try_lqr(true, false));
    assert!(!should_try_lqr(false, true));
}

#[test]
fn client_list_prompt_does_not_match_summary_capability() {
    assert!(!capability_matches_prompt_shape(
        "show me 10 of client list data",
        &capability("client_lifecycle_summary", "client", "summary")
    ));
}

#[test]
fn client_prompt_shapes_match_expected_capabilities() {
    let lifecycle = capability("client_lifecycle_summary", "client", "summary");
    let balance = capability("client_top_n_by_savings_balance", "client", "top_n");
    let accounts = capability("client_top_n_by_savings_account_count", "client", "top_n");
    let deposit_volume = capability("client_top_n_by_deposit_volume", "client", "top_n");

    assert!(capability_matches_prompt_shape(
        "show client lifecycle summary",
        &lifecycle
    ));
    assert!(!capability_matches_prompt_shape(
        "show client lifecycle summary",
        &balance
    ));
    // "most savings accounts" is intentionally ambiguous (count vs balance vs
    // deposit volume) — all three top_n candidates must survive the shape
    // filter so clarification can offer real choices.
    assert!(capability_matches_prompt_shape(
        "show 10 clients with the most savings accounts",
        &accounts
    ));
    assert!(capability_matches_prompt_shape(
        "show 10 clients with the most savings accounts",
        &balance
    ));
    assert!(capability_matches_prompt_shape(
        "show 10 clients with the most savings accounts",
        &deposit_volume
    ));
    assert!(capability_matches_prompt_shape(
        "show 10 clients with the largest savings balance",
        &balance
    ));
    assert!(!capability_matches_prompt_shape(
        "show 10 clients with the largest savings balance",
        &accounts
    ));
    assert!(capability_matches_prompt_shape(
        "show clients with the highest deposit volume this month",
        &deposit_volume
    ));
    assert!(!capability_matches_prompt_shape(
        "show clients with the highest deposit volume this month",
        &balance
    ));
}

#[test]
fn organization_prompt_shapes_match_expected_capabilities() {
    let office_summary = capability("organization_office_summary", "organization", "summary");
    let hierarchy_summary = capability("organization_hierarchy_summary", "organization", "summary");
    let office_tree = capability(
        "organization_office_hierarchy_tree",
        "organization",
        "top_n",
    );
    let office_activity = capability(
        "organization_office_activity_ranking",
        "organization",
        "top_n",
    );
    let office_clients = capability(
        "organization_office_client_summary",
        "organization",
        "top_n",
    );
    let office_savings = capability(
        "organization_office_savings_summary",
        "organization",
        "top_n",
    );
    let dormant = capability("organization_office_dormant", "organization", "top_n");

    assert!(capability_matches_prompt_shape(
        "show organization office summary",
        &office_summary
    ));
    assert!(capability_matches_prompt_shape(
        "show office hierarchy summary",
        &hierarchy_summary
    ));
    assert!(!capability_matches_prompt_shape(
        "show office hierarchy summary",
        &office_tree
    ));
    assert!(capability_matches_prompt_shape(
        "show list of offices",
        &office_tree
    ));
    assert!(!capability_matches_prompt_shape(
        "show list of offices",
        &office_summary
    ));
    assert!(capability_matches_prompt_shape(
        "show top 10 offices by transaction count this month",
        &office_activity
    ));
    assert!(!capability_matches_prompt_shape(
        "show top 10 offices by transaction count this month",
        &office_clients
    ));
    assert!(capability_matches_prompt_shape(
        "which offices have the most active clients",
        &office_clients
    ));
    assert!(!capability_matches_prompt_shape(
        "which offices have the most active clients",
        &office_activity
    ));
    assert!(capability_matches_prompt_shape(
        "rank offices by savings balance",
        &office_savings
    ));
    assert!(!capability_matches_prompt_shape(
        "rank offices by savings balance",
        &office_activity
    ));
    assert!(capability_matches_prompt_shape(
        "list dormant offices this quarter",
        &dormant
    ));
    assert!(!capability_matches_prompt_shape(
        "list dormant offices this quarter",
        &office_tree
    ));
}

fn capability(id: &str, domain: &str, output_mode: &str) -> CapabilityKnowledge {
    CapabilityKnowledge {
        id: id.to_string(),
        status: "approved_mvp".to_string(),
        domain: domain.to_string(),
        query_id: id.replace('_', "."),
        output_mode: output_mode.to_string(),
        display_name: None,
        description: None,
        data_areas: Vec::new(),
        metrics: Vec::new(),
        examples: Vec::new(),
        required_parameters: Vec::new(),
        optional_parameters: Vec::new(),
    }
}

#[test]
fn generated_answer_replaces_only_payload_message_when_citations_are_valid() {
    let payload = serde_json::json!({
        "answer_plan": { "coverage": { "returned_rows": 1 } },
        "structured": { "rows": [{ "transaction_id": 10 }] },
        "message": "deterministic"
    })
    .to_string();
    let answer = GeneratedAnswer {
        message: "llm grounded".to_string(),
        citations: vec![
            "answer_plan.coverage".to_string(),
            "structured.rows[0]".to_string(),
        ],
    };

    let updated = apply_generated_answer(payload, &answer).unwrap();
    let updated: serde_json::Value = serde_json::from_str(&updated).unwrap();

    assert_eq!(updated["message"], "llm grounded");
    assert_eq!(updated["structured"]["rows"][0]["transaction_id"], 10);
}

#[test]
fn generated_answer_is_rejected_when_citation_is_not_grounded() {
    let payload = serde_json::json!({
        "answer_plan": { "coverage": { "returned_rows": 0 } },
        "structured": { "rows": [] },
        "message": "deterministic"
    })
    .to_string();
    let answer = GeneratedAnswer {
        message: "ungrounded".to_string(),
        citations: vec!["structured.rows[0]".to_string()],
    };

    assert!(apply_generated_answer(payload, &answer).is_none());
}

#[test]
fn redis_url_log_value_hides_password() {
    assert_eq!(
        redis_url_log_value("redis://:secret@127.0.0.1:6380/0"),
        "redis://***@127.0.0.1:6380/0"
    );
    assert_eq!(
        redis_url_log_value("redis://127.0.0.1:6380/0"),
        "redis://127.0.0.1:6380/0"
    );
}

#[test]
fn evidence_drops_rows_and_stamps_row_count() {
    let rows: Vec<serde_json::Value> = (0..20).map(|i| serde_json::json!({ "id": i })).collect();
    let payload = serde_json::json!({
        "answer_plan": { "coverage": { "returned_rows": 20 } },
        "structured": { "by_currency": {"IDR": {}}, "rows": rows }
    });
    let evidence = super::build_llm_evidence(&payload);
    assert!(evidence["structured"].get("rows").is_none());
    assert_eq!(evidence["structured"]["row_count"], 20);
    assert_eq!(
        evidence["structured"]["by_currency"]["IDR"],
        serde_json::json!({})
    );
    assert!(evidence.get("answer_plan").is_some());
}

#[test]
fn evidence_is_noop_when_no_structured() {
    let payload = serde_json::json!({ "message": "hi" });
    let evidence = super::build_llm_evidence(&payload);
    assert_eq!(evidence, payload);
}
