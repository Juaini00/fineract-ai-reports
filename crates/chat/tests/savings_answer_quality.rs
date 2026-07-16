mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use sqlx::types::Json;
use std::time::{Duration, Instant};

const POLL_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const OFFICE_IDS: &[i64] = &[1, 2, 3, 4, 40];
#[tokio::test(flavor = "multi_thread")]
async fn savings_answer_matrix_selects_expected_capabilities_and_shapes() {
    let app = spawn_app().await;

    for case in cases() {
        let key = app
            .provision_api_key(&[case.capability], OFFICE_IDS.to_vec(), true)
            .await;
        let session_id = create_session(&app, &key.raw, "savings answer quality").await;
        let job = run_prompt(&app, &key.raw, &session_id, case.prompt, case.capability).await;
        assert_completed_answer(&app, &job, &case).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn savings_answer_respects_narrow_office_scope() {
    let app = spawn_app().await;
    let case = case_for_capability("savings_deposit_total");
    let wide_expected = independent_rows(&app, &case, OFFICE_IDS).await;
    let mut found = None;
    for &office_id in OFFICE_IDS {
        let offices = vec![office_id];
        let expected = independent_rows(&app, &case, &offices).await;
        if expected != wide_expected {
            found = Some((offices, expected));
            break;
        }
    }
    let (narrow_offices, narrow_expected) = found.unwrap_or_else(|| {
        panic!(
            "no office fixture proves narrow savings scope for {}",
            case.capability
        )
    });
    let wide_key = app
        .provision_api_key(&[case.capability], OFFICE_IDS.to_vec(), true)
        .await;
    let narrow_key = app
        .provision_api_key(&[case.capability], narrow_offices.clone(), true)
        .await;
    let wide_session = create_session(&app, &wide_key.raw, "wide savings scope").await;
    let narrow_session = create_session(&app, &narrow_key.raw, "narrow savings scope").await;
    let wide = run_prompt(
        &app,
        &wide_key.raw,
        &wide_session,
        case.prompt,
        case.capability,
    )
    .await;
    let narrow = run_prompt(
        &app,
        &narrow_key.raw,
        &narrow_session,
        case.prompt,
        case.capability,
    )
    .await;
    assert_eq!(
        wide["result_json"]["structured_response"]["table"]["rows"],
        wide_expected
    );
    assert_eq!(
        narrow["result_json"]["structured_response"]["table"]["rows"],
        narrow_expected
    );
    assert_ne!(wide_expected, narrow_expected);
}

#[tokio::test(flavor = "multi_thread")]
async fn savings_clarification_keeps_selected_capability_for_parameter_only_reply() {
    let app = spawn_app().await;
    let case = case_for_capability("savings_deposit_total");
    let key = app
        .provision_api_key(&[case.capability], OFFICE_IDS.to_vec(), true)
        .await;
    let session_id = create_session(&app, &key.raw, "savings clarification quality").await;
    let job_id = create_job(&app, &key.raw, &session_id, "I need a savings report").await;
    let first = wait_until_not_running(&app, &key.raw, &job_id).await;

    assert_eq!(first["status"], "waiting_for_user_input", "{first}");
    let response = &first["result_json"]["structured_response"];
    assert_eq!(response["response_type"], "clarification", "{first}");
    assert!(
        option_ids(response).contains(&"savings_deposit_total"),
        "missing deposit-total option: {first}"
    );
    assert_no_legacy_empty_options_loop(response);
    assert_no_response_leak(response, &first["result_json"]["markdown"]);

    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(&key.raw),
            &json!({
                "message": "Show total deposit amount and deposit count for savings accounts from 2026-01-01",
                "option_id": "savings_deposit_total"
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        201,
        "response failed: {}",
        resp.text().await.unwrap_or_default()
    );
    let selected = wait_until_not_running(&app, &key.raw, &job_id).await;
    assert_eq!(selected["status"], "waiting_for_user_input", "{selected}");
    let response = &selected["result_json"]["structured_response"];
    assert_eq!(
        selected["result_json"]["selected_capability"],
        case.capability
    );
    assert!(
        response["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("missing parameter ")),
        "{selected}"
    );
    assert_no_legacy_empty_options_loop(response);
    assert_no_response_leak(response, &selected["result_json"]["markdown"]);

    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(&key.raw),
            &json!({ "message": "2026-01-01 to 2026-12-31" }),
        )
        .await;
    assert_eq!(
        resp.status(),
        201,
        "response failed: {}",
        resp.text().await.unwrap_or_default()
    );
    let final_job = wait_until_not_running(&app, &key.raw, &job_id).await;
    assert_completed_answer(&app, &final_job, &case).await;
}

struct Case {
    prompt: &'static str,
    capability: &'static str,
    output_mode: &'static str,
    fields: &'static [&'static str],
    max_rows: Option<usize>,
    from_date: Option<&'static str>,
    to_date: Option<&'static str>,
    limit: Option<i64>,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            prompt: "total savings deposits from 2026-01-01 to 2026-12-31",
            capability: "savings_deposit_total",
            output_mode: "total",
            fields: &[
                "from_date",
                "to_date",
                "total_deposit_amount",
                "deposit_count",
            ],
            max_rows: None,
            from_date: Some("2026-01-01"),
            to_date: Some("2026-12-31"),
            limit: None,
        },
        Case {
            prompt: "top 5 savings deposits from 2026-01-01 to 2026-12-31",
            capability: "savings_deposit_top_n",
            output_mode: "top_n",
            fields: &[
                "transaction_date",
                "amount",
                "currency_code",
                "office_name",
                "product_name",
            ],
            max_rows: Some(5),
            from_date: Some("2026-01-01"),
            to_date: Some("2026-12-31"),
            limit: Some(5),
        },
        Case {
            prompt: "monthly savings deposit totals from 2026-01-01 to 2026-09-30",
            capability: "savings_deposit_monthly_breakdown",
            output_mode: "monthly_breakdown",
            fields: &["month_start", "total_deposit_amount", "deposit_count"],
            max_rows: Some(9),
            from_date: Some("2026-01-01"),
            to_date: Some("2026-09-30"),
            limit: None,
        },
        Case {
            prompt: "top 2 savings deposits per month from 2026-01-01 to 2026-09-30",
            capability: "savings_deposit_monthly_top_n",
            output_mode: "monthly_top_n",
            fields: &[
                "month_start",
                "transaction_date",
                "amount",
                "office_name",
                "product_name",
            ],
            max_rows: Some(18),
            from_date: Some("2026-01-01"),
            to_date: Some("2026-09-30"),
            limit: Some(2),
        },
        Case {
            prompt: "Show the savings portfolio summary",
            capability: "savings_balance_summary",
            output_mode: "summary",
            fields: &[
                "account_count",
                "total_balance",
                "average_balance",
                "max_balance",
            ],
            max_rows: None,
            from_date: None,
            to_date: None,
            limit: None,
        },
        Case {
            prompt: "total savings withdrawals from 2026-01-01 to 2026-12-31",
            capability: "savings_withdrawal_total",
            output_mode: "total",
            fields: &[
                "from_date",
                "to_date",
                "total_withdrawal_amount",
                "withdrawal_count",
            ],
            max_rows: None,
            from_date: Some("2026-01-01"),
            to_date: Some("2026-12-31"),
            limit: None,
        },
        Case {
            prompt: "top 5 savings withdrawals from 2026-01-01 to 2026-12-31",
            capability: "savings_withdrawal_top_n",
            output_mode: "top_n",
            fields: &[
                "transaction_date",
                "amount",
                "currency_code",
                "office_name",
                "product_name",
            ],
            max_rows: Some(5),
            from_date: Some("2026-01-01"),
            to_date: Some("2026-12-31"),
            limit: Some(5),
        },
    ]
}

