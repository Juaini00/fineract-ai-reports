use serde_json::json;

use super::*;
use crate::chat::planner::{
    AnswerPlan, EvidenceEvaluation, ExecutionPlan, ExecutionPlanType, PolicyDecision,
    PolicyDecisionStatus, RetrievalPlan,
};
use crate::knowledge::catalog::{loader::KnowledgeLoader, validator::KnowledgeValidator};

#[test]
fn formats_total_from_query_contract_without_currency_guess() {
    let catalog = catalog();
    let plan = plan("savings_deposit_total", "savings.deposit_total", "total");
    let result = json!({
        "rows": [{
            "from_date": "2026-06-01",
            "to_date": "2026-06-21",
            "total_deposit_amount": "200.000000",
            "deposit_count": 2
        }]
    });

    assert_eq!(
        format_report_response(&catalog, &plan, &policy(false), &result).as_deref(),
        Some(
            "From date: 2026-06-01. To date: 2026-06-21. Total deposit amount: 200.000000. Deposit count: 2."
        )
    );
}

#[test]
fn formats_rows_from_query_contract_and_skips_pii() {
    let catalog = catalog();
    let plan = plan("savings_deposit_top_n", "savings.deposit_top_n", "top_n");
    let result = json!({
        "row_count": 1,
        "rows": [{
            "transaction_id": 10,
            "transaction_date": "2026-06-21",
            "amount": "25000000.000000",
            "currency_code": "USD",
            "office_id": 1,
            "office_name": "HQ",
            "product_id": 2,
            "product_name": "Regular Savings",
            "client_id": 3,
            "client_display_name": "Amina"
        }]
    });
    let response = format_report_response(&catalog, &plan, &policy(false), &result).unwrap();

    assert!(response.contains("Amount: USD 25000000.000000"));
    assert!(!response.contains("Amina"));
}

#[test]
fn formats_empty_result_from_response_catalog() {
    let catalog = catalog();
    let plan = plan("savings_deposit_top_n", "savings.deposit_top_n", "top_n");

    assert_eq!(
        format_report_response(&catalog, &plan, &policy(false), &json!({ "rows": [] })).as_deref(),
        Some("No data was found for the requested parameters.")
    );
}

#[test]
fn includes_pii_when_policy_allows_it() {
    let catalog = catalog();
    let plan = plan("savings_deposit_top_n", "savings.deposit_top_n", "top_n");
    let result = json!({
        "rows": [{
            "transaction_id": 10,
            "transaction_date": "2026-06-21",
            "amount": "25000000.000000",
            "currency_code": "USD",
            "office_id": 1,
            "office_name": "HQ",
            "product_id": 2,
            "product_name": "Regular Savings",
            "client_id": 3,
            "client_display_name": "Amina"
        }]
    });

    let response = format_report_response(&catalog, &plan, &policy(true), &result).unwrap();

    assert!(response.contains("Amina"));
}

#[test]
fn formats_activity_list_as_structured_response_per_currency() {
    let catalog = catalog();
    let plan = activity_plan();
    let result = json!({
        "row_count": 3,
        "rows": [
            {
                "transaction_id": 1,
                "transaction_date": "2026-07-05",
                "transaction_type_enum": 4,
                "amount": "7.03",
                "currency_code": "USD",
                "office_id": 1,
                "office_name": "Head Office",
                "product_id": 4,
                "product_name": "Saving Product - USD"
            },
            {
                "transaction_id": 2,
                "transaction_date": "2026-07-05",
                "transaction_type_enum": 5,
                "amount": "0.09",
                "currency_code": "AED",
                "office_id": 1,
                "office_name": "Head Office",
                "product_id": 5,
                "product_name": "Current Account With OD - AED"
            },
            {
                "transaction_id": 3,
                "transaction_date": "2026-07-04",
                "transaction_type_enum": 4,
                "amount": "0.09",
                "currency_code": "USD",
                "office_id": 1,
                "office_name": "Head Office",
                "product_id": 6,
                "product_name": "Current Account USD"
            }
        ]
    });

    let response = format_report_response(&catalog, &plan, &policy(false), &result).unwrap();
    let payload: serde_json::Value = serde_json::from_str(&response).unwrap();

    assert_eq!(
        payload["answer_plan"]["capability"],
        "savings_activity_list"
    );
    assert_eq!(
        payload["structured"]["by_currency"]["USD"]["charges_paid"]["count"],
        2
    );
    assert_eq!(
        payload["structured"]["by_currency"]["USD"]["charges_paid"]["total"],
        "7.12"
    );
    assert_eq!(
        payload["structured"]["by_currency"]["AED"]["charges_paid"]["count"],
        1
    );
    assert_eq!(
        payload["structured"]["by_currency"]["AED"]["charges_paid"]["total"],
        "0.09"
    );
    assert!(payload["message"].as_str().unwrap().contains("#### USD"));
    assert!(
        !payload["message"]
            .as_str()
            .unwrap()
            .contains("total: USD 7.21")
    );
}

fn plan(capability: &str, query_id: &str, output_mode: &str) -> ExecutionPlan {
    ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: "savings".to_string(),
        capability: capability.to_string(),
        query_id: query_id.to_string(),
        output_mode: output_mode.to_string(),
        params: json!({}),
        retrieval_plan: RetrievalPlan::default(),
        evidence_evaluation: EvidenceEvaluation::default(),
        answer_plan: AnswerPlan::default(),
        requires_policy_check: true,
    }
}

fn activity_plan() -> ExecutionPlan {
    ExecutionPlan {
        params: json!({
            "from_date": "2026-07-01",
            "to_date": "2026-07-05",
            "limit": 10,
        }),
        answer_plan: AnswerPlan {
            sections: vec!["overview".to_string(), "charges_paid".to_string()],
        },
        ..plan("savings_activity_list", "savings.activity_list", "list")
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
