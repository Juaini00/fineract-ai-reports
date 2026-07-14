//! End-to-end integration test for the ambiguous-prompt user journey.
//!
//! Walks the FULL chat flow the way a real client does:
//!
//!   1. POST /chat/sessions          → session_id
//!   2. POST /chat/jobs              → job_id (with ambiguous prompt)
//!   3. GET  /chat/jobs/{id}         → poll until terminal (clarification)
//!   4. GET  /chat/jobs/{id}/stream  → SSE reachability check
//!   5. POST /chat/jobs/{id}/responses  → send free-text clarification
//!   6. GET  /chat/jobs/{id}         → **assert the system is NOT stuck in a loop**
//!   7. GET  /chat/sessions/{id}/messages → the message trail is complete
//!
//! The previous integration tests were too permissive about empty
//! clarification options. This test asserts graph-runtime semantics:
//!
//!   * Assistant output must be stored in `result_json.structured_response`.
//!   * After the user's free-text reply, the system MUST NOT be in the shape
//!     `clarification == "Please choose one of the available report options."`
//!     WITH empty options — that specific combination is the loop.

mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const AMBIGUOUS_PROMPT: &str = "Show customer savings activity this week";
const FREE_TEXT_REPLY: &str = "total savings deposits this week";
const CLIENT_BALANCE_CAP: &str = "client_top_n_by_savings_balance";
const CLIENT_OPTION_IDS: &[&str] = &[
    "client_top_n_by_savings_account_count",
    CLIENT_BALANCE_CAP,
    "client_top_n_by_deposit_volume",
    "others",
];

const CAPS: &[&str] = &[
    "savings_deposit_total",
    "savings_deposit_top_n",
    "savings_deposit_monthly_breakdown",
    "savings_withdrawal_total",
    // ponytail: include activity_list so retrieval can surface an intent-appropriate
    // capability for "activity" prompts instead of depending on exact wording.
    "savings_activity_list",
];