fn case_for_capability(capability: &str) -> Case {
    cases()
        .into_iter()
        .find(|case| case.capability == capability)
        .unwrap_or_else(|| panic!("missing test case for {capability}"))
}

async fn run_prompt(
    app: &TestApp,
    api_key: &str,
    session_id: &str,
    prompt: &str,
    capability: &str,
) -> Value {
    let job_id = create_job(app, api_key, session_id, prompt).await;
    let mut job = wait_until_not_running(app, api_key, &job_id).await;
    for _ in 0..3 {
        if job["status"] != "waiting_for_user_input" {
            return job;
        }
        let response = &job["result_json"]["structured_response"];
        let ids = option_ids(response);
        assert_no_legacy_empty_options_loop(response);
        let body = if ids.contains(&capability) {
            json!({ "message": prompt, "option_id": capability })
        } else if ids.contains(&"others") {
            json!({ "message": "others", "option_id": "others" })
        } else if ids.is_empty() {
            json!({ "message": prompt })
        } else {
            panic!("missing {capability} option for {prompt}: {job}");
        };
        let resp = app
            .post_json(
                &format!("/chat/jobs/{job_id}/responses"),
                Some(api_key),
                &body,
            )
            .await;
        assert_eq!(
            resp.status(),
            201,
            "clarification response failed for {prompt}"
        );
        job = wait_until_not_running(app, api_key, &job_id).await;
    }
    job
}

