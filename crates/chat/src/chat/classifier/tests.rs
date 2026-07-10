use super::*;

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 21).unwrap()
}

#[test]
fn classification_result_defaults_missing_layers() {
    let json = serde_json::json!({
        "outcome": "unsupported",
        "domain": null,
        "capability": null,
        "confidence": 0.0,
        "params": {},
        "clarification": null,
        "options": [],
        "source": "legacy",
        "candidates": []
    });

    let result: ClassificationResult = serde_json::from_value(json).expect("legacy result");

    assert!(result.layers.is_empty());
}

#[test]
fn parses_yesterday() {
    let range = date_range("total deposit yesterday", today()).unwrap();
    assert_eq!(
        range,
        (
            NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
        )
    );
}

#[test]
fn parses_kemarin() {
    let range = date_range("total setoran kemarin", today()).unwrap();
    assert_eq!(range.0, NaiveDate::from_ymd_opt(2026, 6, 20).unwrap());
}

#[test]
fn parses_this_year_and_ytd() {
    let r1 = date_range("deposits this year", today()).unwrap();
    let r2 = date_range("deposits ytd", today()).unwrap();
    let r3 = date_range("setoran tahun ini", today()).unwrap();
    let expected = (NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), today());
    assert_eq!(r1, expected);
    assert_eq!(r2, expected);
    assert_eq!(r3, expected);
}

#[test]
fn parses_last_year() {
    let range = date_range("deposits last year", today()).unwrap();
    assert_eq!(
        range,
        (
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
        )
    );
}

#[test]
fn parses_last_month() {
    let range = date_range("deposits last month", today()).unwrap();
    assert_eq!(
        range,
        (
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 5, 31).unwrap(),
        )
    );
}

#[test]
fn parses_last_month_january_wraps() {
    let today_jan = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    let range = date_range("deposits last month", today_jan).unwrap();
    assert_eq!(
        range,
        (
            NaiveDate::from_ymd_opt(2025, 12, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
        )
    );
}

#[test]
fn parses_last_week() {
    // today is 2026-06-21 (Sunday). this_monday = 2026-06-15. last_monday = 2026-06-08, last_sunday = 2026-06-14.
    let range = date_range("deposits last week", today()).unwrap();
    assert_eq!(
        range,
        (
            NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 14).unwrap(),
        )
    );
}

#[test]
fn parses_last_n_days() {
    let range = date_range("deposits last 7 days", today()).unwrap();
    assert_eq!(range.0, today() - chrono::Duration::days(7));
    assert_eq!(range.1, today());
}

#[test]
fn parses_last_n_months_id() {
    let range = date_range("setoran 3 bulan terakhir", today()).unwrap();
    // today is 2026-06-21; minus 3 months → 2026-03-21
    assert_eq!(range.0, NaiveDate::from_ymd_opt(2026, 3, 21).unwrap());
    assert_eq!(range.1, today());
}

#[test]
fn parses_bare_year() {
    let range = date_range("deposits in 2025", today()).unwrap();
    assert_eq!(
        range,
        (
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
        )
    );
}

#[test]
fn parses_month_range_default_year() {
    // No year token; should default to today.year() = 2026.
    let range = date_range("deposits from January to September", today()).unwrap();
    assert_eq!(
        range,
        (
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
        )
    );
}

#[test]
fn parses_month_range_with_year() {
    let range = date_range("deposits from January to September 2025", today()).unwrap();
    assert_eq!(
        range,
        (
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 9, 30).unwrap(),
        )
    );
}

#[test]
fn parses_id_month_range_with_sampai() {
    let range = date_range("setoran dari Januari sampai September 2026", today()).unwrap();
    assert_eq!(
        range,
        (
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
        )
    );
}

#[test]
fn classifies_clarification_response_with_original_dates() {
    let original = clarify_retrieved_capabilities(
        "Show customer activity this week",
        today(),
        Some("savings".to_string()),
        vec![ClarificationOption {
            label: "Total deposit this week".to_string(),
            capability: "savings_deposit_total".to_string(),
            output_mode: Some("total".to_string()),
        }],
        0.7,
        Vec::new(),
    );
    let result = classify_clarification_response(&original, "Total deposits");

    assert_eq!(result.outcome, ClassificationOutcome::Matched);
    assert_eq!(result.capability.as_deref(), Some("savings_deposit_total"));
    assert_eq!(result.params["from_date"], "2026-06-15");
}