#[tokio::test(flavor = "multi_thread")]
async fn ambiguous_prompt_free_text_reply_never_loops() {
    let app = spawn_app().await;
    let key = app.provision_api_key(CAPS, vec![1, 2, 3], true).await;

    // ---------- 1. Create session ----------
    let sess_resp = app
        .post_json(
            "/chat/sessions",
            Some(&key.raw),
            &json!({ "title": "ambiguous flow" }),
        )
        .await;
    assert_eq!(sess_resp.status(), 201);
    let session_id = sess_resp.json::<Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // ---------- 2. Create job with ambiguous prompt ----------
    let job_resp = app
        .post_json(
            "/chat/jobs",
            Some(&key.raw),
            &json!({ "session_id": session_id, "message": AMBIGUOUS_PROMPT }),
        )
        .await;
    assert_eq!(job_resp.status(), 201);
    let job = job_resp.json::<Value>().await.unwrap()["data"].clone();
    let job_id = job["job_id"].as_str().unwrap().to_string();

    // ---------- 3. Poll until Turn-1 terminal ----------
    let after_turn1 = wait_until_not_running(&app, &key.raw, &job_id).await;
    let status1 = after_turn1["status"].as_str().unwrap_or("");

    // Path A: semantic assistant completed directly.
    if status1 == "completed" {
        let result = &after_turn1["result_json"];
        let cap = result["selected_capability"].as_str().unwrap_or("");
        assert!(
            cap.starts_with("savings_"),
            "Turn-1 completed but matched non-savings capability {cap}: {after_turn1}"
        );
        assert!(
            result["structured_response"].is_object(),
            "missing structured response: {after_turn1}"
        );
        assert_no_legacy_empty_options_loop(result);
        assert_no_internal_markdown_leak(result);
        return;
    }

    // Path B: clarification. Reply in natural language and assert NO loop.
    assert_eq!(
        status1, "waiting_for_user_input",
        "Ambiguous prompt must either match or clarify — never leak intermediate state. Got status={status1}, job={after_turn1}"
    );

    let result1 = &after_turn1["result_json"];
    assert!(
        result1["structured_response"].is_object(),
        "missing structured response: {after_turn1}"
    );
    assert_no_legacy_empty_options_loop(result1);

    // ---------- 4. SSE reachability check ----------
    let stream = app
        .get(&format!("/chat/jobs/{job_id}/stream"), Some(&key.raw))
        .await;
    assert!(
        stream.status().is_success() || stream.status().as_u16() == 204,
        "SSE stream endpoint must be reachable, got {}",
        stream.status()
    );

    // ---------- 5. Send natural-language clarification ----------
    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(&key.raw),
            &json!({ "message": FREE_TEXT_REPLY }),
        )
        .await;
    assert_eq!(
        resp.status(),
        201,
        "Clarification POST /responses must succeed: {}",
        resp.text().await.unwrap_or_default()
    );

    // ---------- 6. STRICT: same job must remain reachable and not loop ----------
    let after_turn2 = wait_until_not_running(&app, &key.raw, &job_id).await;
    let result2 = &after_turn2["result_json"];
    assert!(
        result2["structured_response"].is_object(),
        "missing structured response: {after_turn2}"
    );
    assert_no_legacy_empty_options_loop(result2);
    assert_no_internal_markdown_leak(result2);

    // ---------- 7. Message trail complete ----------
    let msgs = app
        .get(
            &format!("/chat/sessions/{session_id}/messages"),
            Some(&key.raw),
        )
        .await
        .json::<Value>()
        .await
        .unwrap();
    let arr = msgs["data"].as_array().expect("messages array");
    assert!(
        arr.len() >= 3,
        "Expected at least user + assistant + user messages, got {} — {msgs}",
        arr.len()
    );
    // Ordering sanity: first is the ambiguous prompt from user.
    assert_eq!(arr[0]["role"], "user");
    assert_eq!(arr[0]["content"], AMBIGUOUS_PROMPT);
    assert!(
        arr.iter().any(|msg| msg["role"] == "assistant"
            && msg["metadata_json"]["type"] == "assistant_response"),
        "assistant_response metadata missing: {msgs}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn wildcard_key_option_id_response_executes_same_job() {
    let app = spawn_app().await;
    let key = app.provision_wildcard_api_key(true).await;

    let sess_resp = app
        .post_json(
            "/chat/sessions",
            Some(&key.raw),
            &json!({ "title": "wildcard option id flow" }),
        )
        .await;
    assert_eq!(sess_resp.status(), 201);
    let session_id = sess_resp.json::<Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let job_resp = app
        .post_json(
            "/chat/jobs",
            Some(&key.raw),
            &json!({ "session_id": session_id, "message": "show 10 clients with the most savings accounts" }),
        )
        .await;
    assert_eq!(job_resp.status(), 201);
    let job_id = job_resp.json::<Value>().await.unwrap()["data"]["job_id"]
        .as_str()
        .unwrap()
        .to_string();

    let after_turn1 = wait_until_not_running(&app, &key.raw, &job_id).await;
    let status1 = after_turn1["status"].as_str().unwrap_or("");

    if status1 == "completed" {
        let result = &after_turn1["result_json"];
        let cap = result["selected_capability"].as_str().unwrap_or("");
        assert_eq!(
            cap, "client_top_n_by_savings_account_count",
            "{after_turn1}"
        );
        assert!(
            result["structured_response"].is_object(),
            "missing structured response: {after_turn1}"
        );
        assert_table_rows_at_most(&result["structured_response"], 10, &after_turn1);
        let memory_summary: Value = sqlx::query_scalar(
            "SELECT execution_summary_json FROM assistant_job_memory WHERE job_id = $1::uuid",
        )
        .bind(&job_id)
        .fetch_one(&app.app_pool)
        .await
        .unwrap();
        let limit = memory_summary["plan"]["params"]["limit"].as_i64();
        assert_eq!(limit, Some(10), "missing preserved limit: {after_turn1}");
        assert_ne!(result["policy_blocked"].as_bool(), Some(true));
        assert_non_empty_office_scope_if_present(&after_turn1);
        return;
    }

    assert_eq!(
        status1, "waiting_for_user_input",
        "first turn must complete or clarify: {after_turn1}"
    );
    let response1 = &after_turn1["result_json"]["structured_response"];
    assert_eq!(response1["response_type"], "clarification", "{after_turn1}");
    let option_ids = option_ids(response1);
    for expected in CLIENT_OPTION_IDS {
        assert!(
            option_ids.iter().any(|id| id == expected),
            "missing option {expected}: {response1}"
        );
    }
    assert!(
        option_ids.iter().all(|id| !id.starts_with("refine_")),
        "refine option leaked: {response1}"
    );

    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(&key.raw),
            &json!({ "message": "Rank clients by total savings balance.", "option_id": CLIENT_BALANCE_CAP }),
        )
        .await;
    assert_eq!(
        resp.status(),
        201,
        "option-id response failed: {}",
        resp.text().await.unwrap_or_default()
    );
    let stored_response: (String, Value) = sqlx::query_as(
        "SELECT content, metadata_json FROM chat_messages WHERE job_id = $1::uuid AND role = 'clarification' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&job_id)
    .fetch_one(&app.app_pool)
    .await
    .unwrap();
    assert_eq!(stored_response.0, "Rank clients by total savings balance.");
    assert_eq!(stored_response.1["selected_option_id"], CLIENT_BALANCE_CAP);
    assert_eq!(
        stored_response.1["source_message"],
        "Rank clients by total savings balance."
    );

    let after_turn2 = wait_until_not_running(&app, &key.raw, &job_id).await;
    assert_ne!(
        after_turn2["status"].as_str(),
        Some("waiting_for_user_input")
    );
    let result2 = &after_turn2["result_json"];
    assert_eq!(result2["selected_capability"], CLIENT_BALANCE_CAP);
    let memory_summary: Value = sqlx::query_scalar(
        "SELECT execution_summary_json FROM assistant_job_memory WHERE job_id = $1::uuid",
    )
    .bind(&job_id)
    .fetch_one(&app.app_pool)
    .await
    .unwrap();
    let limit = memory_summary["plan"]["params"]["limit"].as_i64();
    assert_eq!(limit, Some(10), "missing preserved limit: {after_turn2}");
    assert_ne!(
        result2["structured_response"]["response_type"].as_str(),
        Some("clarification")
    );
    assert_table_rows_at_most(&result2["structured_response"], 10, &after_turn2);
    assert_not_not_allowed(result2);
    assert_non_empty_office_scope_if_present(&after_turn2);
}

