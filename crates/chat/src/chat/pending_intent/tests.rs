use chrono::NaiveDate;
use serde_json::json;

use super::*;
use crate::knowledge::model::{CapabilityKnowledge, KnowledgeCatalog};

#[test]
fn date_response_fills_pending_intent_without_changing_capability() {
    let catalog = catalog(vec![capability(
        "client_top_n_by_deposit_volume",
        "client",
        "top_n",
        vec!["from_date", "to_date", "limit"],
    )]);
    let pending = PendingIntent {
        schema_version: 1,
        revision: 1,
        original_message: "show clients with highest deposit volume".to_string(),
        status: PendingIntentStatus::CollectingSlots,
        domain: Some("client".to_string()),
        target_entity: Some("client".to_string()),
        requested_shape: Some("top_n".to_string()),
        metric: Some("deposit_volume".to_string()),
        candidate_capabilities: vec!["client_top_n_by_deposit_volume".to_string()],
        selected_capability: Some("client_top_n_by_deposit_volume".to_string()),
        params: json!({ "limit": 10, "office_scope": "authorized_scope" }),
        missing_slots: vec!["from_date".to_string(), "to_date".to_string()],
        last_user_response: None,
        invalid_attempts: 0,
    };

    let result = resolve_pending_intent(
        Some(pending),
        "2 months ago from now",
        NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(),
        &catalog,
    );

    match result {
        PendingIntentResolution::Matched(classification, pending) => {
            assert_eq!(
                classification.capability.as_deref(),
                Some("client_top_n_by_deposit_volume")
            );
            assert_eq!(pending.status, PendingIntentStatus::Resolved);
            assert_eq!(classification.params["limit"], 10);
            assert_eq!(classification.params["from_date"], "2026-05-09");
            assert_eq!(classification.params["to_date"], "2026-07-09");
        }
        _ => panic!("expected matched pending intent"),
    }
}

#[test]
fn single_complete_candidate_resolves_without_reclassification() {
    let catalog = catalog(vec![capability(
        "client_top_n_by_savings_account_count",
        "client",
        "top_n",
        vec!["limit"],
    )]);
    let pending = PendingIntent {
        schema_version: 1,
        revision: 1,
        original_message: "show 10 clients with most savings accounts".to_string(),
        status: PendingIntentStatus::CollectingSlots,
        domain: Some("client".to_string()),
        target_entity: Some("client".to_string()),
        requested_shape: Some("top_n".to_string()),
        metric: Some("savings_account_count".to_string()),
        candidate_capabilities: vec!["client_top_n_by_savings_account_count".to_string()],
        selected_capability: None,
        params: json!({ "limit": 10, "office_scope": "authorized_scope" }),
        missing_slots: Vec::new(),
        last_user_response: None,
        invalid_attempts: 0,
    };

    let result = resolve_pending_intent(
        Some(pending),
        "ok",
        NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(),
        &catalog,
    );

    match result {
        PendingIntentResolution::Matched(classification, pending) => {
            assert_eq!(
                classification.capability.as_deref(),
                Some("client_top_n_by_savings_account_count")
            );
            assert_eq!(pending.status, PendingIntentStatus::Resolved);
            assert_eq!(classification.source.as_deref(), Some("pending_intent"));
        }
        _ => panic!("expected matched pending intent"),
    }
}

#[test]
fn irrelevant_response_does_not_resolve_complete_pending_intent() {
    let catalog = catalog(vec![capability(
        "client_top_n_by_savings_account_count",
        "client",
        "top_n",
        vec!["limit"],
    )]);
    let pending = PendingIntent {
        schema_version: 1,
        revision: 1,
        original_message: "show 10 clients with most savings accounts".to_string(),
        status: PendingIntentStatus::CollectingSlots,
        domain: Some("client".to_string()),
        target_entity: Some("client".to_string()),
        requested_shape: Some("top_n".to_string()),
        metric: Some("savings_account_count".to_string()),
        candidate_capabilities: vec!["client_top_n_by_savings_account_count".to_string()],
        selected_capability: None,
        params: json!({ "limit": 10, "office_scope": "authorized_scope" }),
        missing_slots: Vec::new(),
        last_user_response: None,
        invalid_attempts: 0,
    };

    let result = resolve_pending_intent(
        Some(pending),
        "all acticity",
        NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(),
        &catalog,
    );

    match result {
        PendingIntentResolution::StillWaiting(classification, pending) => {
            assert_eq!(pending.invalid_attempts, 1);
            assert_eq!(pending.selected_capability, None);
            assert_eq!(
                classification.outcome,
                ClassificationOutcome::ClarificationRequired
            );
        }
        _ => panic!("expected pending intent to keep waiting"),
    }
}

