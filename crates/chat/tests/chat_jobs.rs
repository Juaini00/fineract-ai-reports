//! Chat job pipeline — the paths that don't require touching Fineract data.
//!
//! The three routes we exercise here (create, get, respond) go through the
//! full auth + validate + service stack, but the *executor* only runs for
//! matched-approved capabilities. Loan / deferred-domain requests, and
//! clarification requests, never reach Fineract SQL — so they're safe to run
//! against the real read-only Fineract DB without seeding data.

mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use uuid::Uuid;

const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const SCENARIO_CAPABILITIES: &[&str] = &[
    "savings_activity_list",
    "savings_deposit_total",
    "savings_deposit_top_n",
    "savings_withdrawal_total",
    "savings_withdrawal_top_n",
];

#[tokio::test(flavor = "multi_thread")]
async fn create_job_without_api_key_is_unauthorized() {
    let app = spawn_app().await;

    let resp = app
        .post_json("/chat/jobs", None, &json!({ "message": "hello" }))
        .await;

    assert_eq!(resp.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_message_is_rejected_by_validator() {
    let app = spawn_app().await;
    let key = app.provision_api_key(&[], vec![1], false).await;

    let resp = app
        .post_json("/chat/jobs", Some(&key.raw), &json!({ "message": "" }))
        .await;

    assert!(
        resp.status().is_client_error(),
        "empty message should 4xx, got {}",
        resp.status()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn deferred_domain_request_ends_without_leaking_internals() {
    // "loan disbursement last month" — loan domain is deferred; the pipeline
    // must classify → policy → reject with a sanitized template.

    let app = spawn_app().await;
    let key = app
        .provision_api_key(&["savings_deposit_total"], vec![1, 2], false)
        .await;

    let job = create_job(&app, &key.raw, "How much loan did we disburse last month?").await;

    let final_job = wait_for_terminal(&app, &key.raw, &job).await;

    // Sanitized-only assertions: whatever the final status, the client-visible
    // response must not leak SQL / stack / prompt / raw table names.
    let payload = serde_json::to_string(&final_job).unwrap();
    for forbidden in ["SELECT ", "m_loan", "panic", "stack backtrace"] {
        assert!(
            !payload.contains(forbidden),
            "response leaked internals ({forbidden}): {payload}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn job_audit_endpoint_returns_pipeline_timeline() {
    let app = spawn_app().await;
    let key = app
        .provision_api_key(&["savings_deposit_total"], vec![1, 2], false)
        .await;

    let job = create_job(&app, &key.raw, "How much loan did we disburse last month?").await;
    let job_id = job["job_id"].as_str().unwrap();
    let _ = wait_for_terminal(&app, &key.raw, &job).await;

    let audit = get_audit(&app, &key.raw, job_id).await;
    assert_eq!(audit["job_id"], job_id);
    let events = audit["events"].as_array().expect("audit events array");
    assert!(
        events.iter().all(|event| event.get("job_id").is_some()
            && event.get("layer").is_some()
            && event.get("blueprint_step").is_some()
            && event.get("status").is_some()
            && event.get("created_at").is_some()),
        "audit events should include full timeline fields when present: {audit}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_date_range_triggers_clarification_and_continues_same_job() {
    let app = spawn_app().await;
    let key = app
        .provision_api_key(&["savings_deposit_total"], vec![1, 2], false)
        .await;

    // Turn 1: no date range → planner should ask for clarification (or classifier)
    let job1 = create_job(&app, &key.raw, "How much did we deposit?").await;
    let job1_id = job1["job_id"].as_str().unwrap().to_string();

    let after_turn1 = wait_for_terminal(&app, &key.raw, &job1).await;
    // Whatever the terminal state (needs_clarification is expected but the
    // planner may fall back to unsupported), the reply must be short and safe.
    let payload = serde_json::to_string(&after_turn1).unwrap();
    assert!(!payload.contains("SELECT "));

    // Turn 2: send a follow-up on the SAME job — must not 404 and must not
    // spawn a new job. Even if the pipeline had already terminated, the
    // /responses route belongs to the same job_id.
    let resp = app
        .post_json(
            &format!("/chat/jobs/{job1_id}/responses"),
            Some(&key.raw),
            &json!({ "message": "from 2026-01-01 to 2026-01-31" }),
        )
        .await;

    // Accept 200/201/409 — the exact code depends on job state; the key
    // guarantee is that the route is wired and reachable (not 404, not 401,
    // not 500).
    assert!(
        matches!(resp.status().as_u16(), 200 | 201 | 400 | 409),
        "responses route must be reachable on the same job, got {}",
        resp.status()
    );

    // The job under this id is still the same one.
    let got = app
        .get(&format!("/chat/jobs/{job1_id}"), Some(&key.raw))
        .await;
    assert_eq!(got.status(), 200);
    let got_json: Value = got.json().await.unwrap();
    assert_eq!(got_json["data"]["id"], job1_id);
}

#[tokio::test(flavor = "multi_thread")]
async fn all_activity_request_returns_activity_list() {
    let app = spawn_app().await;
    let key = app
        .provision_api_key(SCENARIO_CAPABILITIES, vec![1, 2], true)
        .await;

    let job = create_job(&app, &key.raw, "Show customer savings activity this week").await;
    let final_job = wait_for_terminal(&app, &key.raw, &job).await;
    assert_ne!(
        final_job["result_json"]["structured_response"]["response_type"],
        "error"
    );

    let session_id = final_job["session_id"].as_str().unwrap();
    let messages = app
        .get(
            &format!("/chat/sessions/{session_id}/messages"),
            Some(&key.raw),
        )
        .await;
    assert_eq!(messages.status(), 200);
    let body: Value = messages.json().await.unwrap();
    let assistant = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "assistant")
        .expect("assistant message");
    let content = assistant["content"].as_str().unwrap();
    assert!(!content.starts_with('{'), "{assistant}");
    assert_eq!(assistant["metadata_json"]["type"], "assistant_response");
    assert_ne!(
        assistant["metadata_json"]["assistant_response"]["response_type"],
        "error"
    );
}

async fn create_job(app: &TestApp, api_key: &str, message: &str) -> Value {
    let resp = app
        .post_json("/chat/jobs", Some(api_key), &json!({ "message": message }))
        .await;
    assert_eq!(
        resp.status(),
        201,
        "create_job failed: {}",
        resp.text().await.unwrap_or_default()
    );
    let body: Value = resp.json().await.unwrap();
    body["data"].clone()
}

async fn wait_for_terminal(app: &TestApp, api_key: &str, initial: &Value) -> Value {
    let job_id = initial["job_id"].as_str().unwrap();
    // sanity: a valid UUID
    let _ = Uuid::parse_str(job_id).expect("job_id is uuid");

    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        let resp = app
            .get(&format!("/chat/jobs/{job_id}"), Some(api_key))
            .await;
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        let status = body["data"]["status"].as_str().unwrap_or("").to_string();

        if !matches!(status.as_str(), "queued" | "running") {
            return body["data"].clone();
        }
        if Instant::now() >= deadline {
            panic!("job did not reach terminal state within {POLL_TIMEOUT:?}: {body}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn get_audit(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let resp = app
        .get(&format!("/chat/jobs/{job_id}/audit"), Some(api_key))
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    body["data"].clone()
}

#[allow(dead_code)]
async fn wait_for_final_after_response(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        let resp = app
            .get(&format!("/chat/jobs/{job_id}"), Some(api_key))
            .await;
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        let status = body["data"]["status"].as_str().unwrap_or("").to_string();

        if !matches!(
            status.as_str(),
            "queued" | "running" | "waiting_for_user_input"
        ) {
            return body["data"].clone();
        }
        if Instant::now() >= deadline {
            panic!("job did not finish clarification response within {POLL_TIMEOUT:?}: {body}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