async fn assert_completed_answer(app: &TestApp, job: &Value, case: &Case) {
    assert_eq!(job["status"], "completed", "{}: {job}", case.prompt);
    let result = &job["result_json"];
    assert_eq!(
        result["selected_capability"], case.capability,
        "{}: {job}",
        case.prompt
    );
    assert_eq!(
        result["structured_response"]["response_type"], "table",
        "{}: {job}",
        case.prompt
    );
    assert_no_response_leak(&result["structured_response"], &result["markdown"]);
    assert_table_contract(
        &result["structured_response"],
        case.fields,
        case.max_rows,
        job,
    );
    assert_eq!(
        result["structured_response"]["table"]["rows"],
        independent_rows(app, case, OFFICE_IDS).await,
        "independent Fineract values differ for {}",
        case.capability
    );

    let memory: Value = sqlx::query_scalar(
        "SELECT execution_summary_json FROM assistant_job_memory WHERE job_id = $1::uuid",
    )
    .bind(job["id"].as_str().unwrap())
    .fetch_one(&app.app_pool)
    .await
    .unwrap();
    assert_eq!(
        memory["plan"]["capability"], case.capability,
        "{}: {memory}",
        case.prompt
    );
    assert_eq!(
        memory["plan"]["output_mode"], case.output_mode,
        "{}: {memory}",
        case.prompt
    );
    if case.output_mode != "summary" {
        assert!(
            memory["plan"]["params"].get("from_date").is_some(),
            "missing from_date: {memory}"
        );
        assert!(
            memory["plan"]["params"].get("to_date").is_some(),
            "missing to_date: {memory}"
        );
    }
}