#[test]
fn semantic_reply_selects_savings_balance_candidate() {
    let mut balance = capability(
        "client_top_n_by_savings_balance",
        "client",
        "top_n",
        vec!["limit"],
    );
    balance.display_name = Some("Top Clients by Savings Balance".to_string());
    balance.description = Some("Ranks clients by total savings account balance.".to_string());
    let mut count = capability(
        "client_top_n_by_savings_account_count",
        "client",
        "top_n",
        vec!["limit"],
    );
    count.display_name = Some("Top Clients by Savings Account Count".to_string());
    count.description = Some("Ranks clients by number of savings accounts.".to_string());
    let catalog = catalog(vec![balance, count]);
    let pending = PendingIntent {
        schema_version: 1,
        revision: 1,
        original_message: "show 10 clients with most savings accounts".to_string(),
        status: PendingIntentStatus::WaitingForCapabilityChoice,
        domain: Some("client".to_string()),
        target_entity: Some("client".to_string()),
        requested_shape: Some("top_n".to_string()),
        metric: None,
        candidate_capabilities: vec![
            "client_top_n_by_savings_balance".to_string(),
            "client_top_n_by_savings_account_count".to_string(),
        ],
        selected_capability: None,
        params: json!({ "limit": 10, "office_scope": "authorized_scope" }),
        missing_slots: Vec::new(),
        last_user_response: None,
        invalid_attempts: 0,
    };

    let result = resolve_pending_intent(
        Some(pending),
        "the balance one please",
        NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(),
        &catalog,
    );

    match result {
        PendingIntentResolution::Matched(classification, _) => {
            assert_eq!(
                classification.capability.as_deref(),
                Some("client_top_n_by_savings_balance")
            );
        }
        _ => panic!("expected semantic balance match"),
    }
}

#[test]
fn indonesian_ordinal_selects_candidate_by_order() {
    let catalog = catalog(vec![
        capability(
            "client_top_n_by_savings_account_count",
            "client",
            "top_n",
            vec!["limit"],
        ),
        capability(
            "client_top_n_by_savings_balance",
            "client",
            "top_n",
            vec!["limit"],
        ),
    ]);
    let pending = PendingIntent {
        schema_version: 1,
        revision: 1,
        original_message: "show 10 clients with most savings accounts".to_string(),
        status: PendingIntentStatus::WaitingForCapabilityChoice,
        domain: Some("client".to_string()),
        target_entity: Some("client".to_string()),
        requested_shape: Some("top_n".to_string()),
        metric: None,
        candidate_capabilities: vec![
            "client_top_n_by_savings_account_count".to_string(),
            "client_top_n_by_savings_balance".to_string(),
        ],
        selected_capability: None,
        params: json!({ "limit": 10 }),
        missing_slots: Vec::new(),
        last_user_response: None,
        invalid_attempts: 0,
    };

    let result = resolve_pending_intent(
        Some(pending),
        "yang kedua",
        NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(),
        &catalog,
    );

    match result {
        PendingIntentResolution::Matched(classification, _) => assert_eq!(
            classification.capability.as_deref(),
            Some("client_top_n_by_savings_balance")
        ),
        _ => panic!("expected ordinal match"),
    }
}

#[test]
fn pending_clarification_always_offers_others_escape_hatch() {
    let catalog = catalog(vec![capability(
        "client_top_n_by_savings_balance",
        "client",
        "top_n",
        vec!["limit"],
    )]);
    let pending = PendingIntent {
        schema_version: 1,
        revision: 1,
        original_message: "show 10 clients with most savings accounts".to_string(),
        status: PendingIntentStatus::WaitingForCapabilityChoice,
        domain: Some("client".to_string()),
        target_entity: Some("client".to_string()),
        requested_shape: Some("top_n".to_string()),
        metric: None,
        candidate_capabilities: vec!["client_top_n_by_savings_balance".to_string()],
        selected_capability: None,
        params: json!({}),
        missing_slots: Vec::new(),
        last_user_response: None,
        invalid_attempts: 0,
    };

    let result = resolve_pending_intent(
        Some(pending),
        "not that",
        NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(),
        &catalog,
    );

    match result {
        PendingIntentResolution::StillWaiting(classification, _) => {
            assert!(classification.options.iter().any(|option| {
                option.capability == crate::chat::classifier::OTHER_ACTIVITY_CAPABILITY
            }));
        }
        _ => panic!("expected still waiting with Others option"),
    }
}

#[test]
fn pending_others_selection_resets_to_free_form_clarification() {
    let catalog = catalog(vec![capability(
        "client_top_n_by_savings_balance",
        "client",
        "top_n",
        vec!["limit"],
    )]);
    let pending = PendingIntent {
        schema_version: 1,
        revision: 1,
        original_message: "show 10 clients with most savings accounts".to_string(),
        status: PendingIntentStatus::WaitingForCapabilityChoice,
        domain: Some("client".to_string()),
        target_entity: Some("client".to_string()),
        requested_shape: Some("top_n".to_string()),
        metric: None,
        candidate_capabilities: vec!["client_top_n_by_savings_balance".to_string()],
        selected_capability: None,
        params: json!({ "limit": 10 }),
        missing_slots: Vec::new(),
        last_user_response: None,
        invalid_attempts: 0,
    };

    let result = resolve_pending_intent(
        Some(pending),
        "others",
        NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(),
        &catalog,
    );

    match result {
        PendingIntentResolution::StillWaiting(classification, pending) => {
            assert_eq!(pending.status, PendingIntentStatus::Resolved);
            assert!(classification.options.is_empty());
            assert_eq!(classification.params, json!({}));
            assert_eq!(
                classification.source.as_deref(),
                Some("clarification_other_selected")
            );
        }
        _ => panic!("expected Others free-form clarification"),
    }
}