#[test]
fn classifies_withdrawal_monthly_clarification_response() {
    let original = clarify_retrieved_capabilities(
        "Show savings activity this month",
        today(),
        Some("savings".to_string()),
        vec![ClarificationOption {
            label: "Monthly withdrawal breakdown this month".to_string(),
            capability: "savings_withdrawal_monthly_breakdown".to_string(),
            output_mode: Some("monthly_breakdown".to_string()),
        }],
        0.7,
        Vec::new(),
    );

    let result = classify_clarification_response(&original, "monthly withdrawal breakdown");

    assert_eq!(result.outcome, ClassificationOutcome::Matched);
    assert_eq!(
        result.capability.as_deref(),
        Some("savings_withdrawal_monthly_breakdown")
    );
    assert_eq!(result.params["from_date"], "2026-06-01");
}

#[test]
fn parses_indonesian_month_range() {
    let result = classify_retrieved_capability(
        "saya mau tau deposit bulan mei - september 2025",
        today(),
        "savings",
        "savings_deposit_total",
        "total",
        true,
        0.7,
        Vec::new(),
    );

    assert_eq!(result.outcome, ClassificationOutcome::Matched);
    assert_eq!(result.params["from_date"], "2025-05-01");
    assert_eq!(result.params["to_date"], "2025-09-30");
}

#[test]
fn classifies_retrieved_top_n_capability_with_params() {
    let result = classify_retrieved_capability(
        "Show customer savings activity this week top 7",
        today(),
        "savings",
        "savings_deposit_top_n",
        "top_n",
        true,
        0.72,
        Vec::new(),
    );

    assert_eq!(result.outcome, ClassificationOutcome::Matched);
    assert_eq!(result.capability.as_deref(), Some("savings_deposit_top_n"));
    assert_eq!(result.params["from_date"], "2026-06-15");
    assert_eq!(result.params["limit"], 7);
}

#[test]
fn clarification_candidates_keep_requested_limit() {
    let result = clarify_retrieved_capabilities(
        "show 10 clients with the most savings accounts",
        today(),
        Some("client".to_string()),
        vec![ClarificationOption {
            label: "Top Clients by Number of Savings Accounts".to_string(),
            capability: "client_top_n_by_savings_account_count".to_string(),
            output_mode: Some("top_n".to_string()),
        }],
        0.72,
        Vec::new(),
    );

    assert_eq!(result.params["limit"], 10);
}

#[test]
fn classifies_snapshot_top_n_without_date_range() {
    let result = classify_retrieved_capability(
        "Show top clients by current savings balance",
        today(),
        "client",
        "client_top_n_by_savings_balance",
        "top_n",
        false,
        0.72,
        Vec::new(),
    );

    assert_eq!(result.outcome, ClassificationOutcome::Matched);
    assert_eq!(
        result.capability.as_deref(),
        Some("client_top_n_by_savings_balance")
    );
    assert_eq!(result.params["limit"], 10);
    assert!(result.params.get("from_date").is_none());
    assert!(result.params.get("to_date").is_none());
}

#[test]
fn list_limit_ignores_relative_date_number() {
    let result = classify_retrieved_capability(
        "show me list of saving activity for 3 months ago from now",
        today(),
        "savings",
        "savings_activity_list",
        "list",
        true,
        0.72,
        Vec::new(),
    );

    assert_eq!(result.outcome, ClassificationOutcome::Matched);
    assert_eq!(result.params["from_date"], "2026-03-21");
    assert_eq!(result.params["to_date"], "2026-06-21");
    assert_eq!(result.params["limit"], 10);
}

#[test]
fn all_activity_list_has_no_default_limit() {
    let result = classify_retrieved_capability(
        "show me the list of all saving activity for this month",
        today(),
        "savings",
        "savings_activity_list",
        "list",
        true,
        0.72,
        Vec::new(),
    );

    assert_eq!(result.outcome, ClassificationOutcome::Matched);
    assert_eq!(result.params["from_date"], "2026-06-01");
    assert!(result.params.get("limit").is_none());
}

