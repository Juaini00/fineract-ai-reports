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