#[test]
fn selected_candidate_waits_for_limit_then_numeric_limit_resolves() {
    let catalog = catalog(vec![capability(
        "client_top_n_by_savings_account_count",
        "client",
        "top_n",
        vec!["limit"],
    )]);
    let pending = PendingIntent {
        schema_version: 1,
        revision: 1,
        original_message: "show clients with most savings accounts".to_string(),
        status: PendingIntentStatus::WaitingForCapabilityChoice,
        domain: Some("client".to_string()),
        target_entity: Some("client".to_string()),
        requested_shape: Some("top_n".to_string()),
        metric: None,
        candidate_capabilities: vec!["client_top_n_by_savings_account_count".to_string()],
        selected_capability: None,
        params: json!({ "office_scope": "authorized_scope" }),
        missing_slots: Vec::new(),
        last_user_response: None,
        invalid_attempts: 0,
    };

    let waiting = resolve_pending_intent(
        Some(pending),
        "Top Clients by Number of Savings Accounts",
        NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(),
        &catalog,
    );

    let pending = match waiting {
        PendingIntentResolution::StillWaiting(classification, pending) => {
            assert_eq!(
                pending.selected_capability.as_deref(),
                Some("client_top_n_by_savings_account_count")
            );
            assert_eq!(pending.missing_slots, vec!["limit".to_string()]);
            assert!(classification.options.is_empty());
            pending
        }
        _ => panic!("expected pending limit"),
    };

    let resolved = resolve_pending_intent(
        Some(pending),
        "10",
        NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(),
        &catalog,
    );

    match resolved {
        PendingIntentResolution::Matched(classification, pending) => {
            assert_eq!(
                classification.capability.as_deref(),
                Some("client_top_n_by_savings_account_count")
            );
            assert_eq!(classification.params["limit"], 10);
            assert_eq!(pending.status, PendingIntentStatus::Resolved);
        }
        _ => panic!("expected numeric limit to resolve"),
    }
}

#[test]
fn free_form_reply_to_active_pending_intent_starts_new_request() {
    let catalog = catalog(vec![capability(
        "client_top_n_by_savings_account_count",
        "client",
        "top_n",
        vec!["limit"],
    )]);
    let pending = PendingIntent {
        schema_version: 1,
        revision: 1,
        original_message: "show clients with most savings accounts".to_string(),
        status: PendingIntentStatus::WaitingForCapabilityChoice,
        domain: Some("client".to_string()),
        target_entity: Some("client".to_string()),
        requested_shape: Some("top_n".to_string()),
        metric: None,
        candidate_capabilities: vec!["client_top_n_by_savings_account_count".to_string()],
        selected_capability: None,
        params: json!({ "office_scope": "authorized_scope" }),
        missing_slots: Vec::new(),
        last_user_response: None,
        invalid_attempts: 0,
    };

    let result = resolve_pending_intent(
        Some(pending),
        "show savings balance by office instead",
        NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(),
        &catalog,
    );

    match result {
        PendingIntentResolution::StartNewRequest => {}
        _ => panic!("expected free-form response to start a new request"),
    }
}

#[test]
fn resolved_pending_intent_is_not_active() {
    let pending = PendingIntent {
        schema_version: 1,
        revision: 2,
        original_message: "show 10 clients with most savings accounts".to_string(),
        status: PendingIntentStatus::Resolved,
        domain: Some("client".to_string()),
        target_entity: Some("client".to_string()),
        requested_shape: Some("top_n".to_string()),
        metric: Some("savings_account_count".to_string()),
        candidate_capabilities: vec!["client_top_n_by_savings_account_count".to_string()],
        selected_capability: Some("client_top_n_by_savings_account_count".to_string()),
        params: json!({ "limit": 10, "office_scope": "authorized_scope" }),
        missing_slots: Vec::new(),
        last_user_response: Some("ok".to_string()),
        invalid_attempts: 0,
    };

    assert!(!pending.is_active());
}

fn capability(
    id: &str,
    domain: &str,
    output_mode: &str,
    required_parameters: Vec<&str>,
) -> CapabilityKnowledge {
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
        required_parameters: required_parameters
            .into_iter()
            .map(str::to_string)
            .collect(),
        optional_parameters: Vec::new(),
    }
}

fn catalog(capabilities: Vec<CapabilityKnowledge>) -> KnowledgeCatalog {
    KnowledgeCatalog {
        root_path: "knowledge".into(),
        query_path: "queries".into(),
        data_areas: Vec::new(),
        domains: Vec::new(),
        schemas: Vec::new(),
        metrics: Vec::new(),
        capabilities,
        queries: Vec::new(),
        policies: Vec::new(),
        responses: Vec::new(),
        classification: Default::default(),
    }
}