#[tokio::test(flavor = "multi_thread")]
async fn domain_prompt_matrix_returns_expected_contracts() {
    let app = spawn_app().await;
    let key = app.provision_wildcard_api_key(true).await;
    let session_id = create_session(&app, &key.raw, "domain matrix").await;

    for (prompt, supported, expected) in [
        (
            "I want to see top clients by savings account count",
            true,
            &["client_top_n_by_savings_account_count"][..],
        ),
        (
            "I want to see office savings summary",
            true,
            &["organization_office_savings_summary"][..],
        ),
        (
            "I want to see savings deposit report",
            true,
            &[
                "savings_deposit_total",
                "savings_deposit_top_n",
                "client_top_n_by_deposit_volume",
            ][..],
        ),
        ("I want to see loan portfolio report", false, &[][..]),
        ("I want to see savings charges and fees", false, &[][..]),
        ("I want to see tax report", false, &[][..]),
        ("I want to see accounting GL journal report", false, &[][..]),
    ] {
        let job_id = create_job(&app, &key.raw, &session_id, prompt).await;
        let job = wait_until_not_running(&app, &key.raw, &job_id).await;
        let result = &job["result_json"];
        let response = &result["structured_response"];

        if supported {
            assert_ne!(
                result["policy_blocked"].as_bool(),
                Some(true),
                "{prompt}: {job}"
            );
            assert_ne!(
                response["response_type"].as_str(),
                Some("error"),
                "{prompt}: {job}"
            );
            match job["status"].as_str().unwrap_or("") {
                "completed" => {
                    let cap = result["selected_capability"].as_str().unwrap_or("");
                    assert!(expected.contains(&cap), "{prompt}: {job}");
                }
                "waiting_for_user_input" => {
                    let ids = option_ids(response);
                    assert!(
                        expected.iter().any(|id| ids.iter().any(|got| got == id)),
                        "{prompt}: {response}"
                    );
                }
                status => panic!("{prompt}: unexpected status {status}: {job}"),
            }
        } else {
            assert_eq!(job["status"].as_str(), Some("completed"), "{prompt}: {job}");
            assert!(
                response["response_type"].as_str() == Some("unsupported")
                    || response["response_type"].as_str() == Some("out_of_domain"),
                "{prompt}: {job}"
            );
            assert_no_sql_or_table_leak(&job);
            assert!(response["table"].is_null(), "{prompt}: table leaked: {job}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_step_clarification_others_then_free_text_does_not_repeat_same_options() {
    let app = spawn_app().await;
    let key = app.provision_wildcard_api_key(true).await;
    let session_id = create_session(&app, &key.raw, "multi clarification").await;

    let (job_id, first) = first_clarification_job(&app, &key.raw, &session_id).await;
    let first_ids = option_ids(&first["result_json"]["structured_response"])
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert!(!first_ids.is_empty(), "missing first options: {first}");

    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(&key.raw),
            &json!({ "message": "others" }),
        )
        .await;
    assert_eq!(resp.status(), 201, "others response failed");

    let after_others = wait_until_not_running(&app, &key.raw, &job_id).await;
    let others_response = &after_others["result_json"]["structured_response"];
    assert_eq!(
        others_response["response_type"], "clarification",
        "{after_others}"
    );
    let others_ids = option_ids(others_response);
    assert!(
        others_ids.is_empty()
            || others_ids != first_ids.iter().map(String::as_str).collect::<Vec<_>>(),
        "same options repeated after others: {after_others}"
    );

    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(&key.raw),
            &json!({ "message": "show top clients by savings account count" }),
        )
        .await;
    assert_eq!(resp.status(), 201, "free text response failed");

    let final_job = wait_until_not_running(&app, &key.raw, &job_id).await;
    let final_ids = option_ids(&final_job["result_json"]["structured_response"]);
    assert_ne!(
        final_ids,
        first_ids.iter().map(String::as_str).collect::<Vec<_>>(),
        "same options repeated after free text: {final_job}"
    );
}

fn assert_no_legacy_empty_options_loop(result: &Value) {
    let response = &result["structured_response"];
    let response_type = response["response_type"].as_str().unwrap_or("");
    let message = response["message"].as_str().unwrap_or("");
    let options_len = response["options"]
        .as_array()
        .map_or(0, |options| options.len());
    let actions_len = response["actions"]
        .as_array()
        .map_or(0, |actions| actions.len());
    assert!(
        !(response_type == "clarification"
            && options_len == 0
            && actions_len == 0
            && message == "Please choose one of the available report options."),
        "empty-options clarification loop shape: {result}"
    );
}

fn assert_no_internal_markdown_leak(result: &Value) {
    let markdown = result["markdown"].as_str().unwrap_or("");
    assert!(
        !markdown.contains("graph_state") && !markdown.contains("selected_capability"),
        "markdown leaked internal runtime fields: {markdown}"
    );
}

fn assert_no_sql_or_table_leak(value: &Value) {
    let payload = serde_json::to_string(value).unwrap();
    for forbidden in [
        "SELECT ",
        "m_savings_account",
        "m_loan",
        "m_charge",
        "acc_gl",
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

fn assert_not_not_allowed(result: &Value) {
    let markdown = result["markdown"].as_str().unwrap_or("");
    let message = result["structured_response"]["message"]
        .as_str()
        .unwrap_or("");
    assert!(
        !markdown.contains("not allowed to run capability")
            && !message.contains("not allowed to run capability"),
        "authorization denial leaked: {result}"
    );
}

fn assert_non_empty_office_scope_if_present(job: &Value) {
    assert_no_empty_office_ids(&job["state_json"], "state_json");
    assert_no_empty_office_ids(&job["result_json"], "result_json");
}

fn assert_table_rows_at_most(response: &Value, limit: usize, context: &Value) {
    let table = &response["table"];
    if !table.is_null() {
        let rows = table["rows"]
            .as_array()
            .unwrap_or_else(|| panic!("missing table rows: {context}"));
        assert!(rows.len() <= limit, "too many table rows: {context}");
    }
}

fn assert_no_empty_office_ids(value: &Value, label: &str) {
    match value {
        Value::Array(items) => {
            for item in items {
                assert_no_empty_office_ids(item, label);
            }
        }
        Value::Object(map) => {
            for (key, child) in map {
                if key == "office_ids"
                    && let Some(ids) = child.as_array()
                {
                    assert!(!ids.is_empty(), "empty office_ids in {label}: {value}");
                }
                assert_no_empty_office_ids(child, label);
            }
        }
        _ => {}
    }
}

async fn fetch_job(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let resp = app
        .get(&format!("/chat/jobs/{job_id}"), Some(api_key))
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    body["data"].clone()
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

async fn first_clarification_job(
    app: &TestApp,
    api_key: &str,
    session_id: &str,
) -> (String, Value) {
    for prompt in ["show report", "show savings report"] {
        let job_id = create_job(app, api_key, session_id, prompt).await;
        let job = wait_until_not_running(app, api_key, &job_id).await;
        if job["status"].as_str() == Some("waiting_for_user_input") {
            return (job_id, job);
        }
    }
    panic!("could not produce clarification job");
}

async fn wait_until_not_running(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        let job = fetch_job(app, api_key, job_id).await;
        let status = job["status"].as_str().unwrap_or("").to_string();
        if !matches!(status.as_str(), "queued" | "running") {
            return job;
        }
        if Instant::now() >= deadline {
            panic!("job did not leave queued/running: {job}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
