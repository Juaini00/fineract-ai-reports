//! Loop-detection integration tests.
//!
//! Reproduces the exact Postman flow that used to loop: ambiguous prompt →
//! clarification with N options → user replies with free text that does NOT
//! match any option → assert the assistant does NOT return the identical
//! clarification again (that's the loop).
//!
//! The core invariant is state-based, not text-based:
//!
//!   After every user reply, the next assistant message must either
//!   (a) resolve to a matched capability (the classifier understood), or
//!   (b) present a DIFFERENT clarification — different options list or
//!       different clarification text — showing the classifier re-evaluated
//!       the input rather than echoing the previous decision.
//!
//! Two consecutive identical clarifications = loop = test failure.

mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const SETTLE: Duration = Duration::from_millis(1500);

const CAPS: &[&str] = &[
    "savings_deposit_total",
    "savings_deposit_top_n",
    "savings_deposit_monthly_breakdown",
    "savings_deposit_monthly_top_n",
    "savings_withdrawal_total",
    "savings_withdrawal_top_n",
    "savings_balance_summary",
];

/// One realistic Postman-style scenario.
struct Scenario {
    label: &'static str,
    prompt: &'static str,
    /// The clarification reply the user would actually type — deliberately
    /// free-text that doesn't exactly match option labels or capability ids.
    free_text_reply: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        label: "activity_this_month_typo",
        prompt: "Show customer savings activity this month",
        free_text_reply: "all acticity for this month",
    },
    Scenario {
        label: "activity_this_week_free_text",
        prompt: "Show customer savings activity this week",
        free_text_reply: "give me the biggest deposit transactions",
    },
    Scenario {
        label: "activity_bilingual_id",
        prompt: "Lihat aktivitas tabungan bulan ini",
        free_text_reply: "total setoran bulan ini",
    },
];

#[tokio::test(flavor = "multi_thread")]
async fn ambiguous_prompt_never_loops_across_scenarios() {
    let app = spawn_app().await;
    let key = app.provision_api_key(CAPS, vec![1, 2, 3], true).await;

    for scenario in SCENARIOS {
        run_scenario(&app, &key.raw, scenario).await;
    }
}

async fn run_scenario(app: &TestApp, api_key: &str, sc: &Scenario) {
    let sess = app
        .post_json(
            "/chat/sessions",
            Some(api_key),
            &json!({ "title": sc.label }),
        )
        .await;
    assert_eq!(sess.status(), 201, "[{}] session create", sc.label);
    let session_id = sess.json::<Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let job = app
        .post_json(
            "/chat/jobs",
            Some(api_key),
            &json!({ "session_id": session_id, "message": sc.prompt }),
        )
        .await;
    assert_eq!(job.status(), 201, "[{}] job create", sc.label);
    let job_id = job.json::<Value>().await.unwrap()["data"]["job_id"]
        .as_str()
        .unwrap()
        .to_string();

    let after_turn1 = wait_until_not_running(app, api_key, &job_id).await;
    assert_graph_response_present(sc.label, &after_turn1);
    assert_no_legacy_empty_options_loop(sc.label, &after_turn1);
    // Issue 02 (retrieval-pipeline-rework): reranker may now resolve a
    // previously-ambiguous prompt on turn 1 with no clarification at all.
    // A completed job cannot loop, so the invariant holds trivially.
    if after_turn1["status"].as_str() == Some("completed") {
        return;
    }
    let before_response = after_turn1["result_json"]["structured_response"].clone();
    assert!(
        before_response["options"]
            .as_array()
            .is_some_and(|options| options.iter().any(|option| option["id"] == "others")),
        "[{}] clarification must expose Others: {}",
        sc.label,
        after_turn1
    );

    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(api_key),
            &json!({ "option_id": "others", "message": sc.free_text_reply }),
        )
        .await;
    assert!(
        matches!(resp.status().as_u16(), 200 | 201 | 400 | 409),
        "[{}] responses route must be reachable on same job, got {}: {}",
        sc.label,
        resp.status(),
        resp.text().await.unwrap_or_default()
    );

    tokio::time::sleep(SETTLE).await;

    let after_turn2 = fetch_job(app, api_key, &job_id).await;
    assert_graph_response_present(sc.label, &after_turn2);
    assert_no_legacy_empty_options_loop(sc.label, &after_turn2);
    assert_ne!(
        after_turn2["result_json"]["structured_response"], before_response,
        "[{}] dual-field Others response repeated the identical clarification",
        sc.label
    );
    assert_eq!(
        after_turn2["id"], job_id,
        "[{}] response changed job",
        sc.label
    );
}

fn assert_graph_response_present(label: &str, job: &Value) {
    let response = &job["result_json"]["structured_response"];
    assert!(
        response.is_object(),
        "[{label}] missing structured_response: {job}"
    );
    assert!(
        response["response_type"].as_str().is_some(),
        "[{label}] missing response_type: {job}"
    );
    let payload = serde_json::to_string(job).unwrap();
    for forbidden in ["SELECT ", "stack backtrace", "panic"] {
        assert!(
            !payload.contains(forbidden),
            "[{label}] leaked {forbidden}: {payload}"
        );
    }
}

fn assert_no_legacy_empty_options_loop(label: &str, job: &Value) {
    let legacy_options = job["state_json"]["classification"]["options"].as_array();
    let legacy_clarification = job["state_json"]["classification"]["clarification"].as_str();
    assert!(
        !(legacy_options.is_some_and(|options| options.is_empty())
            && legacy_clarification.is_some()),
        "[{label}] legacy empty-options classifier loop state present: {job}"
    );
}

async fn fetch_job(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let resp = app
        .get(&format!("/chat/jobs/{job_id}"), Some(api_key))
        .await;
    assert_eq!(resp.status(), 200);
    resp.json::<Value>().await.unwrap()["data"].clone()
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