async fn independent_rows(app: &TestApp, case: &Case, office_ids: &[i64]) -> Value {
    let dates = (case.from_date.unwrap_or(""), case.to_date.unwrap_or(""));
    let transaction_rows = r#"
        SELECT COALESCE(jsonb_agg(row_data ORDER BY amount DESC, transaction_date DESC, transaction_id DESC), '[]'::jsonb)
        FROM (
            SELECT jsonb_build_object(
                'transaction_id', t.id, 'transaction_date', t.transaction_date,
                'amount', t.amount::text, 'currency_code', a.currency_code,
                'office_id', t.office_id, 'office_name', o.name,
                'product_id', a.product_id, 'product_name', p.name,
                'client_id', c.id, 'client_display_name', c.display_name
            ) AS row_data, t.amount, t.transaction_date, t.id AS transaction_id
            FROM m_savings_account_transaction t
            JOIN m_savings_account a ON a.id = t.savings_account_id
            JOIN m_savings_product p ON p.id = a.product_id
            JOIN m_office o ON o.id = t.office_id
            LEFT JOIN m_client c ON c.id = a.client_id
            WHERE t.transaction_type_enum = $1 AND NOT t.is_reversed
              AND t.transaction_date BETWEEN $2::date AND $3::date
              AND t.office_id = ANY($4::bigint[])
            ORDER BY t.amount DESC, t.transaction_date DESC, t.id DESC
            LIMIT $5
        ) ranked
    "#;
    match case.capability {
        "savings_deposit_total" | "savings_withdrawal_total" => {
            let (amount_key, count_key, kind) = if case.capability == "savings_deposit_total" {
                ("total_deposit_amount", "deposit_count", 1_i32)
            } else {
                ("total_withdrawal_amount", "withdrawal_count", 2_i32)
            };
            sqlx::query_scalar::<_, Json<Value>>(
                "SELECT jsonb_build_array(jsonb_build_object('from_date', $1::date, 'to_date', $5::date, $2, COALESCE(SUM(t.amount), 0)::text, $3, COUNT(*))) FROM m_savings_account_transaction t JOIN m_savings_account a ON a.id = t.savings_account_id JOIN m_savings_product p ON p.id = a.product_id JOIN m_office o ON o.id = t.office_id LEFT JOIN m_client c ON c.id = a.client_id WHERE t.transaction_type_enum = $4 AND NOT t.is_reversed AND t.transaction_date BETWEEN $1::date AND $5::date AND t.office_id = ANY($6::bigint[])",
            )
            .bind(dates.0).bind(amount_key).bind(count_key).bind(kind).bind(dates.1).bind(office_ids).fetch_one(&app.fineract).await.unwrap().0
        }
        "savings_deposit_top_n" | "savings_withdrawal_top_n" => {
            let kind = if case.capability == "savings_deposit_top_n" { 1_i32 } else { 2_i32 };
            sqlx::query_scalar::<_, Json<Value>>(transaction_rows)
                .bind(kind).bind(dates.0).bind(dates.1).bind(office_ids).bind(case.limit.unwrap()).fetch_one(&app.fineract).await.unwrap().0
        }
        "savings_deposit_monthly_breakdown" => sqlx::query_scalar::<_, Json<Value>>(
            "SELECT COALESCE(jsonb_agg(row_data ORDER BY month_start), '[]'::jsonb) FROM (SELECT jsonb_build_object('month_start', date_trunc('month', t.transaction_date)::date, 'total_deposit_amount', SUM(t.amount)::text, 'deposit_count', COUNT(*)) AS row_data, date_trunc('month', t.transaction_date)::date AS month_start FROM m_savings_account_transaction t JOIN m_savings_account a ON a.id = t.savings_account_id JOIN m_savings_product p ON p.id = a.product_id JOIN m_office o ON o.id = t.office_id LEFT JOIN m_client c ON c.id = a.client_id WHERE t.transaction_type_enum = 1 AND NOT t.is_reversed AND t.transaction_date BETWEEN $1::date AND $2::date AND t.office_id = ANY($3::bigint[]) GROUP BY 2) monthly",
        ).bind(dates.0).bind(dates.1).bind(office_ids).fetch_one(&app.fineract).await.unwrap().0,
        "savings_deposit_monthly_top_n" => sqlx::query_scalar::<_, Json<Value>>(
            "SELECT COALESCE(jsonb_agg(row_data ORDER BY month_start, row_number), '[]'::jsonb) FROM (SELECT jsonb_build_object('month_start', date_trunc('month', t.transaction_date)::date, 'transaction_id', t.id, 'transaction_date', t.transaction_date, 'amount', t.amount::text, 'currency_code', a.currency_code, 'office_id', t.office_id, 'office_name', o.name, 'product_id', a.product_id, 'product_name', p.name, 'client_id', c.id, 'client_display_name', c.display_name) AS row_data, date_trunc('month', t.transaction_date)::date AS month_start, row_number() OVER (PARTITION BY date_trunc('month', t.transaction_date) ORDER BY t.amount DESC, t.transaction_date DESC, t.id DESC) FROM m_savings_account_transaction t JOIN m_savings_account a ON a.id = t.savings_account_id JOIN m_savings_product p ON p.id = a.product_id JOIN m_office o ON o.id = t.office_id LEFT JOIN m_client c ON c.id = a.client_id WHERE t.transaction_type_enum = 1 AND NOT t.is_reversed AND t.transaction_date BETWEEN $1::date AND $2::date AND t.office_id = ANY($3::bigint[])) ranked WHERE row_number <= $4",
        ).bind(dates.0).bind(dates.1).bind(office_ids).bind(case.limit.unwrap()).fetch_one(&app.fineract).await.unwrap().0,
        "savings_balance_summary" => sqlx::query_scalar::<_, Json<Value>>(
            "SELECT jsonb_build_array(jsonb_build_object('account_count', COUNT(*), 'total_balance', COALESCE(SUM(a.account_balance_derived), 0)::text, 'average_balance', COALESCE(AVG(a.account_balance_derived), 0)::text, 'max_balance', COALESCE(MAX(a.account_balance_derived), 0)::text)) FROM m_savings_account a JOIN m_client c ON c.id = a.client_id JOIN m_office o ON o.id = c.office_id WHERE a.status_enum = 300 AND c.office_id = ANY($1::bigint[])",
        ).bind(office_ids).fetch_one(&app.fineract).await.unwrap().0,
        capability => panic!("missing independent query for {capability}"),
    }
}

