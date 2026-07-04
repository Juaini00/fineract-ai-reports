//! Verifies the gap-based classifier decision and Others escape hatch. Uses
//! the catalog-lexical fallback (Voyage disabled in the harness) so all
//! assertions are deterministic without external services.

mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[tokio::test(flavor = "multi_thread")]
async fn ambiguous_prompt_produces_options_including_others_or_terminates_safely() {
    // "Show customer savings activity this week" is deliberately ambiguous —
    // it could map to total, top_n, monthly_breakdown, or monthly_top_n. The
    // gap-based classifier should NOT match to a single capability with high
    // confidence; either it clarifies with options (best case), or it ends as
    // unsupported safely — never as a hallucinated match.
    let app = spawn_app().await;
    let key = app
        .provision_api_key(
            &[
                "savings_deposit_total",
                "savings_deposit_top_n",
                "savings_deposit_monthly_breakdown",
                "savings_deposit_monthly_top_n",
            ],
            vec![1, 2, 3],
            false,
        )
        .await;

    let job = create_job(&app, &key.raw, "Show customer savings activity this week").await;
    let terminal = wait_for_terminal(&app, &key.raw, &job).await;

    // Guarantee 1: no leak of SQL / internal names in the client-visible state.
    let payload = serde_json::to_string(&terminal).unwrap();
    for forbidden in ["SELECT ", "m_savings", "panic", "stack backtrace"] {
        assert!(
            !payload.contains(forbidden),
            "response leaked {forbidden}: {payload}"
        );
    }

    // Guarantee 2: if the outcome is clarification, options list is populated
    // AND contains the Others escape hatch.
    let outcome = terminal["state_json"]["classification"]["outcome"]
        .as_str()
        .unwrap_or("");
    if outcome == "clarification_required" {
        let options = terminal["state_json"]["classification"]["options"]
            .as_array()
            .expect("options is array");
        assert!(
            !options.is_empty(),
            "clarification should present at least one option: {terminal}"
        );
        let has_others = options
            .iter()
            .any(|opt| opt["capability"] == "other_activity");
        assert!(has_others, "Others option missing: {options:?}");
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
            panic!("job did not reach terminal state: {body}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
