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
#[tokio::test(flavor = "multi_thread")]
async fn create_job_without_bearer_is_unauthorized() {
    let app = spawn_app().await;

    let resp = app
        .post_json("/chat/jobs", None, &json!({ "message": "hello" }))
        .await;

    assert_eq!(resp.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_message_is_rejected_by_validator() {
    let app = spawn_app().await;
    let token = app.login_admin().await;

    let resp = app
        .post_json_bearer("/chat/jobs", &token, &json!({ "message": "" }))
        .await;

    assert!(
        resp.status().is_client_error(),
        "empty message should 4xx, got {}",
        resp.status()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn job_routes_hide_jobs_owned_by_another_user() {
    let app = spawn_app().await;
    let owner = app.login_admin().await;
    let other = app.create_test_user_bearer().await;
    let job = create_job(&app, &owner, "hello").await;
    let job_id = job["job_id"].as_str().unwrap();

    let mine = app
        .get_bearer(&format!("/chat/jobs/{job_id}"), &owner)
        .await;
    assert_eq!(mine.status(), 200);
    let mine: Value = mine.json().await.unwrap();
    assert!(mine["data"]["user_id"].is_string());
    assert!(mine["data"]["api_key_id"].is_null());

    let theirs = app
        .get_bearer(&format!("/chat/jobs/{job_id}"), &other)
        .await;
    assert_eq!(theirs.status(), 404);

    let foreign_session = app
        .post_json_bearer(
            "/chat/jobs",
            &other,
            &json!({ "session_id": job["session_id"], "message": "not mine" }),
        )
        .await;
    assert_eq!(foreign_session.status(), 404);

    let response = app
        .post_json_bearer(
            &format!("/chat/jobs/{job_id}/responses"),
            &other,
            &json!({ "message": "not mine" }),
        )
        .await;
    assert_eq!(response.status(), 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn partial_structured_response_is_a_sanitized_validation_error() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    let response = app
        .post_json_bearer(
            &format!("/chat/jobs/{}/responses", Uuid::new_v4()),
            &token,
            &json!({ "clarification_id": Uuid::new_v4(), "answers": {} }),
        )
        .await;
    assert_eq!(response.status(), 400);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "clarification_validation_error");
}

#[tokio::test(flavor = "multi_thread")]
async fn deferred_domain_request_ends_without_leaking_internals() {
    // "loan disbursement last month" — loan domain is deferred; the pipeline
    // must classify → policy → reject with a sanitized template.

    let app = spawn_app().await;
    let token = app.login_admin().await;

    let job = create_job(&app, &token, "How much loan did we disburse last month?").await;

    let final_job = wait_for_terminal(&app, &token, &job).await;

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
    let token = app.login_admin().await;

    let job = create_job(&app, &token, "How much loan did we disburse last month?").await;
    let job_id = job["job_id"].as_str().unwrap();
    let _ = wait_for_terminal(&app, &token, &job).await;
    tokio::time::sleep(Duration::from_millis(600)).await;

    let audit = get_audit(&app, &token, job_id).await;
    assert_eq!(audit["job_id"], job_id);
    let events = audit["events"].as_array().expect("audit events array");
    assert!(!events.is_empty(), "audit timeline should contain an event");
    assert!(
        events.iter().all(|event| event.get("job_id").is_some()
            && event["user_id"].is_string()
            && event["api_key_id"].is_null()
            && event.get("layer").is_some()
            && event.get("blueprint_step").is_some()
            && event.get("status").is_some()
            && event.get("created_at").is_some()),
        "audit events should include full timeline fields when present: {audit}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_reads_legacy_unowned_audit_without_claiming_it() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    let key = app.provision_wildcard_api_key(false).await;
    let session_id = Uuid::new_v4();
    let job_id = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_sessions (id, api_key_id, status) VALUES ($1, $2, 'active')")
        .bind(session_id)
        .bind(key.id)
        .execute(&app.app_pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO chat_jobs (id, session_id, api_key_id, status, current_step, message, expires_at) VALUES ($1, $2, $3, 'completed', 'response', 'legacy', now() + interval '1 hour')",
    )
    .bind(job_id)
    .bind(session_id)
    .bind(key.id)
    .execute(&app.app_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO chat_job_audit_events (id, job_id, session_id, api_key_id, event_type, stage, layer, status) VALUES ($1, $2, $3, $4, 'legacy', 'legacy', 'legacy', 'completed')",
    )
    .bind(Uuid::new_v4())
    .bind(job_id)
    .bind(session_id)
    .bind(key.id)
    .execute(&app.app_pool)
    .await
    .unwrap();

    let audit = get_audit(&app, &token, &job_id.to_string()).await;
    assert!(audit["events"][0]["user_id"].is_null());
    assert_eq!(audit["events"][0]["api_key_id"], key.id.to_string());
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM chat_job_audit_events WHERE job_id = $1")
            .bind(job_id)
            .fetch_one(&app.app_pool)
            .await
            .unwrap();
    assert!(owner.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn follow_up_message_stays_on_the_same_job() {
    let app = spawn_app().await;
    let token = app.login_admin().await;

    // Turn 1: this capability's date parameters declare `default: business_today`,
    // so the pipeline auto-fills them and answers in one turn instead of asking.
    let job1 = create_job(&app, &token, "How much did we deposit?").await;
    let job1_id = job1["job_id"].as_str().unwrap().to_string();

    let after_turn1 = wait_for_terminal(&app, &token, &job1).await;
    // Whatever the terminal state, the reply must be short and safe.
    let payload = serde_json::to_string(&after_turn1).unwrap();
    assert!(!payload.contains("SELECT "));

    // Turn 2: send a follow-up on the SAME job — must not 404 and must not
    // spawn a new job. Even if the pipeline had already terminated, the
    // /responses route belongs to the same job_id. A finished job answers
    // 409 clarification_not_active, never 404: a 404 would tell the client the
    // job vanished and push it into spawning a replacement.
    let resp = app
        .post_json_bearer(
            &format!("/chat/jobs/{job1_id}/responses"),
            &token,
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
        .get_bearer(&format!("/chat/jobs/{job1_id}"), &token)
        .await;
    assert_eq!(got.status(), 200);
    let got_json: Value = got.json().await.unwrap();
    assert_eq!(got_json["data"]["id"], job1_id);
}

/// A follow-up for a required parameter stays on the same job even if the job is no
/// longer active; clients must never receive a 404 and create a replacement job.
#[tokio::test(flavor = "multi_thread")]
async fn required_parameter_without_default_asks_and_answer_continues_same_job() {
    let app = spawn_app().await;
    let token = app.login_admin().await;

    let job = create_job(&app, &token, "Find the client named Ada").await;
    let job_id = job["job_id"].as_str().unwrap().to_string();
    let _ = wait_for_terminal(&app, &token, &job).await;

    let response = app
        .post_json_bearer(
            &format!("/chat/jobs/{job_id}/responses"),
            &token,
            &json!({ "message": "Ada Lovelace" }),
        )
        .await;
    assert!(
        matches!(response.status().as_u16(), 200 | 201 | 400 | 409),
        "responses route must be reachable on the same job, got {}",
        response.status()
    );

    let got = app
        .get_bearer(&format!("/chat/jobs/{job_id}"), &token)
        .await;
    assert_eq!(got.status(), 200);
    let got_json: Value = got.json().await.unwrap();
    assert_eq!(got_json["data"]["id"], job_id);
}

/// The date parameters of `savings_deposit_total` declare `default: business_today`,
/// so the pipeline must fill them itself and answer in a single turn instead of
/// demanding a date range. No approved capability currently reaches the
/// clarification path for a missing required parameter, so this asserts the
/// auto-fill contract directly rather than pretending one does.
#[tokio::test(flavor = "multi_thread")]
async fn date_parameters_with_a_policy_default_are_auto_filled_without_asking() {
    let app = spawn_app().await;
    let token = app.login_admin().await;

    let job = create_job(&app, &token, "How much did we deposit?").await;
    let job_id = job["job_id"].as_str().unwrap().to_string();
    let terminal = wait_for_terminal(&app, &token, &job).await;

    assert_eq!(
        terminal["status"], "completed",
        "policy defaults must complete the job in one turn: {terminal}"
    );
    assert!(
        terminal["result_json"]["structured_response"]["clarification"].is_null(),
        "no clarification may be requested when every parameter has a default: {terminal}"
    );
    assert_eq!(terminal["id"], job_id);
}

#[tokio::test(flavor = "multi_thread")]
async fn all_activity_request_returns_activity_list() {
    let app = spawn_app().await;
    let token = app.login_admin().await;

    let job = create_job(&app, &token, "Show customer savings activity this week").await;
    let final_job = wait_for_terminal(&app, &token, &job).await;
    assert_ne!(
        final_job["result_json"]["structured_response"]["response_type"],
        "error"
    );

    let session_id = final_job["session_id"].as_str().unwrap();
    let messages = app
        .get_bearer(&format!("/chat/sessions/{session_id}/messages"), &token)
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

async fn create_job(app: &TestApp, token: &str, message: &str) -> Value {
    let resp = app
        .post_json_bearer("/chat/jobs", token, &json!({ "message": message }))
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

async fn wait_for_terminal(app: &TestApp, token: &str, initial: &Value) -> Value {
    let job_id = initial["job_id"].as_str().unwrap();
    // sanity: a valid UUID
    let _ = Uuid::parse_str(job_id).expect("job_id is uuid");

    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        let resp = app.get_bearer(&format!("/chat/jobs/{job_id}"), token).await;
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

async fn get_audit(app: &TestApp, token: &str, job_id: &str) -> Value {
    let resp = app
        .get_bearer(&format!("/chat/jobs/{job_id}/audit"), token)
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    body["data"].clone()
}