fn assert_table_contract(
    response: &Value,
    fields: &[&str],
    max_rows: Option<usize>,
    context: &Value,
) {
    let table = &response["table"];
    let columns = table["columns"]
        .as_array()
        .unwrap_or_else(|| panic!("missing columns: {context}"));
    let keys = columns
        .iter()
        .filter_map(|column| column["key"].as_str())
        .collect::<Vec<_>>();
    for field in fields {
        assert!(keys.contains(field), "missing column {field}: {context}");
    }
    let rows = table["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("missing rows: {context}"));
    if let Some(max) = max_rows {
        assert!(rows.len() <= max, "too many rows: {context}");
    }
}

fn assert_no_legacy_empty_options_loop(response: &Value) {
    assert!(
        !(response["response_type"] == "clarification"
            && option_ids(response).is_empty()
            && response["actions"].as_array().map_or(0, Vec::len) == 0
            && response["message"] == "Please choose one of the available report options."),
        "empty-options clarification loop: {response}"
    );
}

fn assert_no_response_leak(response: &Value, markdown: &Value) {
    let payload = format!(
        "{}\n{}",
        serde_json::to_string(response).unwrap(),
        markdown.as_str().unwrap_or("")
    );
    for forbidden in [
        "SELECT ",
        "m_savings_account",
        "m_savings_account_transaction",
        "query_id",
        "graph_state",
    ] {
        assert!(
            !payload.contains(forbidden),
            "response leaked {forbidden}: {payload}"
        );
    }
}

fn option_ids(response: &Value) -> Vec<&str> {
    response["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|option| option["id"].as_str())
        .collect()
}

async fn create_session(app: &TestApp, api_key: &str, title: &str) -> String {
    let resp = app
        .post_json("/chat/sessions", Some(api_key), &json!({ "title": title }))
        .await;
    assert_eq!(resp.status(), 201);
    resp.json::<Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn create_job(app: &TestApp, api_key: &str, session_id: &str, message: &str) -> String {
    let resp = app
        .post_json(
            "/chat/jobs",
            Some(api_key),
            &json!({ "session_id": session_id, "message": message }),
        )
        .await;
    assert_eq!(resp.status(), 201, "create job failed for {message}");
    resp.json::<Value>().await.unwrap()["data"]["job_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn wait_until_not_running(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        let resp = app
            .get(&format!("/chat/jobs/{job_id}"), Some(api_key))
            .await;
        assert_eq!(resp.status(), 200);
        let job = resp.json::<Value>().await.unwrap()["data"].clone();
        if !matches!(job["status"].as_str().unwrap_or(""), "queued" | "running") {
            return job;
        }
        if Instant::now() >= deadline {
            panic!("job did not leave queued/running: {job}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