#[test]
fn classifies_numeric_clarification_option() {
    let original = clarify_retrieved_capabilities(
        "Show customer activity this week",
        today(),
        Some("savings".to_string()),
        vec![
            ClarificationOption {
                label: "Total deposits".to_string(),
                capability: "savings_deposit_total".to_string(),
                output_mode: Some("total".to_string()),
            },
            ClarificationOption {
                label: "Largest deposits".to_string(),
                capability: "savings_deposit_top_n".to_string(),
                output_mode: Some("top_n".to_string()),
            },
        ],
        0.7,
        Vec::new(),
    );

    let result = classify_clarification_response(&original, "2");

    assert_eq!(result.outcome, ClassificationOutcome::Matched);
    assert_eq!(result.capability.as_deref(), Some("savings_deposit_top_n"));
}

#[test]
fn classifies_all_activity_clarification_by_meaning() {
    let original = clarify_retrieved_capabilities(
        "Show customer activity this week",
        today(),
        Some("savings".to_string()),
        vec![
            ClarificationOption {
                label: "List activity this week".to_string(),
                capability: "savings_activity_list".to_string(),
                output_mode: Some("list".to_string()),
            },
            ClarificationOption {
                label: "Largest deposit this week".to_string(),
                capability: "savings_deposit_top_n".to_string(),
                output_mode: Some("top_n".to_string()),
            },
            ClarificationOption {
                label: "Total deposit this week".to_string(),
                capability: "savings_deposit_total".to_string(),
                output_mode: Some("total".to_string()),
            },
            ClarificationOption {
                label: "Other activity this week".to_string(),
                capability: OTHER_ACTIVITY_CAPABILITY.to_string(),
                output_mode: None,
            },
        ],
        0.7,
        Vec::new(),
    );

    let result = classify_clarification_response(&original, "all acticity for this week");

    assert_eq!(result.outcome, ClassificationOutcome::Matched);
    assert_eq!(result.capability.as_deref(), Some("savings_activity_list"));
}

#[test]
fn picking_other_resets_state_to_free_form_clarification() {
    // New semantics: picking Others is an escape hatch. The classifier drops
    // the prior options, params and domain so the user's next reply is treated
    // as a brand-new prompt. Re-classification of that next reply is the
    // service layer's job (`classify_with_retrieval`), not the classifier's.
    let original = clarify_retrieved_capabilities(
        "Show customer activity this week",
        today(),
        Some("savings".to_string()),
        vec![
            ClarificationOption {
                label: "List activity this week".to_string(),
                capability: "savings_activity_list".to_string(),
                output_mode: Some("list".to_string()),
            },
            ClarificationOption {
                label: "Total deposit this week".to_string(),
                capability: "savings_deposit_total".to_string(),
                output_mode: Some("total".to_string()),
            },
            ClarificationOption {
                label: "Others — let me describe it in my own words".to_string(),
                capability: OTHER_ACTIVITY_CAPABILITY.to_string(),
                output_mode: None,
            },
        ],
        0.7,
        Vec::new(),
    );

    let other = classify_clarification_response(&original, "other");

    assert_eq!(other.outcome, ClassificationOutcome::ClarificationRequired);
    assert!(other.options.is_empty(), "options should be reset");
    assert_eq!(other.domain, None, "domain should be reset");
    assert_eq!(
        other.params,
        serde_json::json!({}),
        "params should be reset"
    );
    assert_eq!(
        other.source.as_deref(),
        Some("clarification_other_selected")
    );
}

#[test]
fn classifies_free_text_clarification_by_closest_meaning() {
    let original = clarify_retrieved_capabilities(
        "Show customer activity this week",
        today(),
        Some("savings".to_string()),
        vec![
            ClarificationOption {
                label: "Largest deposit this week".to_string(),
                capability: "savings_deposit_top_n".to_string(),
                output_mode: Some("top_n".to_string()),
            },
            ClarificationOption {
                label: "Total deposit this week".to_string(),
                capability: "savings_deposit_total".to_string(),
                output_mode: Some("total".to_string()),
            },
            ClarificationOption {
                label: "Other activity this week".to_string(),
                capability: OTHER_ACTIVITY_CAPABILITY.to_string(),
                output_mode: None,
            },
        ],
        0.7,
        Vec::new(),
    );

    let result =
        classify_clarification_response(&original, "Maybe the largest deposit is good choice");

    assert_eq!(result.outcome, ClassificationOutcome::Matched);
    assert_eq!(result.capability.as_deref(), Some("savings_deposit_top_n"));
}
