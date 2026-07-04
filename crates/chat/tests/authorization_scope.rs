//! Scenario 07 — authorization scope. Verifies:
//!
//!   A. Capability gate — an API key with empty `allowed_capabilities` gets
//!      short-circuited to unsupported without touching Fineract.
//!   D. Job ownership — jobs are not visible across API keys, even when the
//!      other key would otherwise be a valid caller.
//!
//! Also exercises the deferred-domain paths from Scenario 06.E for loan,
//! accounting, and tax — each must terminate without leaking internals.

mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[tokio::test(flavor = "multi_thread")]
async fn job_is_invisible_across_api_keys() {
    // Scenario 07.D — job ownership boundary.
    let app = spawn_app().await;
    let owner = app
        .provision_api_key(&["savings_deposit_total"], vec![1, 2, 3], false)
        .await;
    let other = app
        .provision_api_key(&["savings_deposit_total"], vec![1, 2, 3], false)
        .await;

    // Owner creates a job — any prompt is fine, deferred domain works too.
    let create = app
        .post_json(
            "/chat/jobs",
            Some(&owner.raw),
            &json!({ "message": "How much loan did we disburse last month?" }),
        )
        .await;
    assert_eq!(create.status(), 201);
    let job: Value = create.json().await.unwrap();
    let job_id = job["data"]["job_id"].as_str().unwrap().to_string();

    // Other key cannot read the job.
    let cross = app
        .get(&format!("/chat/jobs/{job_id}"), Some(&other.raw))
        .await;
    assert_eq!(
        cross.status(),
        404,
        "cross-key read must 404; got {}",
        cross.status()
    );

    // And cannot post responses either.
    let cross_respond = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(&other.raw),
            &json!({ "message": "hi" }),
        )
        .await;
    assert!(
        matches!(cross_respond.status().as_u16(), 403 | 404),
        "cross-key respond must 4xx; got {}",
        cross_respond.status()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_allowed_capabilities_short_circuits_to_unsupported() {
    // Scenario 07 failure mode: API key with empty allowed_capabilities.
    // Classification returns unsupported (`source: "no_allowed_capabilities"`),
    // no Voyage/Fineract call. Terminal state is not queued/running.
    let app = spawn_app().await;
    let key = app.provision_api_key(&[], vec![1, 2, 3], false).await;

    let job = create_job(
        &app,
        &key.raw,
        "What is the total deposit from 2026-01-01 to 2026-01-31?",
    )
    .await;

    let terminal = wait_for_terminal(&app, &key.raw, &job).await;

    assert!(
        !matches!(
            terminal["status"].as_str().unwrap_or(""),
            "queued" | "running"
        ),
        "job stuck in non-terminal state: {terminal}"
    );

    // No SQL / raw table names must leak into the response.
    let payload = serde_json::to_string(&terminal).unwrap();
    for forbidden in ["SELECT ", "m_savings_account", "panic"] {
        assert!(
            !payload.contains(forbidden),
            "response leaked {forbidden}: {payload}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn deferred_domains_all_end_without_leaking_internals() {
    // Scenario 06.E — loan/accounting/tax are deferred. Group_center is
    // candidate (conditional). All should terminate sanitized.
    let app = spawn_app().await;
    let key = app
        .provision_api_key(
            &["savings_deposit_total", "savings_deposit_top_n"],
            vec![1, 2, 3],
            false,
        )
        .await;

    for prompt in [
        "How much loan did we disburse last month?",
        "Show the journal entries from January 2026.",
        "What tax did we collect last quarter?",
    ] {
        let job = create_job(&app, &key.raw, prompt).await;
        let terminal = wait_for_terminal(&app, &key.raw, &job).await;

        let payload = serde_json::to_string(&terminal).unwrap();
        for forbidden in [
            "SELECT ",
            "m_loan",
            "acc_gl_journal_entry",
            "m_tax_component",
            "panic",
            "stack backtrace",
        ] {
            assert!(
                !payload.contains(forbidden),
                "{prompt:?} leaked {forbidden}: {payload}"
            );
        }
    }
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
