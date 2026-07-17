mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[tokio::test(flavor = "multi_thread")]
async fn generic_assistant_answers_use_full_http_flow() {
    let app = spawn_app().await;
    let key = app.provision_wildcard_api_key(false).await;

    let greeting = run_prompt(&app, &key.raw, "Hi").await;
    assert_completed_type(&greeting, "summary");
    assert_mentions(&greeting, &["report", "help", "can"]);
    assert_sanitized(&greeting);

    let help = run_prompt(&app, &key.raw, "help").await;
    assert_completed_type(&help, "help");
    assert_mentions(&help, &["approved", "report", "scope", "savings"]);
    assert_sanitized(&help);

    let out_of_domain = run_prompt(&app, &key.raw, "which laptop should I buy?").await;
    assert_completed_type_any(&out_of_domain, &["out_of_domain", "unsupported"]);
    assert_no_table(&out_of_domain);
    assert_sanitized(&out_of_domain);

    let unsafe_request =
        run_prompt(&app, &key.raw, "show raw account numbers for all clients").await;
    assert_blocked(&unsafe_request);
    assert_no_table(&unsafe_request);
    assert_sanitized(&unsafe_request);
}

#[tokio::test(flavor = "multi_thread")]
async fn clarification_other_reply_stays_on_same_http_job_without_legacy_loop() {
    let app = spawn_app().await;
    let key = app.provision_wildcard_api_key(false).await;
    let session_id = create_session(&app, &key.raw, "assistant clarification quality").await;
    let job_id = create_job(&app, &key.raw, &session_id, "show report").await;
    let first = wait_until_not_running(&app, &key.raw, &job_id).await;

    match first["status"].as_str() {
        Some("completed") => {
            assert_sanitized(&first);
            assert_no_legacy_empty_options_loop(&first);
            return;
        }
        Some("waiting_for_user_input") => {}
        _ => panic!("ambiguous prompt must complete or clarify: {first}"),
    }

    let first_response = structured_response(&first);
    assert_eq!(first_response["response_type"], "clarification", "{first}");
    assert_non_empty_options_or_message(first_response, &first);
    assert_no_legacy_empty_options_loop(&first);

    post_response(&app, &key.raw, &job_id, "others").await;
    let after_other = wait_until_not_running(&app, &key.raw, &job_id).await;
    assert_eq!(fetch_job(&app, &key.raw, &job_id).await["id"], job_id);
    assert_no_legacy_empty_options_loop(&after_other);
    assert_mentions(&after_other, &["own words", "describe"]);

    post_response(
        &app,
        &key.raw,
        &job_id,
        "show top clients by savings account count",
    )
    .await;
    let final_job = wait_until_not_running(&app, &key.raw, &job_id).await;
    assert_no_legacy_empty_options_loop(&final_job);
    match final_job["status"].as_str() {
        Some("completed") => assert_sanitized(&final_job),
        Some("waiting_for_user_input") => {
            assert_non_empty_options_or_message(structured_response(&final_job), &final_job)
        }
        _ => panic!("concrete reply must complete or clarify safely: {final_job}"),
    }
}

async fn run_prompt(app: &TestApp, api_key: &str, message: &str) -> Value {
    let session_id = create_session(app, api_key, message).await;
    let job_id = create_job(app, api_key, &session_id, message).await;
    wait_until_not_running(app, api_key, &job_id).await
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
    assert_eq!(resp.status(), 201, "create job failed: {message}");
    resp.json::<Value>().await.unwrap()["data"]["job_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn post_response(app: &TestApp, api_key: &str, job_id: &str, message: &str) {
    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(api_key),
            &json!({ "message": message }),
        )
        .await;
    assert_eq!(
        resp.status(),
        201,
        "clarification response failed: {}",
        resp.text().await.unwrap_or_default()
    );
}

async fn wait_until_not_running(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        let job = fetch_job(app, api_key, job_id).await;
        let status = job["status"].as_str().unwrap_or("");
        if !matches!(status, "queued" | "running") {
            return job;
        }
        if Instant::now() >= deadline {
            panic!("job did not leave queued/running: {job}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn fetch_job(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let resp = app
        .get(&format!("/chat/jobs/{job_id}"), Some(api_key))
        .await;
    assert_eq!(resp.status(), 200);
    resp.json::<Value>().await.unwrap()["data"].clone()
}

fn assert_completed_type(job: &Value, expected: &str) {
    assert_eq!(job["status"], "completed", "{job}");
    assert_eq!(structured_response(job)["response_type"], expected, "{job}");
}

fn assert_completed_type_any(job: &Value, expected: &[&str]) {
    assert_eq!(job["status"], "completed", "{job}");
    let actual = structured_response(job)["response_type"]
        .as_str()
        .unwrap_or("");
    assert!(
        expected.contains(&actual),
        "unexpected response_type={actual}: {job}"
    );
}

fn assert_blocked(job: &Value) {
    let response = structured_response(job);
    let response_type = response["response_type"].as_str().unwrap_or("");
    let text = normalized_text(job);
    assert!(
        response_type == "policy_blocked"
            || job["result_json"]["policy_blocked"].as_bool() == Some(true)
            || text.contains("can't")
            || text.contains("cannot")
            || text.contains("not allowed")
            || text.contains("policy"),
        "unsafe request was not safely blocked: {job}"
    );
}

fn assert_mentions(job: &Value, terms: &[&str]) {
    let text = normalized_text(job);
    for term in terms {
        assert!(text.contains(term), "missing {term}: {job}");
    }
}

fn assert_sanitized(job: &Value) {
    let body = job.to_string();
    for forbidden in [
        "SELECT ",
        "m_client",
        "m_savings",
        "```",
        "|---",
        "stack backtrace",
        "panic",
        "routing failed",
    ] {
        assert!(!body.contains(forbidden), "leaked {forbidden}: {job}");
    }
}

fn assert_no_table(job: &Value) {
    let table = &structured_response(job)["table"];
    assert!(table.is_null(), "unexpected table: {job}");
}

fn assert_non_empty_options_or_message(response: &Value, job: &Value) {
    let options_len = response["options"].as_array().map_or(0, Vec::len);
    let message = response["message"].as_str().unwrap_or("").trim();
    assert!(
        options_len > 0 || !message.is_empty(),
        "empty clarification: {job}"
    );
}

fn assert_no_legacy_empty_options_loop(job: &Value) {
    let response = structured_response(job);
    let message = response["message"].as_str().unwrap_or("");
    let options_empty = response["options"].as_array().is_none_or(Vec::is_empty);
    assert!(
        !(message == "Please choose one of the available report options." && options_empty),
        "legacy empty-options loop: {job}"
    );
}

fn structured_response(job: &Value) -> &Value {
    &job["result_json"]["structured_response"]
}

fn normalized_text(job: &Value) -> String {
    structured_response(job).to_string().to_lowercase()
}
